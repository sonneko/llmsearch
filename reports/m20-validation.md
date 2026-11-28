# M20 検証レポート（Issue #57 / #58 / #59 / #60 対応）

対象ブランチ: `arena/b7f10cc2-llmsearch`（develop `b5638f1` から分岐）
日付: 2026-11-26
検証手法: **実行による検証**（主張ではなく実際のバイナリ出力）。実サイトには一切アクセスせず、
`127.0.0.1` ループバック HTTP スタブ（`reports/m20-evidence/server.py`）＋ 実バイナリのみ。

## 1. 検証環境

- ループバック スタブ: `127.0.0.1:<port>` にバインド。エンドポイント:
  - `/search`: 2 件（doc1 は `?author=alice&keyword=climate&design=modern` 付き絶対 URL、doc2 は相対 `/doc2`）。
  - `/doc1`: `<p>#hashtag_without_space …</p>`（見出し誤認検証）、`<img alt="preserved-alt-text">`（alt 保持検証）、`~~~sh` フェンス（閉じ検証）。
  - `/doc3`: 安定した親子見出し（`Top` → `Child A` / `Child B`）。
  - `/e401` / `/e403`: 固定 401 / 403。
  - 全リクエストを `requests.log` に `METHOD PATH` で記録（リクエスト数計測用）。
- ドライバ: `reports/m20-evidence/driver.sh <binary> <label>`。Issue 各「実機検証エビデンス」を実行し、
  `requests.log` の行数差分で HTTP リクエスト数を計測。
- BEFORE バイナリ: 修正前（develop `b5638f1`）を再構築した `/tmp/llmsearch-prefix`。
- AFTER バイナリ: 修正済み `target/debug/llmsearch`。
- 生出力: `reports/m20-evidence/BEFORE.txt` / `AFTER.txt`。

両バイナリは同一ドライバ・同一ループバックで実行し、**差分はバイナリのみ**。

## 2. BEFORE / AFTER 実行エビデンス

### 2.1 #57（エンジン定義・テンプレート）

| 項目 | BEFORE（修正前） | AFTER（修正後） |
|---|---|---|
| `engines validate stub`（`request.body` に `{{unknown_in_body}}`） | `[stub]` / `[  ok] no issues`（rc=0、**body を検査しない**） | `E1004 … stub: unknown template variable {{unknown_in_body}}`（rc=1、**body も検査**） |
| `engines test stub --query 日本 --dry-run` | `E1008 unknown template variable`（実行時） | `E1004`（**ロード時の fail-fast**。URL 変数と同一経路、後述） |
| `enc`（`encoding=euc-jp`）`--query 日本 --dry-run` | `GET …/search?k=&q=%E6%97%A5%E6%9C%AC`（**UTF-8 固定**） | `GET …/search?k=&q=%C6%FC%CB%DC`（**EUC-JP**） |
| `search "climate" --engine disabled`（`enabled:false`） | rc=0（**実行してしまう**、結果表示） | rc=4 `E4002 engine 'disabled' is disabled (enabled=false)`（**候補から除外**） |

- **#57-1**: `template::prepare` が `request.encoding` を無視し UTF-8 固定でクエリをエンコードしていた（`let _ = encoded;` のデッドコード）。指定 encoding を反映。
- **#57-2**: `enabled:false` エンジンは選択候補に含まれていた。`candidate_engines` で除外し、明示指定時は `E4002`。
- **#57-3**: `engine::validate` が `request.body` のテンプレート変数を検査しなかった。body を検査対象に追加。
  - 補足: b5638f1 では**未知テンプレート変数はロード時（`ensure_valid`）に `E1004` で fail-fast** する（URL 変数でも同一）。issue の「dry-run → E1008」は旧ベースライン（fail-fast 導入前）の記述で、b5638f1 では URL 変数も `E1004`。今回の body 変数追加は URL 変数と同一経路（`E1004`）で一貫している。
- **#57-4**: `{{apiKey}}` が常に空だった（`search.rs` が `api_key: None` 固定）。`tmpl_uses_api_key` / `resolve_api_key` を追加し `value_env` 参照（header/query）を解決。

### 2.2 #58（HTTP・ネットワーク・検索 URL）

