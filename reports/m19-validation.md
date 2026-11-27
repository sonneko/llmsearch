# M19 検証レポート（Issue #62 対応）

対象ブランチ: `arena/b7f10cc2-llmsearch`（develop `73b7bf9e` から分岐）
日付: 2026-11-27
検証手法: **実行による検証**（主張ではなく実際のバイナリ出力）。実サイトには一切アクセスせず、
`127.0.0.1` ループバック HTTP スタブ（`reports/m19-evidence/server.py`）＋ 実バイナリのみ。

## 1. 検証環境

- ループバック スタブ: `127.0.0.1:<port>` にバインド。`/search` は 5 個の `<li><a href="/doc?n=i&token=TOKVAL…&keep=…">`
  を返す（**URL に secret を含める**伏字化検証用）。`/doc` は `<h2>` 2 節の文書。
- ドライバ: `reports/m19-evidence/driver.sh <binary> <label>`。ループバック経由で履歴を生成
  （search / outline / read / refine / search、refine は `--session` で同一セッションへ）し、
  Issue #62 の「実機検証エビデンス」を実行する。
- BEFORE バイナリ: 修正前（develop `73b7bf9e`）を `git stash` で退避したツリーから再構築。
- AFTER バイナリ: 修正済み。
- 生出力: `reports/m19-evidence/driver-before.txt` / `driver-after.txt`。

両バイナリは同一ドライバ・同一ループバックで実行し、**差分はバイナリのみ**。

## 2. BEFORE / AFTER 実行エビデンス

### 2.1 グループ a — `history clear --before`（P0 データ消失＋件数 0）

2026 年の履歴 6 件（H1–H6）を作り、`--before 2000-01-01`（= 2000 年以前削除＝本来何もしない）で clear。

| 項目 | BEFORE（修正前） | AFTER（修正後） |
|---|---|---|
| `history --before 2000-01-01T00:00:00Z clear --yes`（親形式） | `Cleared the history (0 entries)` ＋ **2026 年 6 件すべて消去**（残りは delete レコード 1 件のみ） | `Cleared the history (0 entries)`（2000 年以前は 0 件で正解）＋ **2026 年 6 件すべて保持**（残り 7 行＝6 件＋delete） |
| `history clear --before 2000-01-01T00:00:00Z --yes`（サブコマンド形式） | clap `error: unexpected argument '--before' found` rc=2 | 正常動作（`HistoryAction::Clear` に `--before` を追加） |
| `history clear --before 2027-01-01T00:00:00Z --yes`（全件削除） | clap rc=2 | `Cleared the history (6 entries)`（**実測件数 6**、従来は常に 0）＋全削除 |
| `history clear`（`--yes` なし） | `E1004 --yes is required` rc=1 | 同様（仕様 §4.3 に従い、対話確認なしで E1004） |

**BEFORE の決定的なバグ（親形式）**: `--before 2000-01-01` を指定したのに全 6 件が消去され、
件数は `0` と報告される（`commands.rs` が `before` を捨てて `clear(app, None)` を呼び、
`history_jsonl.rs` が `Ok(0)` をハードコードしていた二重バグ）。
**AFTER**: `--before` を通し（サブコマンド＆親の両形式）、指定より前のエントリだけ削除し、
件数は「全件−保持件数」の実測値を報告。

### 2.2 グループ b — `history export`（200 件打ち切り・Markdown 伏字化・デッドコード）

| 項目 | BEFORE | AFTER |
|---|---|---|
| `history --limit 3 export --format jsonl` | 6 件出力（`--limit` を**無視**、200 ハードコード） | 3 件出力（`--limit` 反映） |
| export 既定（`--limit` なし） | 6 件 | 6 件（全件） |
| **200 件上限**（ディスク 216 件） | **200 件で打ち切り（16 件を黙って欠落）** | **216 件全件出力** |
| Markdown 伏字化 | 記録時伏字化のみ | JSON パスと同じ生 secret セットで**表示時も再伏字化**（防御的深度） |

