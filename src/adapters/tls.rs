//! ルート証明書（CA）の読み込み。
//!
//! HTTP 層が同梱のルートストアだけを信頼すると、TLS を中継するプロキシや
//! 私設 CA を導入した環境で公開サイトの検証に失敗する。実行環境のシステム
//! ルートストアを追加で読み込み、追加設定なしに検証を通せるようにする。
//!
//! 参照先は環境変数（`SSL_CERT_FILE` / `CURL_CA_BUNDLE` / `REQUESTS_CA_BUNDLE` /
//! `SSL_CERT_DIR`）と、OS が標準的に備える証明書束である。環境変数はポート
//! `Env` 経由で読み、テストでは差し替えられるようにする。

use std::path::{Path, PathBuf};

use crate::core::errors::{AppError, ErrorCode};
use crate::ports::Env;

/// 証明書ファイルを指す環境変数（先勝ち）。
pub const BUNDLE_ENV_VARS: &[&str] = &["SSL_CERT_FILE", "CURL_CA_BUNDLE", "REQUESTS_CA_BUNDLE"];

/// 証明書ディレクトリを指す環境変数（先勝ち）。
pub const DIR_ENV_VARS: &[&str] = &["SSL_CERT_DIR"];

/// OS が標準的に備える証明書束（先勝ち）。
pub const BUNDLE_PATHS: &[&str] = &[
    "/etc/ssl/certs/ca-certificates.crt",
    "/etc/pki/tls/certs/ca-bundle.crt",
    "/etc/pki/tls/cert.pem",
    "/etc/ssl/cert.pem",
    "/etc/ssl/ca-bundle.pem",
    "/usr/local/share/certs/ca-root-nss.crt",
];

/// OS が標準的に備える証明書ディレクトリ（先勝ち）。
pub const DIR_PATHS: &[&str] = &["/etc/ssl/certs", "/etc/pki/tls/certs"];

/// 1 ディレクトリから読む最大ファイル数。
const MAX_DIR_FILES: usize = 512;

const PEM_MARKER: &[u8] = b"-----BEGIN CERTIFICATE-----";

/// 読み込んだルート証明書と、その由来（表示・検証用）。
#[derive(Debug, Clone, Default)]
pub struct Roots {
    /// 由来の表示（例: `/etc/ssl/certs/ca-certificates.crt`）。
    pub sources: Vec<String>,
    certs: Vec<reqwest::Certificate>,
}

impl Roots {
    pub fn certs(&self) -> &[reqwest::Certificate] {
        &self.certs
    }

    pub fn len(&self) -> usize {
        self.certs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.certs.is_empty()
    }

    fn extend(&mut self, source: &str, certs: Vec<reqwest::Certificate>) {
        if certs.is_empty() {
            return;
        }
        self.sources.push(source.to_string());
        self.certs.extend(certs);
    }
}

/// プロファイルの `tls.caFile` と実行環境のシステムルートストアを読み込む。
///
/// `ca_file` は最優先で必ず信頼する対象へ加える。読み込みに失敗した場合は
/// 検証を緩めないよう `E2008` を返す。システムルートストアは見つからない
/// 場合もエラーにしない（同梱のルートストアで検証を続ける）。
pub fn load(env: &dyn Env, ca_file: &str) -> Result<Roots, AppError> {
    let mut roots = Roots::default();

    let ca_file = ca_file.trim();
    if !ca_file.is_empty() {
        let path = PathBuf::from(ca_file);
        let bytes = std::fs::read(&path).map_err(|e| {
            AppError::with_detail(
                ErrorCode::E2008,
                format!("tls.caFile を読み込めない（{}）: {}", path.display(), e),
            )
        })?;
        let certs = parse_bundle(&bytes).map_err(|e| {
            AppError::with_detail(
                ErrorCode::E2008,
                format!("tls.caFile の解析に失敗（{}）: {}", path.display(), e),
            )
        })?;
        if certs.is_empty() {
            return Err(AppError::with_detail(
                ErrorCode::E2008,
                format!("tls.caFile に証明書が無い（{}）", path.display()),
            ));
        }
        roots.extend(&path.display().to_string(), certs);
    }

    let mut found_bundle = false;
    for path in candidate_bundles(env) {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(certs) = parse_bundle(&bytes) else {
            continue;
        };
        if certs.is_empty() {
            continue;
        }
        roots.extend(&path.display().to_string(), certs);
        found_bundle = true;
        break;
    }

    if !found_bundle {
        for dir in candidate_dirs(env) {
            let certs = read_dir_certificates(&dir);
            if certs.is_empty() {
                continue;
            }
            roots.extend(&dir.display().to_string(), certs);
            break;
        }
    }

    Ok(roots)
}

/// 証明書束の候補（環境変数の指す先が優先）。
pub fn candidate_bundles(env: &dyn Env) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for name in BUNDLE_ENV_VARS {
        if let Some(value) = env.var(name) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                out.push(PathBuf::from(trimmed));
            }
        }
    }
    for path in BUNDLE_PATHS {
        out.push(PathBuf::from(*path));
    }
    out
}

/// 証明書ディレクトリの候補（環境変数の指す先が優先）。
pub fn candidate_dirs(env: &dyn Env) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for name in DIR_ENV_VARS {
        if let Some(value) = env.var(name) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                out.push(PathBuf::from(trimmed));
            }
        }
    }
    for path in DIR_PATHS {
        out.push(PathBuf::from(*path));
    }
    out
}

