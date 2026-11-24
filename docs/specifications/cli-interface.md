# 仕様書: コマンドラインインターフェース

本書は `llmsearch` の起動方法、対話メニュー、非対話サブコマンド、出力チャネル、入力規則を定義する。

---

## 1. 起動とモード

### 1.1 起動形態

引数なしで起動した場合は対話モードとなる。サブコマンドを指定した場合は非対話モードとして 1 操作を実行して終了する。対話モードでも同一の操作が内部的に実行される。

### 1.2 モード判定

標準入力が端末でない場合、対話モードは起動せず、エラー（`E1001`）を返して非対話サブコマンドの利用を促す。標準出力が端末でない場合でも動作は変えず、番号メニューは標準エラーへ出力する。

### 1.3 共通オプション

| オプション | 既定 | 説明 |
|---|---|---|
| `--json` | 無効 | 標準出力を JSON イベント形式にする |
| `--session <id>` | 自動採番 | 既存セッションを継続する |
| `--engine <id[@variant]>` | 設定値 | 検索エンジンを一時的に切り替える |
| `--language <tag>` | 設定値 | 検索エンジンへ送る言語（`search.language`）を一時的に切り替える |
| `--network <id>` | 設定値 | ネットワークプロファイルを一時的に切り替える |
| `--filters <id>` | 設定値 | フィルタ集合を一時的に切り替える |
| `--quota <id>` | 設定値 | クォータポリシーを一時的に切り替える |
| `--no-filter` | 無効 | 結果・本文フィルタを無効化する（既定の除去要素は維持する） |
| `--no-cache` | 無効 | キャッシュの読み書きを行わない |
| `--refresh` | 無効 | キャッシュを無視して再取得する |
| `--config <path>` | 既定の探索順 | 設定ファイルを明示指定する |
| `--state-dir <path>` | 設定値 | 状態ディレクトリを明示指定する |
| `--engines-dir <path>` | なし | エンジン定義の追加探索ディレクトリ（複数指定可） |
| `--no-color` | 自動判定 | 装飾を無効化する |
| `--color` | 無効 | 標準出力が端末でなくても装飾する |
| `--quiet` | 無効 | 進捗表示を抑制する |
| `--verbose` | 無効 | 診断情報を標準エラーへ追加する |
| `--help` / `--version` | — | 使用方法・版数を表示して終了する |

---

## 2. 対話モードのメニュー

### 2.1 検索結果メニュー

検索直後に、結果一覧と次のメニューを表示する。番号は仕様として固定であり、項目の入れ替えを行わない。表示はすべて英語である（メッセージの言語は常に英語。`errors.md` §4.2）。

```text
session 20261107-142233-a7f3 (resume with --session 20261107-142233-a7f3)
query: search: 7 results (filters: all 7 shown) / 13/50 (session)
Q: climate adaptation    engine: duckduckgo
[R1] Climate Adaptation Platform
     https://example.jp/adaptation
     A portal that collects national adaptation plans and local case studies.…
[R2] …

  1) Quit
  2) Select results (multiple allowed)
  3) Show results from the 8th onward
  4) Edit the query
> 
```

先行する `session …` 行はセッション開始時に 1 回だけ出す（`--quiet` では出さない）。`search: …` の情報行は §2.5 の形式で、末尾にフィルタ適用後の件数を続ける。対話モードの `Q: …` 行には `showing:` を付けない（非対話モードの `search` は §3.2 のとおり `showing: 1-3` を付ける）。

- **1) Quit**: セッションを閉じ、終了コード `0` で終了する。
- **2) Select results**: 選択用の入力待ちへ移る（§2.2）。選択後は文書取得とアウトライン表示へ進む。
- **3) Show results from the 8th onward**: 次のページを取得して追記表示する。取得済みの結果は再表示せず、識別子は連番を継続する。フィルタ適用後の件数と除外件数を 1 行で示す。
- **4) Edit the query**: 現在の検索語を初期値として編集入力させ、新しい検索を実行する。以前の結果一覧はセッション内に保持し、履歴から再参照できる。

### 2.2 結果選択の入力規則

- 受け付ける形式: 単一番号 `3`、複数指定 `1,3`、範囲 `1-3`、識別子 `R1,R3`、混在 `R1,3-4`。
- 区切り文字はカンマ、空白、読点のいずれも受け付ける。
- 空入力はキャンセルとして扱い、検索結果メニューへ戻る。
- 存在しない番号、範囲外、重複は警告を出して再入力させる。3 回連続で不正な場合はメニューへ戻る。
- 選択順は読み取り順として保持し、アウトラインの表示順に反映する。

### 2.3 アウトライン・メニュー