- 200 件上限の除去: ストアの `.min(200)` ハードキャップを撤廃し、limit を呼び出し側が負う。
  export は既定で全件（`None`）、`--limit` 指定時はその件数。一覧・grep は仕様 §2.1 に従い最大 200 件。
- Markdown: `renderer.out(&export_markdown(…))` のみが固定パターン `safe_text` に依存していたため、
  `renderer.redacted_value` と同一の生 secret セット（`redact_display_text`＋`secret_values(app)`）で
  表示時に再伏字化。ペイロードは記録時（`redact_json_output`）に既に伏字化済みなので、これは
  JSON パスとの整合と防御的深度であり、通常ケースでは観察可能な差は出ない（`token=***` は
  BEFORE/AFTER ともに出る）。
- デッドコード `usecases::history.rs::export`（`session: None` 固定、どこからも未呼び出し）を削除。

### 2.3 グループ c — refine 要約・未発行イベント種別・fetch ペイロード

| 項目 | BEFORE | AFTER |
|---|---|---|
| refine の `history list` 行 | `adaptation policy  0 results (0 filtered out)` | `adaptation policy (from climate adaptation)` |
| refine ペイロード | `{query, previous}` | `{query, previous}`（要約側を実ペイロードに準拠） |
| fetch ペイロード | `{url, document, chars, cached, robotsSkipped}` | `{url, finalUrl, status, bytes, durationMs, extractor, document, chars, cached, robotsSkipped}` |

- refine 要約: `summary()` が Search/Refine 共有分岐で存在しない `results`/`filtered` を読んでいたため
  常に `0 results (0 filtered out)`。Refine を分岐させ実ペイロード `{query, previous}` を表示。
- fetch ペイロード: 仕様 §1.1 の `{url, finalUrl, status, bytes, durationMs, extractor}` を充足するよう、
  `DocumentMeta` に `status`/`bytes`/`duration_ms` を追加（`#[serde(default)]` で旧 meta.json 後方互換）、
  `HttpFetcher::fetch_fresh` が HTTP レスポンスから計測・記録。実運用で有用な
  `document`/`chars`/`cached`/`robotsSkipped` は併記（spec フィールドの**超集合**）。
  `finalUrl`/`extractor` は既存 meta から取得。
- 未発行イベント種別 `engine`/`config`: `src/` 全体で一度も発行されない（実発行は
  search/next/refine/fetch/outline/read/select/quota/delete の 9 種）。`engine` はエントリ
  トップレベル `engine` フィールド（§1.1 線 39）と冗長、`config` はユーザ操作トリガを持たないため、
  **実装→仕様同期**として `EventType` 列挙と §1.1 の行を削除（`parse` は未知種別を `None` で無視し、
  既存ファイルには該当レコードが存在しないため安全）。

### 2.4 グループ d — 永続化配置・破損セッション隔離ファイル名・headings

| 項目 | BEFORE | AFTER |
|---|---|---|
| state ディレクトリ配置 | `cache/robots.json, counters.json, documents/<hash>/{meta.json,body.md}, history/history.jsonl, sessions/<id>.json` | 同様（**変更不要**。issue が引用した「仕様」パス `state/cache/documents/<hash>.md` / `state/history/events.jsonl` / `state/quota/counters.json` は実際の仕様 file と一致せず、**実仕様 session-state.md §3.2 は実装と既に一致**） |
| 破損セッション隔離ファイル名 | `<id>.corrupt-<stamp>`（末尾 `.json` なし） | 同様（**仕様側を実装に合わせて `.json` なしに統一**、下参照） |
| `meta.json` の `headings` | 件数（`usize`） | 同様（仕様コメントを「見出し数（整数。構成は body.md から再生成）」に明確化） |

