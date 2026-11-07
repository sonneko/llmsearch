# 設計書: 検索プロバイダ

本書は `REF:docs/specifications/search-providers.md` に対応する実装設計を記述する。パスは実装予定の配置である。

---

## 1. 対象プロバイダ

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 1.1 一覧

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

各プロバイダは `src/adapters/providers/<name>/index.ts` で `SearchProvider` ポートを実装し、`src/adapters/providers/registry.ts` に登録する。レジストリは `name`, `requiresApiKey`, `create(config)` を持ち、設定検証（`E4002` の判定）もここで行う。

- `duckduckgo`: HTML エンドポイントの応答を解析する。解析は `parse.ts` に分離し、文字列から `SearchResult[]` を返す純粋関数としてテストする。
- `searxng`: JSON API（`format=json`）を既定とし、無効なインスタンスでは HTML 解析へフォールバックする。
- `brave`: JSON API を利用し、キーは `search.brave.api_key_env` が指す環境変数から取得する。

### 1.2 プロバイダの選択順

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

選択順の解決は `src/usecases/search/select-provider.ts` で行い、解決結果と「どのソースで決まったか」を `ProviderChoice` として返す。`--verbose` と `config show` はこの由来情報を表示する。解決したプロバイダが利用不能な場合は暗黙のフォールバックを行わず `E4002` を生成する。

---

## 2. 検索結果の正規化

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 2.1 フィールド

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

正規化は各プロバイダの `parse.ts` → 共通の `normalizeResults(raw[], limit)`（`src/core/search/normalize.ts`）の 2 段で行う。`normalizeResults` はタイトルの空白正規化、スニペットの切り詰め、サイト名の導出、重複除去、識別子付与を順に行う純粋関数とする。

### 2.2 URL の正規化

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`src/core/url/normalize.ts` に `normalizeUrl()` と `stripTracking()` を置く。追跡パラメータの一覧は定数配列とし、判定は「パラメータ名が一覧に完全一致、または `utm_` で始まる」とする。リダイレクト URL の実 URL 抽出は、クエリの既知キー（`uddg`, `url`, `q`, `target`）を順に試すヒューリスティクスとし、抽出できない場合のみ HTTP で解決する。

### 2.3 重複の除去

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

正規化 URL をキーとした `Set` で初出のみ残す。識別子は重複除去後の配列順に `R<n>` を振る。元の順位は `rank` として保持し、表示に用いる。

### 2.4 並び順

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

プロバイダ応答の順序をそのまま保持する。ソート処理を入れないことをテストで担保する（同一入力で順序が保存されることを検証）。

---

## 3. ページング

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 3.1 ページサイズ

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`search.limit` の検証は設定スキーマで 1〜30 の範囲制約として行う。`PageSlice`（`src/core/search/paging.ts`）は `items`, `page`, `hasMore`, `nextOffset` を持ち、`nextOffset` はプロバイダごとの `offset` または `page` パラメータの基底値を返す。

### 3.2 次ページの取得

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`next` ユースケースはセッションの `page` を 1 進め、既存 `results` の末尾の識別子番号から連番を継続する。識別子生成は `src/core/search/ids.ts` の `nextResultId(existing)` に閉じる。

### 3.3 取得上限と終端

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`search.max_results` とプロバイダ上限の小さい方を採用し、到達時は `hasMore=false` を返す。空ページは `hasMore=false` かつエラーなしとして扱う。

### 3.4 失敗時の扱い

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

次ページ取得の失敗は `E4xxx` として返し、状態書き戻しを行わない（`page` を進めない）。したがって再実行で同じページを再試行できる。

---

## 4. 検索語の修正

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 4.1 挙動

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`refine` ユースケースは `SessionState.queries` に新しいエントリを追加し、`activeQuery` を切り替える。結果一覧は `results[queryIndex]` としてスロットを分け、過去の一覧を保持する。

### 4.2 検索語の履歴

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`!<n>` の展開は `src/core/parse/reuse.ts` で行い、範囲外は `E1005` とする。展開後の文字列も正規化を通す。

### 4.3 入力の正規化

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`normalizeQuery()`（`src/core/parse/query.ts`）で空白・全角空白・制御文字を処理する。512 文字超過は切り詰め、`W1003`（検索語の切り詰め）を警告として返す。

---

## 5. レート制限と礼儀

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 5.1 ホスト単位の制限

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`RateLimiter`（`src/usecases/net/rate-limiter.ts`）がホストごとの最終アクセス時刻を保持し、待機が必要な場合は待機して `W2001` を記録する。待機は `Clock.sleep()` ポート経由とし、テストでは仮想時計で待機時間を検証する。

### 5.2 同時実行

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

取得は `pLimit` 相当の小さな同時実行制御（自前の `src/usecases/net/limited-pool.ts`、同時 2 件）で逐次実行し、進捗を `Reporter.info` へ出す。

### 5.3 robots.txt

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`robots-parser` を用い、ホスト単位で判定結果を `cache/robots.json` にキャッシュする（TTL 既定 1 時間）。取得失敗（404・タイムアウト）は許可として扱い、`network.respect_robots=false` の場合は判定自体を行わない。

### 5.4 User-Agent

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

既定値は `package.json` の `version` と `repository` から組み立てる。空文字の設定はスキーマの `min(1)` 制約で拒否する。