選択された結果を取得・変換し、各文書の見出し（既定 h1〜h3）を表示した後に次のメニューを表示する。階層は 1 段あたり 2 文字の字下げで表し、子見出しを持つ行の末尾には `↓` を付ける。

```text
[D1] Climate Adaptation Platform   https://example.jp/adaptation
S1    h1  Climate Adaptation Platform            (920 chars) ↓
  S2    h2  1. Climate impact assessment          (2,140 chars)
  S3    h2  2. Planning adaptation                (3,560 chars) ↓
    S4    h3  2.1 Regional adaptation plans       (1,240 chars)
[D2] …

  1) Quit
  2) Read sections (multiple allowed)
  3) Change the outline depth
  4) Back to the results
  5) Show the history
>
```

- **2) Read sections**: `D1:S3` 形式、または `S3`（単一文書の場合）で指定する。複数指定可。指定順に本文を出力する。
- **3) Change the outline depth**: 2〜4 の範囲で再指定し、同じ文書のアウトラインを再表示する。再取得は行わない。
- **4) Back to the results**: 取得済み文書と選択状態を保持したまま、検索結果メニューへ戻る。
- **5) Show the history**: 直近の履歴を表示する。

### 2.4 節本文の表示形式

節本文は、出所が分かるヘッダ行と、信頼できない外部コンテンツであることを示すマーカーの後に出力する。

```text
--- D1:S3 | 2. Adaptation planning | h2 | 3,560 chars ---
[untrusted] https://example.jp/adaptation
<節の Markdown 本文>
--- end ---
```

予算超過で切り詰めた場合は、`--- end ---` の直前に `[truncated: shown 8000 / total 12000 chars]` の形式で、表示した字数と全体の字数を 1 行出力する。見出しの階層を保つため、切り詰めは行境界で行う。

節を出力した直後には、`(output 3560 chars / budget 8000 chars / 3560/200000 (session))` の形式で、出力した字数・当該節に割り当てた予算・セッションの文字消費を 1 行出力する。

### 2.5 通常の情報行

対話モードでは、操作の結果を 1 行で示す。表示するのは次の情報である。

| 状況 | 表示例 |
|---|---|
| 検索後 | `search: 7 results (filters: all 7 shown) / 13/50 (session)` |
| 取得後 | `fetched D1 4200 chars / 9/100 (session)` |
| 上限到達 | `quota exceeded: day.searches 500/500 (resets 2026-11-08T00:00 local)` |

### 2.6 中断と復帰

入力の終端（`Ctrl+D`）は終了として扱い、終了コード `0` を返す。`1) Quit` でも同じく終了する。割り込み（`SIGINT`）の扱いは独自に定めず、端末と OS の既定動作に従う。

---

## 3. 非対話サブコマンド

### 3.1 コマンド一覧

| コマンド | 目的 | 主なオプション |
|---|---|---|
| `search <query>` | 検索して結果一覧を出力する | `--limit`, `--page` |
| `next` | 直前の検索の次ページを取得する | `--query`（セッションの直前クエリを上書きする） |
| `refine <query>` | 検索語を変更して再検索する（変更前はセッションの直前クエリ） | なし |
| `outline <target>` | 文書を取得・変換し、見出し一覧を出力する | `--depth` |
| `read <target>` | 節本文を出力する | `--section`, `--max-chars`, `--children` / `--no-children` |
| `history [list\|show\|grep\|export\|clear]` | 履歴の参照・出力・削除 | `--limit`, `--query`, `--type`, `--before`, `--yes`（`clear`） |
| `history export` | 履歴を書き出す | `--format <形式>`（既定 `markdown`。`json` または `jsonl` で JSON Lines、それ以外は Markdown） |
| `engines [list\|show\|validate\|test]` | エンジン定義の参照・検証・試行 | `show --raw`、`test --query`, `test --dry-run`, `test --raw` |
| `network [list\|show\|validate]` | ネットワークプロファイルの参照・検証 | なし |
| `filters [list\|show\|validate]` | フィルタ集合の参照・検証 | `show --raw` |
| `quota [show\|reset\|check]` | クォータの参照・初期化・事前確認 | `reset --scope`, `reset --yes`、`check --kind`, `check --amount` |
| `config [show\|path\|init\|validate\|explain]` | 設定の表示・初期化・検証 | `init --force` |
| `doctor` | 環境・ネットワーク・設定の診断 | なし |

`--json` / `--session` / `--engine` / `--language` / `--network` / `--filters` / `--quota` / `--no-filter` / `--no-cache` / `--refresh` / `--config` / `--state-dir` / `--engines-dir` / `--no-color` / `--color` / `--quiet` / `--verbose` はルートで定義したグローバルオプションで、すべてのサブコマンドで受け付ける（§1.3）。

### 3.2 対象の指定方法

