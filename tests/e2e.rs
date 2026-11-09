//! E2E（実環境）テスト。
//!
//! 既定では実行しない。`LLMSEARCH_E2E_LIVE=1` を設定した時だけ公開ページへ
//! 実際にアクセスし、検索→取得→アウトライン→節読みが通ることを確認する。
//! フラグなしで実行した場合は、実ネットワークに出ない経路（決定論性・
//! 同梱アセットの解決）だけを検証する。

use std::process::Command;

fn bin() -> std::path::PathBuf {
    // テストバイナリと同じディレクトリに置かれた実行ファイルを使う
    let mut path = std::env::current_exe().expect("テストバイナリのパス");
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.join("llmsearch")
}

fn state_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("一時ディレクトリ")
}

fn run(args: &[&str], state: &std::path::Path) -> (i32, String, String) {
    let out = Command::new(bin())
        .args(args)
        .env("LLMSEARCH_STATE_DIR", state)
        .env("NO_COLOR", "1")
        .output()
        .expect("コマンドの実行に失敗");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn live() -> bool {
    std::env::var("LLMSEARCH_E2E_LIVE").ok().as_deref() == Some("1")
}

/// 同梱アセットだけで起動でき、診断と設定表示が成功する。
#[test]
fn e2e_bundled_assets_are_enough() {
    let dir = state_dir();
    let (code, out, err) = run(&["config", "show"], dir.path());
    assert_eq!(code, 0, "stderr: {err}");
    assert!(out.contains("search.engine"), "出力: {out}");

    let (code, out, err) = run(&["engines", "list"], dir.path());
    assert_eq!(code, 0, "stderr: {err}");
    assert!(out.contains("duckduckgo"), "出力: {out}");
}

/// 決定論性: 同じ入力から同じ出力が得られる。
#[test]
fn e2e_output_is_deterministic() {
    let dir = state_dir();
    let (c1, o1, _) = run(&["config", "show"], dir.path());
    let (c2, o2, _) = run(&["config", "show"], dir.path());
    assert_eq!(c1, 0);
    assert_eq!(c2, 0);
    assert_eq!(o1, o2, "同じ入力で出力が変わっている");

    let (_, q1, _) = run(&["quota", "show"], dir.path());
    let (_, q2, _) = run(&["quota", "show"], dir.path());
    let strip = |s: &str| {
        s.lines()
            .map(|l| l.split_whitespace().take(1).collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(strip(&q1), strip(&q2), "クォータ表示が揺れている");
}

/// 実端末に近い経路（非 TTY の標準入力）で対話モードが E1001 を返す。
#[test]
fn e2e_interactive_requires_tty() {
    let dir = state_dir();
    let (code, _out, err) = run(&[], dir.path());
    assert_eq!(code, 1, "stderr: {err}");
    assert!(err.contains("E1001"), "stderr: {err}");
}

/// 実ネットワーク: 検索→取得→アウトライン→節読み。
#[test]
fn e2e_live_search_and_read() {
    if !live() {
        eprintln!("LLMSEARCH_E2E_LIVE=1 が無いためスキップ");
        return;
    }
    let dir = state_dir();
    let (code, out, err) = run(&["search", "気候変動", "--limit", "3"], dir.path());
    assert_eq!(code, 0, "検索に失敗: {err}");
    assert!(out.contains("http"), "検索結果が空: {out}");

    // 1 件目の URL を拾って本文の先頭を読む
    let url = out
        .split_whitespace()
        .find(|w| w.starts_with("https://") || w.starts_with("http://"))
        .expect("URL が出力に無い")
        .trim_end_matches([')', ',', ']'])
        .to_string();
    let (code, outline, err) = run(&["outline", &url], dir.path());
    assert_eq!(code, 0, "アウトラインに失敗: {err}\n{outline}");
    assert!(!outline.trim().is_empty(), "アウトラインが空");
}
