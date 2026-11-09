//! アーキテクチャの検査（エンジン非依存・依存方向・禁止パターン）。

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(rust_files(&path));
        } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
            out.push(path);
        }
    }
    out
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// `core` は `ports` / `adapters` / `usecases` / `ui` に依存しない。
#[test]
fn core_does_not_depend_on_outer_layers() {
    let mut violations = Vec::new();
    for file in rust_files(&root().join("src/core")) {
        let text = read(&file);
        for needle in [
            "crate::ports",
            "crate::adapters",
            "crate::usecases",
            "crate::ui",
            "reqwest::",
        ] {
            if text.contains(needle) {
                violations.push(format!("{}: {needle}", file.display()));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "core が外側に依存している: {violations:?}"
    );
}

/// `ports` は `adapters` / `usecases` / `ui` に依存しない（`adapters` は `usecases` に依存しない）。
#[test]
fn layer_direction_src() {
    let mut violations = Vec::new();
    for file in rust_files(&root().join("src/ports")) {
        let text = read(&file);
        for needle in ["crate::adapters", "crate::usecases", "crate::ui"] {
            if text.contains(needle) {
                violations.push(format!("{}: {needle}", file.display()));
            }
        }
    }
    for file in rust_files(&root().join("src/adapters")) {
        let text = read(&file);
        for needle in ["crate::usecases", "crate::ui"] {
            if text.contains(needle) {
                violations.push(format!("{}: {needle}", file.display()));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "依存方向が逆転している: {violations:?}"
    );
}

/// `core` に特定エンジンのドメイン名・識別子が現れない。
#[test]
fn core_has_no_engine_specific_knowledge() {
    const FORBIDDEN: &[&str] = &[
        "duckduckgo",
        "google",
        "bing",
        "brave",
        "searx",
        "qwant",
        "yandex",
        "baidu",
        "startpage",
        "result__a",
        "html.duckduckgo",
        "api.search.brave",
    ];
    let mut violations = Vec::new();
    for file in rust_files(&root().join("src/core")) {
        let text = read(&file).to_lowercase();
        for needle in FORBIDDEN {
            if text.contains(needle) {
                violations.push(format!("{}: {needle}", file.display()));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "core にエンジン固有の語が入っている: {violations:?}"
    );
}

/// エンジン定義の評価に、識別子による分岐が無い。
#[test]
fn extractor_has_no_id_based_branch() {
    let text = read(&root().join("src/core/extract.rs"));
    for needle in [
        "match def.id.as_str()",
        "match engine.id.as_str()",
        "\"duckduckgo\" =>",
        "\"google\" =>",
    ] {
        assert!(
            !text.contains(needle),
            "エンジン ID による分岐がある: {needle}"
        );
    }
    // 応答種別の分岐は 1 か所だけに閉じる
    let branches = text.matches("ResponseKind::Html =>").count();
    assert!(branches <= 1, "応答種別の分岐が増えている: {branches}");
}

/// コードに秘密情報を直接書かない（`value_env` のみ）。
#[test]
fn no_hardcoded_api_keys() {
    let mut violations = Vec::new();
    for file in rust_files(&root().join("src")) {
        let text = read(&file);
        if text.contains("sk-") && !text.contains("テスト") {
            // 誤検出を避けるため、明らかな代入パターンのみを見る
            for (i, line) in text.lines().enumerate() {
                if line.contains("sk-")
                    && line.contains('=')
                    && !line.trim_start().starts_with("//")
                {
                    violations.push(format!("{}:{}", file.display(), i + 1));
                }
            }
        }
    }
    assert!(violations.is_empty(), "鍵らしき文字列: {violations:?}");
}

/// 同梱 JSON に秘匿値（鍵そのもの）が入っていない。
#[test]
fn bundled_json_has_no_secrets() {
    let mut violations = Vec::new();
    for dir in ["engines", "network", "filters", "quotas", "config"] {
        for file in json_files(&root().join("assets").join(dir)) {
            let text = read(&file);
            let lower = text.to_lowercase();
            // `value_env` 以外での鍵記述を禁止する
            if (lower.contains("api_key") || lower.contains("apikey"))
                && !lower.contains("value_env")
                && !lower.contains("requiresapikey")
            {
                violations.push(file.display().to_string());
            }
        }
    }
    assert!(violations.is_empty(), "同梱 JSON に鍵: {violations:?}");
}

fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            out.push(path);
        }
    }
    out
}

/// 仕様書に実装のコードが混ざっていない（`specifications/` はコード非依存）。
#[test]
fn specifications_have_no_file_references() {
    let dir = root().join("docs/specifications");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut violations = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e == "md").unwrap_or(false) {
            let text = read(&path);
            for (i, line) in text.lines().enumerate() {
                if line.contains("REF:") || line.contains("](") {
                    violations.push(format!("{}:{}", path.display(), i + 1));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "仕様書にファイル参照が入っている: {violations:?}"
    );
}

/// 警告コードの発行元が仕様の表（`REF:docs/specifications/errors.md` §3）と一致する。
///
/// 発行元が 1 か所に定まるコードだけを検査する。状態系（`W5xxx`）は
/// 復旧処理・運用コマンドの双方から出るため対象外とする。
#[test]
fn warning_codes_are_emitted_where_spec_says() {
    // (コード, 仕様上の発行元ファイル, 許容する追加の発行元)
    const OWNERS: &[(&str, &str, &[&str])] = &[
        ("W3001", "src/core/convert.rs", &[]),
        ("W3002", "src/core/convert.rs", &[]),
        (
            "W3003",
            "src/adapters/extract/mod.rs",
            &["src/adapters/http.rs", "src/adapters/engine/eval.rs"],
        ),
        ("W4001", "src/core/extract.rs", &["src/usecases/search.rs"]),
        ("W4002", "src/adapters/engine/loader.rs", &[]),
        ("W4003", "src/usecases/search.rs", &[]),
        ("W4005", "src/usecases/search.rs", &[]),
        ("W4006", "src/usecases/search.rs", &[]),
    ];

    let mut violations = Vec::new();
    for (code, owner, allowed) in OWNERS {
        let needle = format!("WarningCode::{code}");
        let mut found: Vec<String> = Vec::new();
        for file in rust_files(&root().join("src")) {
            let text = read(&file);
            if !text.contains(&needle) {
                continue;
            }
            // 発行（Diagnostic::new / push）に関わる行のみを対象にし、
            // コード定義そのもの（core/errors.rs）は除く。
            let rel = file
                .strip_prefix(root())
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            if rel == "src/core/errors.rs" {
                continue;
            }
            // `Diagnostic::new(` とコードが別行になることがあるため前後を見る
            let lines: Vec<&str> = text.lines().collect();
            let emits = lines.iter().enumerate().any(|(i, l)| {
                if !l.contains(&needle) {
                    return false;
                }
                let lo = i.saturating_sub(3);
                let hi = (i + 2).min(lines.len());
                lines[lo..hi].iter().any(|w| w.contains("Diagnostic::new"))
            });
            if emits {
                found.push(rel);
            }
        }
        for rel in &found {
            if rel != owner && !allowed.contains(&rel.as_str()) {
                violations.push(format!("{code} を {rel} が発行している（仕様は {owner}）"));
            }
        }
        if !found.iter().any(|r| r == owner) {
            violations.push(format!("{code} の正規の発行元 {owner} が見つからない"));
        }
    }
    assert!(
        violations.is_empty(),
        "警告コードの発行元がずれている: {violations:?}"
    );
}

/// `ExtractOutcome` の警告は「コード付き」で持ち、どの層でも同じ解釈になる。
#[test]
fn extract_outcome_carries_typed_diagnostics() {
    let text = read(&root().join("src/core/extract.rs"));
    assert!(
        text.contains("pub warnings: Vec<Diagnostic>"),
        "ExtractOutcome.warnings は Vec<Diagnostic> であること（文字列だと発行元でコードを推測することになる）"
    );
    assert!(
        !text.contains("warnings: Vec<String>"),
        "ExtractOutcome に文字列の警告が残っている"
    );
}

/// テスト以外で `unwrap` / `expect` / `panic!` を使わない（失敗は `AppError` で返す）。
///
/// 例外は「起動時に一度だけ組み立てる静的パターン」と「ランタイム生成」で、
/// 失敗しても回復できないものに限る。例外は下の一覧に明示し、増やすときは
/// ここへ追記する（黙って増えることを防ぐため）。
#[test]
fn no_panics_outside_tests() {
    const ALLOWED: &[(&str, &str)] = &[
        ("src/adapters/http.rs", "tokio ランタイムの生成に失敗"),
        ("src/core/convert.rs", "静的セレクタ"),
        ("src/core/markdown.rs", "link regex"),
        ("src/core/select.rs", "静的パターン"),
    ];

    let mut violations = Vec::new();
    for file in rust_files(&root().join("src")) {
        let rel = file
            .strip_prefix(root())
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let text = read(&file);
        // `#[cfg(test)]` 以降はテストコード
        let body = match text.find("#[cfg(test)]") {
            Some(i) => &text[..i],
            None => text.as_str(),
        };
        for (i, line) in body.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            let hit = [".unwrap()", "panic!(", "todo!(", "unimplemented!("]
                .iter()
                .find(|p| line.contains(**p));
            if hit.is_some() {
                violations.push(format!("{rel}:{}", i + 1));
            }
            if line.contains(".expect(")
                && !ALLOWED
                    .iter()
                    .any(|(f, needle)| *f == rel && line.contains(needle))
            {
                violations.push(format!("{rel}:{}", i + 1));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "テスト外で panic しうる呼び出しがある: {violations:?}"
    );
}

/// 出力を揺らす呼び出し（現在時刻・乱数・環境変数の直読み）を `src` に置かない。
///
/// 時刻・乱数・環境変数はポート（`Clock` / `IdGen` / `Env`）経由でのみ取得し、
/// テストでは差し替えられるようにする。
#[test]
fn no_nondeterministic_calls_in_src() {
    const FORBIDDEN: &[&str] = &[
        "SystemTime::now",
        "Instant::now",
        "rand::random",
        "thread_rng",
        "std::env::var",
        "std::env::vars",
        "chrono::Utc::now",
        "chrono::Local::now",
        "Local::now()",
    ];
    let mut violations = Vec::new();
    for file in rust_files(&root().join("src")) {
        let rel = file
            .strip_prefix(root())
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let text = read(&file);
        let body = match text.find("#[cfg(test)]") {
            Some(i) => &text[..i],
            None => text.as_str(),
        };
        for (i, line) in body.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            for needle in FORBIDDEN {
                if line.contains(needle) {
                    violations.push(format!("{rel}:{} {needle}", i + 1));
                }
            }
        }
    }
    // 時刻・環境変数の実装（ポートのアダプタ）だけは直接呼んでよい
    let allowed_prefixes = ["src/ports/system.rs", "src/adapters/state/mod.rs"];
    violations.retain(|v| {
        let path = v.split(':').next().unwrap_or_default();
        !allowed_prefixes.contains(&path)
    });
    assert!(
        violations.is_empty(),
        "時刻・乱数・環境変数を直接読んでいる: {violations:?}"
    );
}

/// `core` は I/O を持たない（`std::fs` / `std::net` / `std::process` を参照しない）。
#[test]
fn core_is_pure() {
    let mut violations = Vec::new();
    for file in rust_files(&root().join("src/core")) {
        let text = read(&file);
        let body = match text.find("#[cfg(test)]") {
            Some(i) => &text[..i],
            None => text.as_str(),
        };
        for needle in ["std::fs", "std::net", "std::process", "std::io"] {
            if body.contains(needle) {
                violations.push(format!("{}: {needle}", file.display()));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "core が I/O を持っている: {violations:?}"
    );
}

/// 履歴の読み戻しは壊れた行を飛ばし、件数を返す（`W5004` の前提）。
#[test]
fn history_import_skips_broken_lines() {
    use llmsearch::adapters::state::history_jsonl::import_lines;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.jsonl");
    std::fs::write(
        &path,
        "{\"id\":\"H1\",\"at\":\"2026-01-01T00:00:00Z\",\"session\":\"S1\",\"type\":\"search\",\"payload\":{}}\n\
         not json\n\
         \n\
         {\"id\":\"H2\",\"at\":\"2026-01-01T00:01:00Z\",\"session\":\"S1\",\"type\":\"fetch\",\"payload\":{}}\n",
    )
    .unwrap();
    let mut sink = Vec::new();
    let skipped = import_lines(&path, &mut sink).unwrap();
    assert_eq!(sink.len(), 2);
    assert_eq!(skipped, 1);
}

mod support;

/// `config explain <key>` は仕様どおり「値 + 由来」を返し、未知キーは `E1004`。
#[test]
fn config_explain_reports_value_and_origin() {
    let h = support::harness(None);
    let (value, origin) = llmsearch::usecases::config::explain(
        &h.app,
        &h.resolved,
        h.app.env.as_ref(),
        "search.limit",
    )
    .unwrap();
    assert_eq!(value, "7");
    assert!(matches!(
        origin,
        llmsearch::usecases::config::Origin::Default
    ));

    let missing = llmsearch::usecases::config::explain(
        &h.app,
        &h.resolved,
        h.app.env.as_ref(),
        "no.such.key",
    )
    .unwrap_err();
    assert_eq!(missing.code, llmsearch::core::errors::ErrorCode::E1004);
}
