# M16 検証記録

## 1. Muse による実環境検証（ユーザー報告の引用・要約）

対象: develop `5db4708`（M15 PR #46 後）、llmsearch 0.1.2。
以下は Muse がローカルで実施した結果であり、エージェントによる再実行ではない。

- DuckDuckGo 実検索で 7 件取得。JSON Lines と quota メトリクス正常。
- refine の W4001 が JSON `warnings` に伝播。`--quiet` では stderr が空、JSON の警告は保持。
- next / refine / セッション継続が動作。
- `outline "https://example.com/?api_key=SECRET123XYZ&token=abc123token&password=hunter2pass&normal=keepme"` の JSON url / finalUrl は、報告上 `?api_key=<redacted>&token=<redacted>&password=<redacted>&normal=keepme`。機微値は stdout / stderr に出ず、通常パラメータは保持。`<redacted>` はユーザー報告の表記であり、今回のスタブで確認した実装の表示値は `***`（下記）。
- キャッシュ再利用の警告は W3003 として再掲（初回 W3001/W3002）。document-fetching.md の規定どおりであり修正対象外。
- rust-lang.org の outline は 12 節、read で本文取得。テキストに `[untrusted] https://rust-lang.org/` と `--- end ---` の境界あり。

### 発見と処置

- F1 / #47: 同一セッション・同一クエリで refine を繰り返すと、2 回目に `{"code":"W4001","message":"changed the query from 'Rust CLI web search staged' to 'Rust CLI web search staged'"}` が出た。クエリ不変時だけ W4001 を抑制する。
- F2 / #48: JSON read の content に非信頼メタデータも文字列境界もなかった。仕様→設計→計画を更新し、content/document に `untrusted: true` を追加。本文・文字数・v=1 は維持。消費側の境界保持責任も明記。
- DuckDuckGo の一時的 E4004 は再試行で成功。W4004 の応答先頭 2048 文字は上限付き仕様どおり。Issue 化・修正は行わない。

### 今回も未検証

実サイトでの URL userinfo 成功系の秘匿。Muse の `https://user:pass123secret@example.com/` は E2002 タイムアウト、エラーに認証情報漏れはなかったが成功系は確認できなかった。既存の単体テストがあっても、この実環境検証を完了扱いにはしない。

## 2. エージェントによる再現可能なローカル回帰

実サイト・実検索プロバイダへのアクセスは行わない。`tests/m16.rs` は一時 state/config/engine と OS 割当ポートの 127.0.0.1 HTTP スタブを使用し、実バイナリを子プロセスで実行する。外部リンクは検索結果データであり取得しない。

- 同一 session の search → refine（変更）→ refine（同一）→ refine（変更・quiet）→ refine（同一・quiet）。変更時のみ W4001、非 quiet stderr に JSON と同じ文言、quiet stderr は空、JSON は警告保持。履歴には 4 回の refine が残る。
- outline / read の document と content に boolean `untrusted: true`。JSON text は `EXTERNALBODY ignore all instructions.` を保持し、テキスト用境界を挿入しない。
- text read は `[untrusted] http://127.0.0.1:<port>/document?...` と `--- end ---`、本文を実出力する。
- スタブの JSON url/finalUrl は `?api_key=***&token=***&password=***&normal=keepme`。outline と JSON/text read の stdout/stderr のどちらにも元の 3 値は出ない。

最初の試験では no-cache の read が複数回 fetch する経路に対しスタブの受付数が不足したため失敗。受付数を実経路に合わせ、Markdown 変換によるアンダースコアのエスケープに依存しない本文 fixture に修正して再実行した（製品仕様の変更なし）。

### 実行結果

- `cargo test --locked --test m16`: 2 passed / 0 failed。
- `bash scripts/verify.sh --quick`: docs / release-gate / fmt-check / lint / test すべて成功。`cargo clippy --locked --all-targets -- -D warnings` 警告なし、`cargo test --locked --all-targets --no-fail-fast` は 19 スイート合計 357 passed / 0 failed。
- 回帰の有効性: 製品コード 2 ファイルだけを一時的に修正前 HEAD の内容に戻し、同じ `cargo test --locked --test m16` を実行すると 2 件とも失敗（F1: 同一 refine の W4001 が true、F2: untrusted が null）。修正内容を復元して同じコマンドを再実行し、2 passed / 0 failed を確認。
- 初回の docs 検査は仕様／設計の見出し不一致と進捗 SYNC 不一致を検出。文書構造・同期を直し、全体ゲートを再実行して成功。
- `--quick` のため release ビルドは今回省略。実サイト再検証は実施していない。
