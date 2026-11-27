# M18 ゼロ前提・全量ドキュメント／ソース同期監査記録

## 1. 監査の目的・前提・制約

- **対象ベース**: `origin/develop` (`0326c1abc0b110ca5faa69050c39ebabaca5a17a`、M17 PR #52 マージ直後)
- **作業ブランチ**: `arena/b7f10cc2-llmsearch`（Arena セッション固定ブランチ制約により別名ブランチへの切替は行わず、`origin/develop` `0326c1a` へ fast-forward リセットしたクリーン状態から開始）
- **監査方針**:
  - 過去の監査（M12・M13 Issue #37〜#44・M15・M16・M17）の結果を一切前提とせず、`docs/specifications/*.md`（15 本）・`docs/designs/*.md`（15 本）・`README.md`・`docs/developers.md`・`docs/plans.md`・`docs/handoff.md`・`docs/history.md` と、`src/**`・`tests/**`・`assets/**`・`scripts/**`・`build.rs`・`.github/workflows/*` をゼロから全行照合した。
  - すべての指摘事項について、設定キーや `enum` の存在確認だけに頼らず、表示文字列リテラルの読解および `target/debug/llmsearch` 実バイナリ＋`127.0.0.1` ループバック HTTP スタブ／PTY／単一バイナリ隔離配置（`no-assets`）の実行出力で立証した。
  - **外部の実在 Web サイトへの通信は 0 回**。
  - **ソース不変更規律**: M18 では `src/`・`tests/`・`assets/`・`scripts/`・`Cargo.toml`・`build.rs` を一切編集せず、純粋な文書側の誤記・古い構成図・架空モジュール名・M14〜M17 反映漏れのみ修正し、実装バグ・未実装機能・仕様判断事項はすべてオープン Issue として起票した。

---

## 2. コード横断の発行・参照実態（機械照合テーブル）

### 2.1 `ErrorCode` 発行箇所数（定義 `src/core/errors.rs`・分類 `src/ports/mod.rs` を除く）

| コード | 発行箇所数 | 備考 |
|---|---:|---|
| `E1001`〜`E1008` | 2〜33 | 全コード発行あり |
| `E2001`〜`E2006`, `E2008` | 2〜8 | 発行あり（ただし HTTP `401` が `E4003` でなく `E2003` に分類される不具合あり → #58） |
| `E2007` | **0** | `errors.md` §2.2 に未発行と明記済み（打ち切り＋`W3003`） |
| `E3001` | **0** | **未発行**（`errors.md` §2.3 に記載あり → #56） |
| `E3002` | 1 | `src/adapters/extract/mod.rs`（非対応 Content-Type） |
| `E3003` | **0** | **未発行**（`errors.md` §2.3 に記載あり。`errors.rs:289` の英語 hint `"set network.charset"` も実キー名 `charset` と不一致 → #56） |
| `E3004` | 1 | `src/core/convert.rs` |
| `E4001`〜`E4004` | 4〜8 | 発行あり |
| `E4005` | **0** | **未発行**（重複エンジン定義は `W4002` でスキップ → #56） |
| `E4006` | **0** | **未発行**（相対 URL は `src/core/extract.rs:334` で無条件自動解決 → #56 / #58） |
| `E5001`, `E5002`, `E5004`, `E5005` | 3〜19 | 発行あり |
| `E5003` | **0** | **未発行**（`designs/session-state.md` §3.4 のみ注記、`errors.md` §2.5 は未注記 → #56） |

### 2.2 `WarningCode` および `EventType` 発行実態

- **`WarningCode`**: `W5004` のみ 0 件（M12 で未発行と明記済み）。他 19 種はすべて発行箇所あり（ただし `W4002` は引数順序逆転 → #56、`W5005` は `onExceed="warn"` で常時誤発火 → #61、`W4001` は `refine` の前後空白未正規化比較で誤発火 → #58）。
- **`EventType`（`src/core/history.rs`）**: `Search`, `Next`, `Refine`, `Fetch`, `Outline`, `Read`, `Select`, `Quota`, `Delete` は発行あり。**`EventType::Engine`（0 件）および `EventType::Config`（0 件）は未発行**（`history.md` §1.1 と乖離 → #62）。

---

## 3. 起票した GitHub Issue 一覧（#53〜#64）と実機検証エビデンス

| Issue | タイトル | 対応区分 | 主な該当箇所（文書 ↔ 実装） |
|---|---|---|---|
| **#53** | `[M18-01]` トップレベル文書（README / developers / plans / handoff / history）と実態・M14〜M17 の不整合、および仕様書内の他ファイル参照 | **M18 で解消** | `README.md:29,68,76-87,124`, `developers.md:100,110-131`, `plans.md`（M13〜M14/M18/M19 の記録・アンカー補完・M15〜M17 の状態更新）, `handoff.md`, `history.md`, 仕様書 8 本（`search` / `context-assembly` / `agent-integration` / `cli-interface` / `configuration` / `engine-definitions` / `errors` / `network`）。仕様書 5 本に残留していた裸名（`configuration.md` §3.2.1 等）の他ファイル参照 11 箇所を、ファイル名を使わない概念記述へ置換（ドキュメント管理規則「仕様書は他のファイルへの参照を一切含めない」準拠・意味不変） |
| **#54** | `[M18-02]` 設計書群（`docs/designs/*.md`）の架空モジュールパス・型名・実装説明の乖離 | **M18 で解消** | 全 15 本の `docs/designs/*.md` ↔ `src/**`。`engine-definitions.md` §1.1〜§1.3 に残留した架空記述（`Extractor` トレイト / `HtmlExtractor` / `JsonExtractor` / `registry.rs` / `SelectorSpec` / 架空の検索結果キャッシュキー）を実在の `extract()` / `ResponseKind` / `EngineRegistry` / `FieldSpec` / `EngineDef::hash()` → `engineHash` へ是正（`src/core/extract.rs:34,28`, `src/core/engine.rs:48,193`, `src/adapters/engine/loader.rs:44`, `src/usecases/engines.rs:233`） |
| **#55** | `[M18-03]` 検索結果キャッシュ（`cache.searchResults`）および設定・モデルの未実装／死にフィールド群 | **M19 実装・仕様課題** | `configuration.md` §3.4/§5, `search.md` §2.1, `engine-definitions.md` §6, `context-assembly.md` §3.2/§3.5 ↔ `settings.rs`, `search.rs`, `interactive.rs:257`, `document.rs:59,326` |
| **#56** | `[M18-04]` エラー・警告・`doctor` 診断の乖離（未発行エラーコード 5 件、`W4002` 引数逆転、`doctor` の HTTP GET と `rustc` 重複表記） | **M19 実装・仕様課題** | `errors.md` §2.3-2.5/§3/§5 ↔ `errors.rs:289,437`, `loader.rs:36`, `doctor.rs:220-261`, `build.rs:15` |
| **#57** | `[M18-05]` エンジン定義・テンプレート展開のバグ（`request.encoding` 無視、`enabled=false` 無視、`body` 未検証、仕様スキーマ差分） | **M19 実装・仕様課題** | `engine-definitions.md` §2.1/§3.2-3.4/§4 ↔ `template.rs:141`, `engine.rs:607`, `search.rs:733`, `extract.rs:423` |
| **#58** | `[M18-06]` HTTP・ネットワーク層と検索 URL 処理の乖離（`401` のエラーコード、`403` 再試行、`onError` 混同、相対 URL 自動解決、秘匿誤爆） | **M19 実装課題** | `network.md` §5.1/§7.2/§8.3, `search.md` §2.2/§4.1 ↔ `http.rs:368,456`, `extract.rs:334`, `url.rs:42`, `search.rs:831` |
| **#59** | `[M18-07]` 設定・プロファイル解決と検証の欠落（埋め込み既定・`--state-dir` 非対応、起動時検証スルー、`config validate` 範囲不足） | **M19 実装課題** | `configuration.md` §1.2/§2.3/§6 ↔ `commands.rs:940-1155,1532`, `usecases/config.rs:144,171,647`, `network.rs:169`, `filter.rs:495` |
| **#60** | `[M18-08]` 文書取得・キャッシュ・HTML 変換・コンテキスト組立の乖離（キャッシュ無効化無視、ATX 誤認、`~~~` フェンス破壊、`read` オプション無視） | **M19 実装課題** | `document-fetching.md` §2/§3.1/§4, `context-assembly.md` §1.1/§3.2/§4.3 ↔ `document.rs:135,335`, `convert.rs:102,270`, `outline.rs:38`, `context.rs:66`, `commands.rs:329,354,434` |
| **#61** | `[M18-09]` クォータ管理のバグと仕様差分（`onExceed="warn"` の常時誤警告、`quota check --kind` 値域、`quota reset` の空セッション ID） | **M19 実装・仕様課題** | `quotas.md` §2.6/§4.3-4.4/§6.1 ↔ `quota.rs:344`, `commands.rs:1203,1399` |
| **#62** | `[M18-10]` 状態ディレクトリ構造・セッション隔離・履歴操作のバグ（`clear` の全件消去と件数 0 固定、`refine` 要約、永続化パス差分） | **M19 最優先課題** | `history.md` §1/§3.1-3.2/§4.3, `session-state.md` §4.2, `quotas.md` §3.2 ↔ `commands.rs:630,672,682`, `history_jsonl.rs:209`, `history.rs:131`, `fs.rs:55` |
| **#63** | `[M18-11]` 対話モードの入出力ストリーム違反と画面遷移バグ（`stdout` へのメニュー出力、ページ送り無視、`4) Edit the query` の挙動） | **M19 実装・仕様課題** | `cli-interface.md` §1.2/§2.1/§5.1 ↔ `main.rs:146`, `interactive.rs:78,106,208,254`, `menu.rs:151` |
| **#64** | `[M18-12]` エージェント連携（`--json`）イベント構造と仕様書の乖離（`engines test` イベント種別、コマンド対応表、追加フィールド） | **文書同期済 + M19 残（`omittedChildren`）** | `agent-integration.md` §2.2, `context-assembly.md` §3.5 ↔ `commands.rs:360,804,835,1470-1525` |

### 主要な実バイナリ検証ログ抜粋（`target/debug/llmsearch` @ `0326c1a` + `127.0.0.1` ループバックスタブ）

```text
# [#57] request.body の未知変数を engines validate が見逃し、dry-run で E1008
validate rc: 0 out: [stub] / [  ok] no issues
dry-run rc: 1 err: error: E1008 unknown template variable (detail: {{unknown_in_body}})

# [#57] request.encoding = "euc-jp" で "日本" を展開 -> EUC-JP (%C6%FC%CB%DC) ではなく UTF-8 になる
GET http://127.0.0.1:35913/search?k=&q=%E6%97%A5%E6%9C%AC

# [#55 / #58] titleChars(100)/snippetChars(120) 未適用、author/keyword/design の誤伏字化、相対 URL 自動解決
r0 title len: 150 snippet len: 180 url: http://127.0.0.1:52197/doc1?author=***&keyword=***&design=***
r1 url: http://127.0.0.1:52197/doc2

# [#55] cache.searchResults = true でも同一検索 2 回で HTTP 2 リクエスト送信
search HTTP requests for identical query (expected 1 if cached, actual): 2

# [#60] expires_at 期限切れ + --no-cache + -o cache.enabled=false でもキャッシュ本文が返り HTTP 0 回
HTTP requests to /doc1 with expired expires_at + --no-cache + -o cache.enabled=false (expected 1, actual): 0

# [#60] ~~~sh フェンス途中で --max-chars 500 切り詰め -> ``` が出力されて ~~~ が閉じない
  'echo hello_world_1234567890'
  '```'
  '[truncated: shown 492 / total 857 chars]'
  '--- end ---'

# [#58] HTTP 401 -> E2003 (exit 2)、HTTP 403 -> onStatus 外でも 3 回再試行
401 rc: 2 err: error: E2003 HTTP error response
403 rc: 4 requests sent (onStatus=[429,502,503,504], maxAttempts=3): 3 (+ 1 robots.txt)

# [#59 / #61] 不正な network/quota が起動を通過し、onExceed="warn" が 1/50 でも W5005 を誤発火
invalid network/quota startup rc: 0
warning: W5005 quota searches is over its limit (continuing)

# [#59] assets/ 無しの単一バイナリ配置（no-assets）で network / filters が失敗・空出力
no-assets network list -> rc=0 out=''
no-assets network show default -> rc=1 err='error: E1003 cannot load or resolve configuration resource'
no-assets filters show default -> rc=1 err='error: E1003 cannot load or resolve configuration resource'

# [#62] 2026 年の履歴に対し --before 2000-01-01T00:00:00Z clear --yes -> 全件消去され removed=0
Cleared the history (0 entries)
No history entries.

# [#56] W4002 の引数逆転と doctor の rustc 重複表示
warning: W4002 ignored the definition '/tmp/.../b.json' in dup_id because it is already loaded
[  ok] version                  llmsearch 0.1.2 / rustc rustc 1.90.0 (1159e78c4 2025-09-14) / linux-x86_64

# [#63] PTY 接続時の対話プロンプト出力先（stderr でなく stdout）
PTY stdout: 'session 20261127-064300-4217 (resume with --session 20261127-064300-4217)\r\nquery: '
PTY stderr: ''
```

---

## 4. M14〜M17 の意図された仕様変更の確認（Issue 対象外として記録）

以下の変更は M10・M14・M15・M16・M17 を通じて意図的に仕様・設計・実装へ導入されたものであり、不整合 Issue とはせず `docs/plans.md` §8.1 に記録した。

1. CLI 全出力の英語固定・`ui.locale` / `LLMSEARCH_UI_LOCALE` 廃止（`W1002` 告知）と `search.language` / `--language` による検索言語の一本化。
2. 明示 `--session` での未知 ID 拒否（`E5001`）と、非 `search` 系サブコマンドでの `counters.json` 由来セッション暗黙再開。
3. JSON `document` / `content` イベントへの構造化非信頼境界 `untrusted: true` 追加（本文文字列不変）。
4. 同一検索語での `refine` 実行時における自己矛盾 `W4001` 抑止方針。
5. URL userinfo・埋込 URL・不正/非 HTTP URL・`RobotsGate` 拒否パス・応答ヘッダパラメータ・抽出不一致プレビュー（JSON/form/header）の多層秘匿化と form テンプレート無変換保持。

---

## 5. 品質ゲート検証結果

- `git diff --name-only src/ tests/ assets/ scripts/ Cargo.toml build.rs`: 出力空（コード・テスト・同梱資産・スクリプト無変更）
- `python3 docs/script/verify-docs.py`: 全規則（rule-1〜rule-7）合格
- `. /home/user/build/env.sh && bash scripts/verify.sh --quick`: ドキュメント規則検査・配布互換ゲート回帰・`cargo fmt --all --check`・`cargo clippy --locked --all-targets -- -D warnings`・全ターゲット `cargo test --locked --all-targets` 合格
- 最終実行（2026-11-26）: `scripts/verify.sh --quick` 5/5 段階緑（docs / release-gate / fmt-check / lint / test。ユニットテスト 166 件＋統合・契約・アーキテクチャ全スイート合格）
