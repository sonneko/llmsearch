# 設計書: 概要（Overview）

本書は `REF:docs/specifications/overview.md` に対応する実装設計を記述する。プロジェクトは Rust（`edition 2021`、`rust-toolchain.toml` で 1.90 系に固定）で実装する。

---

## 1. 目的と背景

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

<!-- 目的を実装でどう担保するか、クレート構成の単位で記述する -->

### 1.1 解決したい課題

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

段階的開示を型で強制する。`core` 層に `SearchHit`（本文を持たない）、`Outline`（見出しと文字数のみ）、`SectionBody`（選択節の本文のみ）を定義し、検索経路の戻り値から本文へ到達できない構造にする。エンジン依存は JSON 評価器（`adapters/engine/`）に閉じ込め、`core` はエンジン名を知らない。

### 1.2 提供する価値

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

価値の実装はユースケース層に集約し、UI 層は入力の解釈と描画のみを担う。エンジン非依存は「エンジン定義の評価器が 1 種類しかない」ことではなく「エンジン固有の分岐が存在しない」ことをテスト（`tests/architecture.rs` の禁止パターン検査）で担保する。

### 1.3 想定利用者

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

エージェント向けの JSON 出力と、人間向けの対話メニューは同じユースケース関数を呼び、描画器のみを差し替える。クォータ残量は両方の出力に含め、計画立案に使えるようにする。

---

## 2. 設計原則

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 2.1 段階的開示（Progressive Disclosure）

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`core/view.rs` に段階ごとのビュー型を置く。`SearchHit` にスニペット以外の本文フィールドを持たせず、`Outline` から節本文を取り出す API を設けない。

### 2.2 コンテキスト予算の明示

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

予算計算は `core/context/budget.rs` の純粋関数とし、書記素ではなく `char` 数で計測する（表示幅は `ui/render/width.rs` が `unicode-width` で別途計算する）。切り詰めと注記生成を同一関数で行う。

### 2.3 探索と取得の分離

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`ports::HttpClient` を検索と取得で共用するが、ユースケースは分ける。`usecases::search` は `ports::Fetcher` に依存しない。

### 2.4 エンジン非依存（宣言的定義）

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`core::engine` にエンジン定義の型（`EngineDef`, `RequestSpec`, `ExtractSpec`）を置き、`core::extract` に CSS セレクタ評価と JSON パス評価の純粋関数を置く。`adapters::engine` は HTTP 応答を `core` の型へ流し込むだけであり、エンジン識別子による `match` を持たない。

### 2.5 宣言的設定による挙動の外部化

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

設定のマージは `usecases::config::resolve` で 5 層（既定値・利用者設定・プロファイル・環境変数・引数）を畳み込み、各項目の由来を `Origin` として保持する。未知キーは `serde_json::Map` に温存する（`#[serde(flatten)] extra`）。

### 2.6 決定論的な出力

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

時刻・乱数・端末判定は `ports::{Clock, IdGen, Env}` を通す。テストでは固定実装（`tests/support/ports.rs`）を注入し、`SystemTime::now()` と `rand` 系の直接利用を禁止パターンとして検査する。

### 2.7 ローカル完結とプライバシー

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

永続化は状態ディレクトリに閉じる。認証情報は設定の `value_env` 間接参照のみを許し、`serde` の `skip_serializing` により表示・保存の双方から値を排除する。

### 2.8 礼儀と抑制

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`usecases::guard` にレート制限（短時間の連続抑止）とクォータ（総量制限）を別々に実装し、取得・検索の全経路がここを通る。上限到達は `errors::AppError::Quota` として返し、暗黙の継続を行わない。

---

## 3. 用語

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

仕様の用語とコード上の型名の対応を固定する。用語追加時はここへ追記する。