`outline` と `read` の `<target>` は次のいずれかを受け付ける。

1. URL（`https://` または `http://` で始まる）
2. セッション内の検索結果識別子（`R3`）
3. セッション内の文書識別子（`D2`）
4. セッション内の結果番号（`3`）。3 と URL 以外の数値は検索結果番号として解釈する

`read` の `--section` は `S3`（単一文書）または `D1:S3`（文書指定付き）の形式で受け付ける。`S0` は文書全体（見出しが無い場合は本文全体）を指す予約識別子で、`--section` を省略した場合も同じく全文を出力する。`0` のような接頭辞の無い数値は検索結果番号として解釈されるため節としては扱えず、`E1005`（`0 is not a section reference`）を返す。`--section` は複数指定でき、指定順に出力する。

セッションが無い状態で識別子を指定した場合は `E5001` を返し、`search` でセッションを開始するよう案内する（参照解決のためにセッションを暗黙に作成しない）。セッションはあるが識別子が存在しない場合は `E1006` を返し、利用可能な識別子の一覧を標準エラーへ出力する。URL を指定した場合はセッションが無くても取得でき、その時点でセッションを作成する。

### 3.3 終了コード

| コード | 意味 |
|---|---|
| `0` | 正常終了 |
| `1` | 引数・入力・設定の誤り（`E1xxx`） |
| `2` | ネットワーク・取得の失敗（`E2xxx`） |
| `3` | 変換・解析の失敗（`E3xxx`） |
| `4` | 検索エンジン・抽出の失敗（`E4xxx`） |
| `5` | セッション・状態・クォータの不整合（`E5xxx`） |

割り込み（`SIGINT`）で終了した場合の終了コードは、OS と端末の既定動作に従う（§2.6）。

---

## 4. 出力チャネルと終了コード

### 4.1 チャネルの分離

- **標準出力**: 結果データ（一覧・アウトライン・節本文・JSON イベント）。パイプやファイルへのリダイレクト対象。
- **標準エラー**: メニュー、進捗、警告、診断。リダイレクトしても結果データを汚さない。

### 4.2 テキスト出力の規則

1 行 1 レコードを基本とし、識別子を行頭に置く。装飾（色・太字）は標準出力が端末のときのみ有効とし、`--no-color` で無効化する。`--color` は端末でない場合に装飾を強制するが、実際には装飾を無効化して `W1001`（`color was requested but stdout is not a terminal; decorations disabled`）を 1 度だけ標準エラーへ出す（`--json` / `--quiet` 時は出さない）。

### 4.3 JSON 出力の規則

`--json` 指定時は 1 行 1 JSON オブジェクト（JSON Lines）を標準出力へ出力する。イベントの種類と必須項目はエージェント連携の仕様で定義する。エラー時も JSON を出力し、`error` イベントにコードとメッセージを含める。

---

## 5. キーボード・入力規則

### 5.1 入力プロンプト

- 数値メニューは 1 文字以上の数字を受け付け、Enter で確定する。前後の空白は無視する。
- プロンプトは画面ごとに固定である: `query:` / `selection (e.g. 1,3 / 1-3 / R1):` / `depth (2-4):` / `section (e.g. D1:S3 / S3):`。
- 検索語の編集（`4) Edit the query`）では空の `query:` を出し、空入力は何もせず現在の画面に留まる（`q` で終了）。
- 1 語コマンドは大文字小文字を区別せずに解釈する: `q` / `quit` / `exit`、`h` / `?` / `help`、`b` / `back`、`n` / `next`、`r` / `refine`、`s` / `history`。画面ごとに有効なコマンドは異なり、検索語入力の画面では `h` 系がヘルプ（`commands: q=quit  n=next page  r=edit query  s=history  h=help`）を、それ以外は不正入力として扱う。`n` / `r` / `s` は結果一覧で、`b` は結果選択・アウトラインで有効である。
- 解釈できない入力は `cannot parse the input (attempt {n} of 3 before returning)` を出し、3 回続くと検索結果メニューへ戻る（検索結果メニュー自身では同じ一覧を再表示する）。

### 5.2 表示幅

- 対話モードの結果一覧は、タイトルを `search.titleChars`（既定 100 文字）、スニペットを `search.snippetChars`（既定 120 文字）で切り詰める。
- 非対話 `search` の結果一覧は、タイトルを切り詰めず、スニペットを 80 表示幅で切り詰める。スニペットが空でない行は `      <スニペット> <URL>` の 2 行目に URL を出す。
- 切り詰めは表示幅（全角文字は 2 桁）で数え、切り詰めた場合は末尾に `…` を付ける。
- 端末幅に応じた折り返しと URL の省略は行わない（端末幅は `COLUMNS` から取るが、出力の組み立てには使っていない）。
