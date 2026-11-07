# 設計書: 概要（Overview）

本書は `REF:docs/specifications/overview.md` に対応する実装設計を記述する。ソースコードの実体は未作成のため、本書のパスは実装予定の配置を示す（`REF:docs/plans.md#m2-implement` 完了時点で `REF:` 参照へ切り替える）。

---

## 1. 目的と背景

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

<!-- 仕様の目的を実装でどう担保するかを記述する -->

### 1.1 解決したい課題

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

段階的開示をパイプラインの各工程で強制する。検索工程・アウトライン工程・節出力工程で戻り値の型を分離し、検索工程の戻り値型には本文フィールドを持たせない。これにより「一覧表示のつもりが本文を出してしまう」実装を型検査で排除する。

### 1.2 提供する価値

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

対話スキン（`src/ui/cli/`）とエージェント向けサブコマンド（`src/ui/cli/commands/`）が同一のユースケース層（`src/usecases/`）を呼ぶ構成とする。価値の実装はユースケース層に集約し、UI 層は入力の解釈と出力の整形のみを担う。

### 1.3 想定利用者

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

エージェント向けには `REF:docs/designs/agent-integration.md` の JSON イベントを、開発者向けには `REF:docs/designs/cli-interface.md` の対話メニューを提供する。利用者ごとに別コードパスを作らず、出力レンダラのみを差し替える。

---

## 2. 設計原則

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 2.1 段階的開示（Progressive Disclosure）

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

段階ごとに戻り値型を分ける。検索は `SearchView`（タイトル・URL・スニペットのみ）、アウトラインは `OutlineView`（見出しと文字数のみ）、節出力は `SectionView`（選択節の本文のみ）とし、下位段階の型から上位段階の本文へ到達できないようにする。定義は `src/core/view/` に置く。

### 2.2 コンテキスト予算の明示

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

予算計算を純粋関数として `src/core/context/budget.ts` に切り出し、書記素単位で計算する。切り詰めの判定と注記生成を同一関数で行い、表示側で再計算しない。

### 2.3 探索と取得の分離

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

取得は `DocumentFetcher` ポート（`src/ports/document-fetcher.ts`）の背後に置き、検索ユースケースは取得ポートに依存しない。検索結果の選択が確定してはじめて取得ユースケースが呼ばれる。

### 2.4 決定論的な出力

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

時刻・乱数・TTY 判定は `Clock` / `IdGenerator` / `Env` ポート（`src/ports/`）を通じて注入する。変換・抽出の純粋関数は入力のみに依存させ、同一入出力をテストで検証する。

### 2.5 ローカル完結とプライバシー

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

永続化は状態ディレクトリ（`REF:docs/designs/session-state.md`）に閉じる。外部送信を行うのは検索プロバイダと文書取得の HTTP のみであり、テレメトリ用のコードパスを持たない。

### 2.6 礼儀正しいアクセス

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

レート制限・robots.txt 判定を `RefusalPolicy`（`src/usecases/net/refusal-policy.ts`）として独立させ、取得・検索の両方から共通に呼ぶ。テストではポリシーを差し替え、実通信なしで検証する。

---

## 3. 用語

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

仕様の用語とコード上の型名の対応を次に固定する。用語の追加時はここに対応を追記する。

| 用語 | 型・シンボル | 配置（実装予定） |
|---|---|---|
| セッション | `SessionState` | `src/core/session/state.ts` |
| 検索結果 | `SearchResult` | `src/core/model/result.ts` |
| ページ | `PageSlice` | `src/core/search/paging.ts` |
| 文書 | `Document` | `src/core/model/document.ts` |
| 見出し | `Heading` | `src/core/model/heading.ts` |
| アウトライン | `Outline` | `src/core/outline/outline.ts` |
| 節 | `Section` | `src/core/model/section.ts` |
| スニペット | `Snippet` | `src/core/model/result.ts` |
| コンテキスト予算 | `ContextBudget` | `src/core/context/budget.ts` |
| プロバイダ | `SearchProvider`（ポート） | `src/ports/search-provider.ts` |
| 状態ディレクトリ | `StatePaths` | `src/adapters/state/paths.ts` |
| 対話モード | `InteractiveSession` | `src/ui/cli/interactive.ts` |

---

## 4. 利用シナリオ

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 4.1 対話モードでの調査

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

`REF:docs/designs/cli-interface.md` のメニュー実装がユースケース層を順に呼ぶ。中断・復帰はメニューの状態機械で表現し、各状態の遷移を単体テストする。

### 4.2 エージェントからの 1 回呼び出し

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

各サブコマンドはセッション状態を読み込み、ユースケースを 1 つ実行し、イベントを出力して状態を書き戻す。プロセスをまたぐ暗黙のキャッシュを持たない。

### 4.3 過去の閲覧の再参照

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

履歴レコードから文書 ID と節識別子を引き、キャッシュが有効なら取得ポートを呼ばずに節を再構成する。

---

## 5. スコープと非目標

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 5.1 初期リリースのスコープ

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

スコープの各項目は `REF:docs/plans.md` のマイルストーンで実装する。実装順序は同ファイルのフェーズに従う。

### 5.2 非目標

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

非目標に該当する機能は、依存の追加も行わない。将来拡張の候補は `REF:docs/plans.md#unplanned` に記録し、初期リリースのコードに準備用の分岐を残さない。

### 5.3 将来拡張の候補

状態: 未実装・未計画（REF:docs/plans.md#unplanned）

拡張点は設計に組み込むが、機能自体は実装しない。

---

## 6. 全体アーキテクチャ

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

### 6.1 パイプライン

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

```
ui/cli（対話・サブコマンド）──┐
ui/render（text / json）    ──┴─► usecases ─► core（純粋ロジック）
                                    │
                                    ├─► ports（SearchProvider, DocumentFetcher, StateStore,
                                    │          HistoryStore, Clock, IdGenerator, Env, Reporter）
                                    └─► adapters（http, state/fs, providers/*, extract/*）
```

依存方向は `ui → usecases → core`、`usecases → ports ← adapters` とする。`core` は `ports` と `adapters` に依存しない（`tests/architecture/layering.test.ts` で import 文を走査して検査）。

### 6.2 状態の保持単位

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

セッション・文書・履歴の 3 種を `StateStore` / `HistoryStore` ポートで抽象化し、アダプタとしてファイルシステム実装（`src/adapters/state/fs-store.ts`）を用意する。テストではメモリ実装（`src/adapters/state/memory-store.ts`）を用いる。

### 6.3 拡張点

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-implement）

1. **プロバイダ追加**: `SearchProvider` ポートの実装追加のみで完結させる。登録方法は `REF:docs/designs/search-providers.md` §1.1 に記述する。
2. **変換器の差し替え**: `HtmlToMarkdown` ポートを設け、既定実装を `src/adapters/extract/readability.ts` に置く。
3. **出力形式の追加**: `Renderer` インターフェース（`text` / `json`）を出力形式ごとに実装する。
4. **未知設定の温存**: 設定パーサで未知キーを保持し、書き戻し時に再出力する（`REF:docs/designs/configuration.md` §3.2）。