| 項目 | BEFORE | AFTER |
|---|---|---|
| HTTP 401 | rc=2 `E2003 HTTP error response` | rc=4 `E4003 rate limited or blocked` |
| HTTP 403（`onStatus=[429,502,503,504]`, `maxAttempts=3`） | `/e403` **3 リクエスト**（再試行） | `/e403` **1 リクエスト**（`403 ∉ onStatus` のため再試行せず） |
| 検索結果 URL の伏字化（`?author=alice&keyword=climate&design=modern`） | `?author=***&keyword=***&design=***`（**誤伏字化**） | `?author=alice&keyword=climate&design=modern`（**保持**） |
| 相対 URL `/doc2`（`absolutize` 未指定） | `http://127.0.0.1:PORT/doc2`（**自動絶対化**、結果 2 件） | **破棄**（結果 1 件） |
| `refine "  hello  "`（前クエリ `hello`） | `warning: W4001 changed the query from 'hello' to '  hello  '`（**誤発火**） | 無（**正規化比較で一致**） |

- **#58-1**: `http.rs` が 401/403 を `E2003` にしていた。仕様 network.md §8.3 に従い 401/403/429 → `E4003`。`retryable()` は E2001/E2002 の論理和混同を解消（`timeout` / `connect` を分離）し、ステータス由来（`E4003`/`E2003`）は `onStatus` に含まれる場合のみ再試行。
- **#58-2**: 伏字化が `.contains` の部分一致で `author`/`keyword`/`design` を誤検出していた。`url.rs` がパラメータ名単位の判定に。
- **#58-3**: 相対 URL を `absolutize` 未指定でも自動絶対化していた。仕様 search.md §2.2 に従い破棄（`absolutize` 指定時のみ基準 URL で解決）。
- **#58-5**: `refine` が生文字列比較で `W4001` を誤発火。正規化（前後空白除去）比較に。

### 2.3 #59（設定・プロファイル・解決経路）

| 項目 | BEFORE | AFTER |
|---|---|---|
| no-assets `network list` | `default self-hosted` | 同様（**b5638f1 に既に埋め込みフォールバックあり**） |
| no-assets `network show default` / `filters show default` | rc=0 | 同様 |
| 不正なネットワークプロファイル（`schemaVersion=99`, `timeoutConnectMs=1`, `headers.Host=evil.example`）を `--config` で読み込ませ `search` | rc=0、`/search` **1 リクエスト**（**検証せず実行**） | rc=1 `E1004 invalid configuration value`、**0 リクエスト**（**起動時に停止**） |

- **#59-1**: `network`/`filters` の list/show が埋め込み既定（`src/assets.rs`）フォールバックと `--state-dir` 反映を欠いていた。`resource_ids_with_embedded` / `resolve_resource_value` / `resource_not_found` で埋め込みフォールバックを明示し、`search.rs::engine_profile` もディスク→埋め込み→アクティブの順に解決。
- **#59-2**: `config_uc::build` が `NetworkProfile` / `QuotaPolicy` の `Check::Fail` で停止していなかった（`Settings::validate` のみ `E1004`）。`network.validate()` / `quota.validate()` の `Fail` も `E1004` で起動停止に。
- **#59-3**: `NetworkProfile` / `FilterSet` が `schemaVersion != 2` を拒否していなかった（読み込むだけで値を検証しない）。仕様 configuration.md §1.3 に従い `E1004` で拒否（`Settings` / `QuotaPolicy` と同一）。
- **#59-4**: `config validate` がエンジン定義・フィルタ定義を検証していなかった。`engine.validate()`（`engine.<selector>` 接頭）＋`CompiledFilters::compile` を追加。
- **#59-5**: `config show` / `explain` の `origin_of` が `--engines-dir` / `LLMSEARCH_ENGINES_DIR` / `--no-cache` による上書きを反映していなかった。反映を追加。
- 注: #59-1 の no-assets 挙動は **b5638f1（develop tip）に既に実装済み**で BEFORE/AFTER 差分なし（issue は旧ベースラインに対する記述）。本修正は埋め込みフォールバックを明示的・堅牢に実装し、#59-2/3/4/5 の検証ギャップを閉じる。

### 2.4 #60（文書・キャッシュ・HTML 変換・コンテキスト）