- 破損セッション隔離: 実装 `mod.rs::quarantine` は `path.with_extension("corrupt-<stamp>")` で
  `<id>.corrupt-<stamp>`（`.json` なし）を生成。session-state.md §4.2 は従来 `<id>.corrupt-<timestamp>.json`
  と述べ、quotas.md:175 は `.json` なしで**2 仕様が不一致**。
  **判断: 実装→仕様（`.json` なしに統一）**。理由: セッション読込は `sessions/` を `*.json` 拡張子で
  列挙して読む（`session_store.rs`）ため、隔離名に `.json` をつけると**次回ロードで再拾い→再解析→
  再隔離のループ**になる。`.json` なしは `*.json` グラブで拾われず安全。これは state-dir 互換性に
  影響しない（新規に生成するファイル名のみ）ため、優先方向「実装→仕様同期」を採用。
- 検証: 壊れたセッション JSON を作り `--session <id> next` でロード→隔離。
  BEFORE/AFTER とも `20200101-000000-abcd.corrupt-<stamp>`（`.json` なし）を確認。

## 3. 検証ゲート

| ゲート | 結果 |
|---|---|
| `python3 docs/script/verify-docs.py` | **OK**（15 specifications, 15 designs, SYNC/REF 有効） |
| `bash scripts/verify.sh --quick` | **すべて緑**（docs / release-gate / fmt-check / lint / test 全て ok） |
| `cargo test --locked`（全スイート） | **全 21 テストバイナリ 0 failed**（lib 166 件＋各 integration/m* 含む） |
| `cargo build --locked` | 成功（警告なし、lint とも green） |

## 4. PR に注記する仕様-意味の変更

1. **`history clear --before` は新規オプション**（history.md §4.3 に追記）。従来仕様は `--yes` のみ。
   data-loss 修正（#62 グループ a）のため実装＆仕様両方に追加。
2. **export は既定で全件**（history.md §2.1）。従来「export の既定は 200 件」を「全件、`--limit` 指定時は
   その件数」に変更。一覧・grep の 200 件上限は維持。
3. **`engine`/`config` イベント種別を削除**（history.md §1.1 行＋`EventType` 列挙）。未発行かつ
   `engine` は冗長（トップレベルフィールド既存）・`config` はトリガなしのため、実装→仕様同期。
4. **破損セッション隔離ファイル名を `.json` なしに統一**（session-state.md §4.2）。`*.json` グラブで
   再拾いされるのを防ぐため（実装→仕様）。
5. **`meta.json` の `headings` は「見出し数（整数）」**（session-state.md §3.2 コメント明確化）。
   構成は `body.md` から再生成する設計。
6. **fetch ペイロードに `finalUrl`/`status`/`bytes`/`durationMs`/`extractor` を追加**（実装側）。
   仕様 §1.1 の記述はそのまま充足（プロース記述なので仕様の文言変更は不要）。

## 5. 設計書（designs）側の同期

M18 で「Issue #62 で追跡」として記されていた設計メモを、修正後の挙動に書き換えた（SYNC/REF は
verify-docs が検証済み）:
- `docs/designs/history.md`: export 200 ハードコード／Markdown 伏字化漏れの追跡記述 → 「export は
  既定で全件、Markdown も JSON と同じ生 secret セットで伏字化」に変更。`clear --before`／件数 0／
  refine 要約／未発行 `engine`・`config` の追跡記述 → 修正後の挙動（`--before` 対応・実測件数・
  `{query, previous}` 要約・`engine`/`config` 削除）に変更。
- `docs/designs/session-state.md`: 隔離ファイル名の「`.json` 未付は Issue #62 で追跡」→ 「あえて
  `.json` を持たせず `*.json` グラブに再拾われない理由」に変更。

## 6. 残事 / 範囲外

- 本 PR は #62 の 4 グループの実装修正＋仕様・設計書同期に限定。他の milestone の課題は包含しない。