/// 1 つのディレクトリから証明書を集める（読めないファイルは無視する）。
fn read_dir_certificates(dir: &Path) -> Vec<reqwest::Certificate> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten().take(MAX_DIR_FILES) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if let Ok(certs) = parse_bundle(&bytes) {
            out.extend(certs);
        }
    }
    out
}

/// PEM の束（または 1 件の DER）から証明書を取り出す。
fn parse_bundle(bytes: &[u8]) -> Result<Vec<reqwest::Certificate>, reqwest::Error> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    if contains_pem_marker(bytes) {
        return reqwest::Certificate::from_pem_bundle(bytes);
    }
    // DER は先頭が SEQUENCE（0x30）で始まる。それ以外の生ファイルは証明書でない。
    if bytes.first() == Some(&0x30) {
        return reqwest::Certificate::from_der(bytes).map(|cert| vec![cert]);
    }
    Ok(Vec::new())
}

fn contains_pem_marker(bytes: &[u8]) -> bool {
    bytes.windows(PEM_MARKER.len()).any(|w| w == PEM_MARKER)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct MapEnv(BTreeMap<String, String>);

    impl Env for MapEnv {
        fn var(&self, name: &str) -> Option<String> {
            self.0.get(name).cloned()
        }
        fn var_names(&self) -> Vec<String> {
            self.0.keys().cloned().collect()
        }
        fn is_stdin_tty(&self) -> bool {
            false
        }
        fn is_stdout_tty(&self) -> bool {
            false
        }
        fn terminal_width(&self) -> usize {
            100
        }
        fn home_dir(&self) -> Option<PathBuf> {
            None
        }
        fn state_home(&self) -> Option<PathBuf> {
            None
        }
        fn config_home(&self) -> Option<PathBuf> {
            None
        }
    }

    fn env(pairs: &[(&str, &str)]) -> MapEnv {
        MapEnv(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        )
    }

    const PEM_A: &str = "-----BEGIN CERTIFICATE-----\nQUJD\n-----END CERTIFICATE-----\n";
    const PEM_B: &str = "-----BEGIN CERTIFICATE-----\nREVG\n-----END CERTIFICATE-----\n";

    #[test]
    fn reads_bundle_from_ssl_cert_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ca.pem");
        std::fs::write(&path, format!("{PEM_A}{PEM_B}")).unwrap();
        let e = env(&[("SSL_CERT_FILE", path.to_str().unwrap())]);
        let roots = load(&e, "").unwrap();
        assert_eq!(roots.len(), 2);
        assert_eq!(roots.sources, vec![path.display().to_string()]);
    }

    #[test]
    fn ca_file_is_added_before_system_store() {
        let dir = tempfile::tempdir().unwrap();
        let ca = dir.path().join("extra.pem");
        std::fs::write(&ca, PEM_A).unwrap();
        let bundle = dir.path().join("system.pem");
        std::fs::write(&bundle, PEM_B).unwrap();
        let e = env(&[("SSL_CERT_FILE", bundle.to_str().unwrap())]);
        let roots = load(&e, ca.to_str().unwrap()).unwrap();
        assert_eq!(roots.len(), 2);
        assert_eq!(roots.sources[0], ca.display().to_string());
        assert_eq!(roots.sources[1], bundle.display().to_string());
    }

    #[test]
    fn missing_ca_file_is_an_error() {
        let err = load(&env(&[]), "/nonexistent/ca.pem").unwrap_err();
        assert_eq!(err.code, ErrorCode::E2008);
    }

    #[test]
    fn ca_file_without_certificates_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.pem");
        std::fs::write(&path, "no certificates here").unwrap();
        let err = load(&env(&[]), path.to_str().unwrap()).unwrap_err();
        assert_eq!(err.code, ErrorCode::E2008);
    }

    #[test]
    fn reads_directory_when_no_bundle_exists() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.pem"), PEM_A).unwrap();
        std::fs::write(dir.path().join("b.pem"), PEM_B).unwrap();
        assert_eq!(read_dir_certificates(dir.path()).len(), 2);
    }

    #[test]
    fn unrelated_files_in_a_bundle_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mixed.pem");
        std::fs::write(&path, format!("garbage line\n{PEM_A}")).unwrap();
        let e = env(&[("SSL_CERT_FILE", path.to_str().unwrap())]);
        assert_eq!(load(&e, "").unwrap().len(), 1);
    }

    #[test]
    fn candidate_order_prefers_environment() {
        let e = env(&[
            ("SSL_CERT_FILE", "/tmp/from-env.pem"),
            ("CURL_CA_BUNDLE", "/tmp/from-curl.pem"),
        ]);
        let candidates = candidate_bundles(&e);
        assert_eq!(candidates[0], PathBuf::from("/tmp/from-env.pem"));
        assert_eq!(candidates[1], PathBuf::from("/tmp/from-curl.pem"));
        assert!(candidates.len() > 2);
        let dirs = candidate_dirs(&env(&[("SSL_CERT_DIR", "/tmp/certs")]));
        assert_eq!(dirs[0], PathBuf::from("/tmp/certs"));
    }
}