| 項目 | BEFORE | AFTER |
|---|---|---|
| `outline` doc1（`<p>#hashtag_without_space</p>`） | `S2 h1 hashtag\_without\_space …`（**ハッシュタグが見出しに誤認**） | 見出し非該当（`S1 h1 Main title` / `S2 h2 Second section` のみ） |
| doc1 の img（`<img alt="preserved-alt-text">`） | `![preserved-alt-text](http://…/img.png)`（**alt 保持**） | `![](http://…/img.png)`（**alt 空**） |
| `read D1 --section D99:S2`（別文書 ID） | rc=0（**別文書節を D1 の節として返す**） | rc=1 `E1005 cannot parse the selection`（**拒否**） |
| `context.includeChildren=false`（`<state-dir>/settings.json`）で `read doc3 --section S1` | 子節 `Child A` 本文**含む**（`present=1`） | **含まない**（`present=0`） |
| `--no-children` で `read doc3 --section S1` | 子節含まない（`present=0`） | 同様（`present=0`） |
| `read --max-chars 28`（範囲 500..=100000 の外） | rc=0（**黙って 500 に丸め**） | rc=1 `E1004 --max-chars must be in the range 500..=100000` |

- **#60-1**（`document::load` が TTL / `--no-cache` / `cache.enabled=false` を無視）: b5638f1 では **CLI 経由では観測不能**（`read` / `outline` は前置き `document::fetch` を経ち、`HttpFetcher` が既に no-cache / TTL を尊うため、`load` の stale キャッシュ再利用は必ず上書きされる）。修正は `load` に `cache_entry_valid`（`cacheable` / `ttl_hours` / `expires_at` 検査）を追加し、呼び出し側で `!no_cache` / `cache.enabled` を併せて検査。**単体テスト `cache_entry_valid_gates_ttl_cacheable_and_expiry` で実行検証**。
- **#60-2**: `convert.rs` が `remove_selectors`（nav/aside/footer 等除去）より先に `pick_by_density` を走らせていた。除去後に密度判定へ。`outline::atx` を CommonMark に（先頭 ≤3 空白・`#` 1〜6 個・直後に空白/行末）に。`strip_image_alt` を追加し画像 alt を除去（document-fetching.md §3.1）。
- **#60-3**（`~~~` フェンスを ` ``` ` で閉じ、閉じられず `[truncated]`/`--- end ---` が飲み込まれる）: b5638f1 では **CLI 経由では観測不能**（HTML→Markdown 変換で `<pre>` 内の `~~~` が `\~~~` にエスケープされ、実際の閉じ経路に入らない）。`truncate_to_budget` は開きフェンスの marker（` ``` ` / `~~~`）を記憶し、閉じも同一 marker で補う。**単体テスト `truncates_and_closes_tilde_fence_with_tilde` で実行検証**。
- **#60-4**: `read` が `--children` を捨てて `!no_children` を渡すため `settings.context.includeChildren` を常に無視していた。`--no-children` > `--children` > `settings.context.includeChildren` の優先順で解決。`parse_sections` が `if *id != doc.id {}`（空ブロック）で別文書プレフィックスを拒否していなかった。`selection.document != doc.id` → `E1005`。
- **#60-5**: `--max-chars` が範囲外を `(*max).clamp(500, 100_000)` で黙って丸めていた。範囲外は `E1004`。
- **#60-6**: `read` が `chars` クォータ検査（`guard.consume_all`）より先に `EventType::Read` 履歴を記録していた（超過で失敗した read も成功として履歴に残る）。`consume_all` を履歴記録より前に移動。

## 3. 検証ゲート

| ゲート | 結果 |
|---|---|
| `python3 docs/script/verify-docs.py` | **OK**（15 specifications, 15 designs, SYNC/REF 有効） |
| `bash scripts/verify.sh --quick` | **すべて緑**（docs / release-gate / fmt-check / lint / test 全て ok） |
| `cargo test --locked`（全スイート） | **全テストバイナリ 0 failed**（lib 169 件＋各 integration / m* 含む） |
| `cargo build --locked` / `cargo clippy --locked --all-targets -- -D warnings` | 成功（警告なし） |

## 4. PR に注記する仕様-意味の変更（#57–60 範囲内・実装→仕様同期）