| 用語 | 型・シンボル | 配置（実装予定） |
|---|---|---|
| セッション | `SessionState` | `src/core/session.rs` |
| 検索結果 | `SearchHit` | `src/core/model.rs` |
| ページ | `PageSlice` | `src/core/search/paging.rs` |
| 文書 | `Document` | `src/core/model.rs` |
| 見出し | `Heading` | `src/core/model.rs` |
| アウトライン | `Outline` | `src/core/outline.rs` |
| 節 | `Section` | `src/core/model.rs` |
| エンジン定義 | `EngineDef` | `src/core/engine.rs` |
| 抽出ルール | `FieldSpec`, `ExtractSpec` | `src/core/engine.rs` |
| ネットワークプロファイル | `NetworkProfile` | `src/core/config/network.rs` |
| フィルタ | `FilterSet` | `src/core/filter.rs` |
| クォータ | `QuotaPolicy`, `Counter` | `src/core/quota.rs` |
| カウンタ | `Counters` | `src/core/quota.rs` |
| 状態ディレクトリ | `StatePaths` | `src/adapters/state/paths.rs` |
| 対話モード | `InteractiveSession` | `src/ui/cli/interactive.rs` |

---

## 4. 利用シナリオ

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 4.1 対話モードでの調査

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

メニューは状態機械（`ui/cli/screens`）として実装し、遷移関数を純粋関数としてテストする。非同期ランタイム（tokio）は `ui/cli/runtime.rs` の 1 か所で `block_on` し、画面コードは同期で書く。

### 4.2 エージェントからの 1 回呼び出し

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

各サブコマンドは状態を読み、ユースケースを 1 つ実行し、イベントを出力して書き戻す。出力には常に `quota` オブジェクト（残量）を含める。

### 4.3 過去の閲覧の再参照

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

履歴レコードから文書 ID と節 ID を引き、キャッシュが有効なら `ports::Fetcher` を呼ばずに節を再構成する。

### 4.4 エンジンの切り替えと検証

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`engines validate` はエンジン定義のデシリアライズと参照解決（`value_env` の存在、セレクタ構文、JSON パス構文）を検査する。`engines test` はクォータを消費する点を明示し、`--dry-run` でリクエスト定義のみを表示できるようにする。

---

## 5. スコープと非目標

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 5.1 初期リリースのスコープ

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

スコープの各項目は `REF:docs/plans.md` のマイルストーンに割り当てる。実装順序は同ファイルのフェーズに従う。

### 5.2 非目標

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

非目標に該当する機能は依存の追加も行わない。将来拡張は `REF:docs/plans.md#unplanned` に記載し、準備用の分岐を残さない。

### 5.3 将来拡張の候補

状態: 未実装・未計画（REF:docs/plans.md#unplanned）

拡張点は設計に組み込むが、機能自体は実装しない。

---

## 6. 全体アーキテクチャ

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 6.1 パイプライン

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

```
ui/cli（対話・サブコマンド）
  └─► ui/render（text / json）
        └─► usecases（search / fetch / outline / read / history / config / quota）
              ├─► core（model, engine, extract, markdown, outline, context, filter, quota）
              └─► ports（HttpClient, Fetcher, StateStore, HistoryStore, Clock, IdGen, Env, Reporter）
                    └─► adapters（http/reqwest, state/fs, engine/eval, extract/density）
```

依存方向は `ui → usecases → core`、`usecases → ports ← adapters`。`core` は `ports`・`adapters` に依存しない（`tests/architecture.rs` がソースの `use` 文を走査して検査する）。

### 6.2 設定駆動の境界

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`core::engine::EngineDef` のフィールド以外に、エンジン固有の知識をコードへ置かない。禁止パターン（特定ドメインの文字列リテラル、エンジン ID による分岐）を `tests/architecture.rs` で検出する。組み込み既定エンジンも `engines/*.json` として同梱し、コードへ埋め込まない。

### 6.3 状態の保持単位

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

セッション・文書・履歴・カウンタの 4 種を `StateStore` / `HistoryStore` / `CounterStore` トレイトで抽象化し、ファイル実装（`adapters/state/fs_*.rs`）とメモリ実装（`tests/support/memory_store.rs`）を用意する。

### 6.4 拡張点

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

1. **エンジン追加**: JSON の追加のみ。応答種別は `response.kind`（`html` / `json`）で宣言する。
2. **抽出方式の追加**: `trait Extractor` を追加実装し、`response.kind` への登録を 1 か所（`adapters/engine/registry.rs`）に限定する。
3. **ネットワーク実装の差し替え**: `ports::HttpClient` の実装差し替えで完結させる。
4. **出力形式の追加**: `trait Renderer`（`text` / `json`）を追加実装する。
5. **未知設定の温存**: `#[serde(flatten)] extra` に保持し、書き戻し時に再出力する。
