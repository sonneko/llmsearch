# 設計書: 概要（Overview）

本書は `REF:docs/specifications/overview.md` に対応する実装設計を記述する。

---

## 1. 設計原則

状態: 実装完了（REF:docs/specifications/overview.md）

<!-- アプリケーションの設計原則の実装方針を記述する -->

### 1.1 仕様先行の実装アプローチ

状態: 実装完了（REF:docs/developers.md）

すべての機能はまず `docs/specifications/` に明文化され、その後 `docs/designs/` に実装設計が記述され、最後にコードとして実装される。

### 1.2 テスト容易性の確保

状態: 実装完了（REF:docs/developers.md）

純粋関数とポート・アダプタパターンにより、OS・ファイルシステム・ネットワークへの依存を `adapters/` 層に閉じ込め、`core/` 層は単体テストで完全に検証可能にする。

### 1.3 段階的拡張の設計

状態: 実装完了（REF:docs/developers.md）

`#[serde(flatten)]`（または同等の機構）により未知フィールドを温存し、将来の機能追加時に既存データの再マイグレーションを不要にする。

---

## 2. 用語

状態: 実装完了（REF:docs/specifications/overview.md）

<!-- 用語の実装対応を記述する -->

---

## 3. アーキテクチャ

状態: 実装完了（REF:docs/specifications/overview.md）

### 3.1 層構造

状態: 実装完了（REF:docs/developers.md）

```
src/
├── core/        # 純粋ドメイン層（I/O を含まない。外部依存禁止）
├── ports/       # 外部境界 trait（storage, clock, env 等のインターフェース）
├── adapters/    # 環境別実装（storage_fs, storage_idb, clock_system 等）
└── ui/          # ユーザーインターフェース層（optional feature で制御）
```

各層の責務と制約は `REF:docs/specifications/overview.md` §3.1 の通り。

### 3.2 許可クレート・依存リスト

状態: 実装完了（REF:docs/developers.md）

テストコードによる自動検査で許可リスト外の依存を検出する。

### 3.3 将来の拡張点

状態: 実装完了（REF:docs/developers.md）

1. **未知キーの温存**: 設定ファイルのパーサで未知フィールドを `HashMap` に保持
2. **未知フィールド行の温存**: パーサで未知 `key:` を警告しつつ無損失保持
3. **コード予約**: 診断・イベント ID を事前に定数として定義