1. **engine-definitions.md §4.1 / §4.4: `transformArgs` を追加**。実装の `ResponseSpec.transformArgs`（`chars` / `keys` / `formats` / `pattern` / `with`）を `response` 構造テーブルと変換説明に明記（従来は引数の所在が不明確）。
2. **engine-definitions.md §4.7: `unwrap_redirect` の既定 keys を実装に一致**。`redirect_dest` → `redirect_url`, `dest`, `destination`, `to`, `link`, `href` など（実装 `extract.rs` の既定キー列と同期）。
3. **engine-definitions.md §3.4: `request.body` のテンプレート変数も送信前検証の対象と明記**（#57-3 の実装への同期）。エラーコードは `E1004`（定義の不備）のまま（実装 `ensure_valid` と一致）。

（#59-3 の `schemaVersion != 2` → `E1004` は仕様 configuration.md §1.3 と既に一致のため、仕様の文言変更は不要。）

## 5. 設計書（designs）側の同期

M19 と同様に、「Issue #NN で追跡」として記されていた設計メモを修正後の挙動に書き換えた
（`verify-docs.py` の SYNC / REF 検証済み）。18 箇所・11 ファイル:

- `docs/designs/configuration.md`: ① `document::load` のキャッシュ再利用を `cache_entry_valid`
  経由に（#60-1）。② `build` が `NetworkProfile` / `quota` の `Fail` で起動停止＋`schemaVersion != 2` を
  `E1004` 拒否（#59-2 / #59-3）。③ `config validate` にエンジン定義・フィルタ集合を追加（#59-4）。
- `docs/designs/context-assembly.md`: ④ `atx()` を CommonMark に（先頭空白 ≤3・`#` 直後空白/行末）
  に（#60-2）。⑤ `truncate_to_budget` が開きフェンスと同じマーカーで閉じる（#60-3）。
- `docs/designs/document-fetching.md`: ⑥ `load` の `cache_entry_valid` 経由＋`strip_image_alt`
  （仕様 §3.1）を明記（#60-1 / #60-2）。
- `docs/designs/engine-definitions.md`: ⑦ 未知変数はロード時 `E1004`（`ensure_valid`）/ 実行時
  `E1008` フォールバック、`request.encoding` 反映、`{{apiKey}}` 解決（#57-1 / #57-3 / #57-4）。
  ⑧ `transformArgs`（`chars`/`pattern`・`with`/`formats`/`keys`）の対象変換を
  `truncate`/`replace`/`to_date`/`unwrap_redirect` に修正（#57-3 仕様同期）。⑨ `request.body` も
  未知変数検査対象に（#57-3）。
- `docs/designs/errors.md`: ⑩ `E4003` を `401`/`403`/`429` に、`E4006`（相対 URL は `absolutize`
  未指定時破棄）をハードエラー未発行として明記（#58-1 / #58-3）。
- `docs/designs/filters.md`: ⑪ `remove_selectors` → `pick_by_density` の順、`filters` サブコマンドの
  埋め込み既定・`--state-dir`・`schemaVersion` 拒否（#60-2 / #59-1 / #59-3）。
- `docs/designs/network.md`: ⑫ エンジン別プロファイルの解決順（ディスク→埋め込み→アクティブ）と
  `network` サブコマンドの対応（#59-1）。⑬ `retryable` の `onStatus` / `onError`（timeout / connect
  分離）判定（#58-1）。⑭ `send_once` の `401`/`403`/`429`→`E4003`、`408`→`E2002`、他→`E2003`
  （#58-1）。
- `docs/designs/overview.md`: ⑮ `network` / `filters` サブコマンドの埋め込み参照を
  `resource_ids_with_embedded` / `resolve_resource_value` に（#59-1）。
- `docs/designs/search.md`: ⑯ 相対 URL の破棄（`absolutize` 時のみ自動解決）とクエリ秘匿の
  パラメータ名単位判定（#58-2 / #58-3）。⑰ `W4001` を `normalize_query` 正規化比較に（#58-5）。
- `docs/designs/session-state.md`: ⑱ `load` の `cache_entry_valid` 制御（#60-1）。

## 6. 残事 / 範囲外

- 本 PR は #57–#60 の実装修正＋仕様・設計書同期に限定。他の milestone の課題は包含しない。
- #60-1（`document::load`）と #60-3（`~~~` フェンス閉じ）は b5638f1 の CLI では観測不能（前者は前置き `fetch` が no-cache / TTL を既に尊う、後者は HTML 変換で `~~~` がエスケープされる）ため、**単体テストで実行検証**。
- #59-1 の no-assets 挙動は b5638f1 に既に実装済み（BEFORE/AFTER 差分なし）。
