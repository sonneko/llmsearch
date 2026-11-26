# M13 dogfooding 報告（実行結果ベース）

- 対象: `develop` `5c88300`（M12 マージ後）をビルドした `llmsearch 0.1.2`
- 作業ブランチ: `arena/0152023f-llmsearch`（HEAD `c082849` = `origin/develop` と同一内容。**M13 ではコードを変更していない**）
- 制約: マージ・PR 作成は行わない（本報告と Issue 化まで）
- 環境: 外部ネットワークが不通（`curl https://example.com` も SSL で失敗し、`E2001`）ため、検証は `127.0.0.1:8899` のローカル HTTP サーバ（stub サイト）と、自作のエンジン定義 `/tmp/m13/eng/*.json` で行った
- 発行した Issue: #37〜#44（8 件）

---

## 0. 検証対象のビルド

```console
$ . /home/user/build/env.sh && cargo build --release
$ ./target/release/llmsearch --version
llmsearch 0.1.2
$ git log --oneline -1
c082849 fix(quota): reject unknown --scope and reset the session counters
$ git rev-parse HEAD && git rev-parse origin/develop
c0828496ceddcb3b3a52c07643754356670d9e9f
5c8830039bd272d5e1731a6616c7ed4301fa78bb
```

`git diff origin/develop --stat` は空（M13 でコードに触れていない）。

### stub の構成（再現用）

- stub サイト（Python）: `/html/?q=&s=<offset>` で 10 件ずつ 20 件を `host{i}.example` の別ホストで返す。`/same` は 10 件すべて `same.example`。`/doclist` は 20 件を `127.0.0.1:8899/doc?n=i` で返す。`/doc` は h1 + h2×4 + h3×1 の本文。
- エンジン定義: `paging: {style: offset, offsetParam: "s", pageSize: 10, offsetStep: 10}`、`limits.maxPageSize: 10`、`minIntervalMs: 0`。
- 起動確認:

```console
$ ./target/release/llmsearch --engines-dir /tmp/m13/eng engines validate
[stub]
[  ok] no issues
```

---

## 1. 基本経路（search / next / refine / outline / read）の実測

### 1.1 `search`（テキスト）

```console
$ D=$(mktemp -d)
$ ./target/release/llmsearch --state-dir $D --engines-dir /tmp/m13/eng --engine stub search rust
Q: rust    engine: stub    showing: 1-7
[R1] Result 1: Rust topic 1
      Snippet 1 about ownership and borrowing in Rust, which explains memory safety w… <https://host1.example/article/1>
...
[R7] Result 7: Rust topic 7
(filters: all 10 shown)
(continue with `next`, or choose 3) Show results from the 8th onward)
```

スニペットは 120 字で `…` 切り詰め。タイトル・URL・件数は仕様どおり。

### 1.2 `search --json`

```console
$ ./target/release/llmsearch ... --json search rust | head -1 | jq -c '{page,hasMore,filtered,session}'
{"page":1,"hasMore":true,"filtered":{"kept":10,"removed":0,"reasons":{},"disabled":false,"truncatedByLimit":3},"session":"20261124-062907-9011"}
$ ./target/release/llmsearch ... --json search rust | tail -1
{"command":"search","summary":"7 results","v":1,"event":"done","session":"20261124-062907-9011","at":"2026-11-24T06:29:07Z"}
```

`at` は `Z` 形式、`done` に `session` / `at` あり。`quota.metrics` は 9 項目。

### 1.3 `next`（不具合 #37 / #38 の実測は各 Issue 参照）

```console
$ S=$(ls $D/sessions | sed 's/.json//')
$ ./target/release/llmsearch ... --session $S next | grep -o 'Result [0-9]*' | head -3 | tr '\n' ' '
Result 11 Result 12 Result 13
```

- `--limit 3` でも既定 7 でも、2 ページ目は 11 件目から（8..10 が飛ぶ）。`--limit 10` のときだけ連続する。
- 画面ラベルは `[R1]`〜 に戻るが、セッションの ID は 1..14。

### 1.4 `outline` / `read`

```console
$ ./target/release/llmsearch ... --session $S outline R1
[D1] Rust Ownership Explained   http://127.0.0.1:8899/doc?n=1
  S1    h2  Introduction                              (324 chars)
  S2    h2  Moves                                     (282 chars) ↓
    S3    h3  Clone                                     (127 chars)
  S4    h2  Borrowing                                 (91 chars)
  S5    h2  Lifetimes                                 (49 chars)

$ ./target/release/llmsearch ... --session $S read R1 --section S2
--- D1:S2 | Moves | h2 | 307 chars ---
[untrusted] http://127.0.0.1:8899/doc?n=1
...
--- end ---
(output 307 chars / budget 8000 chars)
```

`--children` の既定は含む、`--depth 2` で S3 が消えることも確認。`read --section S9` は `E1006`（`available: S1..S5`）、`--section 0` は `E1005`（ただし hint が結果選択の書式 → #41）。

### 1.5 `refine`

```console
$ ./target/release/llmsearch ... --session $S refine "rust ownership"
warning: W4001 changed the query from 'rust' to 'rust ownership'
[R1] Result 1: Rust topic 1
...
```

### 1.6 対話モード（`script` で疑似 tty）

```console
$ printf 'rust\n3\nq\n' | timeout 60 script -qec "./target/release/llmsearch --state-dir $D ... --engine stub" /dev/null
...
  1) Quit
  2) Select results (multiple allowed)
  3) Show results from the 8th onward
  4) Edit the query
> warning: W2001 waited 999 ms before sending to 127.0.0.1 (rate limit)
added 7 results (page 2); searches 2/50
Q: rust    engine: stub
[R1] Result 1: Rust topic 1
...
[R14] Result 17: Rust topic 17
```

メニューの再描画、`3` での追加取得、`select` での `W2001`／`E2001`（stub ホストは外部なので取得不可）を確認。

---

## 2. `--help` とメッセージの日本語残存（0 件）

`--help` を全 36 系統（`search` / `next` / `refine` / `outline` / `read` / `quota` と 3 サブ / `engines` と 4 サブ / `network` と 3 サブ / `filters` と 3 サブ / `config` と 5 サブ / `history` と 4 サブ / `doctor` / `completion` / なし）について取得し、日本語（ひらがな・カタカナ・漢字・全角記号）を機械走査した。

```console
$ python3 - <<'PY'
import subprocess, re
B="./target/release/llmsearch"
cmds=["","search","next","refine","outline","read","quota","quota reset", ... ,"completion"]
jp=re.compile(r'[\u3040-\u30ff\u4e00-\u9fff\uff00-\uffef]')
hits=[]
for c in cmds:
    out=subprocess.run([B]+c.split()+["--help"],capture_output=True,text=True)
    for line in (out.stdout+out.stderr).splitlines():
        if jp.search(line): hits.append((c,line))
print("help lines with Japanese:",len(hits))
PY
help lines with Japanese: 0
```

エラー・警告の代表例（14 パターン）でも 0 件。

```console
$ ./target/release/llmsearch ... search x --engine nope
error: E4002 search engine is unavailable
  detail: engine 'nope' was not found
  hint: run `engines list`
$ ./target/release/llmsearch ... quota reset --scope bogus --yes
error: E1004 invalid configuration value
  detail: --scope must be session, day, window, total, or all (got bogus)
  hint: run `config validate` / `engines validate`
$ ./target/release/llmsearch ... read R1
error: E5001 session not found or expired
  detail: no session, so the identifier cannot be resolved
  hint: run `search` before naming a target
```

ソースの表示文字列も走査（`//` コメントを除外）:

```console
$ python3 - <<'PY'   # src/**/*.rs の非コメント行に日本語が残っていないか
... 0 行
PY
```

残っているのは次の 2 種類だけで、いずれも**正規のデータ**である。

1. `src/ui/render.rs:337` `neutralize_markers()` の `［untrusted］` / `— end —`（仕様 `context-assembly.md` §5.2 の偽装対策。`// m10:allow-japanese` 付き）
2. `assets/filters/default.json` の `title_regex`（「ログイン」「ページが見つかりません」等）。これは日本語サイトの結果を落とすためのパターンで、表示文言ではない。

```console
$ ./target/release/llmsearch filters show default | grep -n regex
24:        "url_regex": "^https?://[^/]+/(login|signin|register)"
27:        "title_regex": "^(ログイン|サインイン|Sign in|Log in)(\\b|$)"
30:        "titleRegex": "404 Not Found|ページが見つかりません"
```

**結論: CLI の表示文言に日本語は残っていない（Issue 化対象 0 件）。** ただし同ファイルのキー表記が `title_regex` と `titleRegex` で混在しているため、これを #42 に含めた。

---

## 3. `quota reset --scope` の実挙動

`counters.json` を各実行の前後で読み出して比較した。

```console
[before]
   session {'searches': 1, ...}  day {'searches': 2, ...}  total {'searches': 2, ...}
$ llmsearch ... quota reset --scope bogus --yes
error: E1004 invalid configuration value
  detail: --scope must be session, day, window, total, or all (got bogus)
[after]
   session {'searches': 1, ...}  day {'searches': 2, ...}  total {'searches': 2, ...}   ← 不変

$ llmsearch ... quota reset --scope session --yes
Reset the quota (scope=session)
   session {'searches': 0, ...}  day {'searches': 2, ...}  total {'searches': 2, ...}   ← セッションのみ 0

$ llmsearch ... quota reset --scope total --yes
   session {'searches': 0, ...}  day {'searches': 2, ...}  total {'searches': 0, ...}   ← total のみ 0

$ llmsearch ... quota reset --scope all --yes
   session 0 / day 0 / total 0
```

- 未知語は `E1004` で停止し、**カウンタは一切変わらない**（M12 の修正が効いている）。
- `--scope session` は `Counters.session` のみを 0 にし、日次・累積は維持する。
- 終了コードも確認: `quota reset --scope bogus --yes; echo $?` → `1`。

`quota show` は `session|day|window60m` × 3 指標（searches / results / chars 等）の 9 行を出し、`window60m` のリセット時刻を併記する。

```console
$ llmsearch ... quota show | head -3
session.searches                1 / 50
day.searches                    1 / 500      2026-11-25T00:00 local
session.fetches                 0 / 100
```

ただし `--session` を省くと**新規セッション（消費 0）の値**を表示してしまう（#39 / #43）。

---

## 4. 設定探索（`--state-dir` の不具合）

```console
$ D=$(mktemp -d); echo '{"search":{"limit":2}}' > $D/settings.json
$ llmsearch --state-dir $D config path | grep settings
[candidate] settings   /home/user/.config/llmsearch/settings.json
[candidate] settings   /home/user/.local/state/llmsearch/settings.json
$ llmsearch --state-dir $D config explain search.limit
search.limit = 7 (default)
$ LLMSEARCH_STATE_DIR=$D llmsearch config explain search.limit
search.limit = 2 (file: /tmp/tmp.N94hCFfKDC/settings.json)
```

仕様 `configuration.md` §2.3 の探索順 3 番目（`<state.dir>/settings.json`）が `--state-dir` では効かない（#40）。

```console
$ llmsearch config path      # 候補一覧
[used] engines    /tmp/m13/eng        # --engines-dir
[used] counters   $D/counters.json
[used] sessions   $D/sessions
```

`engines list`（brave/api・duckduckgo/default・searxng/json・stub/default）、`network list`（default・self-hosted）、`filters list`（default）も実行し、いずれも期待どおり列挙された。

---

## 5. `--json` の stdout / stderr 分離

```console
$ llmsearch ... --json --engine nope search x 1>out.json 2>err.txt; echo "exit=$?"
exit=1
$ cat out.json
{"v":1,"event":"error","code":"E4002","message":"search engine is unavailable", ...}
$ cat err.txt
error: E4002 search engine is unavailable
  detail: engine 'nope' was not found
  hint: run `engines list`
```

stdout は JSON Lines のみ、stderr は人間向けの文面のみ。分離は保たれている。

---

## 6. Issue 化した内容（#37〜#44）

| Issue | 内容 | 主な根拠 |
|---|---|---|
| #37 | `next` / `--page 2` の表示ラベルがセッション ID と一致せず、画面の `R1` が別の文書を指す | 画面 `[R1] Result 11` に対し `sessions/*.json` の ID は 8..14。`read R1` が `host1.example/article/1` を取得 |
| #38 | ページ位置が `--limit` で飛ぶ（8..10 欠落）／ホスト上限の除外が `filtered` に出ない／`--limit`・`--page` が未検証 | `--limit 3` → `next` が Result 11 から。`--engine same` で 3 件表示・`removed: 0`。`--limit 31` / `--page 0` が素通り |
| #39 | `quota show` が架空のセッションを表示、未知の `--session` が無言で作成、`--session` 省略時に既存セッションを拾わない | `session.searches 0/50` 表示（実測 1）。`--session nonexist search` で stderr 空。`read R1` が `E5001` |
| #40 | `--state-dir` が `<state.dir>/settings.json` を読まない／`doctor` が既定環境で exit 1 | 上記 §4。`[fail] engine brave@api`（キー未設定）で exit 1、`summary 1 failures (E1004)`、`rustc (unknown)` |
| #41 | エラーの detail / hint が原因を指さない、`next` が `lastEngine` を継承しない | `network show nope` の detail が `nope` のみ（仕様は候補列挙）、`--section 0` の hint が結果選択の書式、`next` が既定 duckduckgo へ |
| #42 | フッタの固定文言（`all 10 shown`、括弧欠け、常に「8 件目以降」）／history に `next` が残らない／`history list` の `engine` が null／フィルタ定義のキー混在 | `--limit 3` でも `(filters: all 10 shown)`、`truncatedByLimit: 7`。`history.jsonl` は search 2 件のみ |
| #43 | `engines test`・`quota show` で `sessionKey` が入れ替わりセッション消費量が 0 に戻る | `engines test` 後に `sessionKey` が新 ID へ、`day.searches` は +1、`session.searches` は 1 のまま→表示は 0 |
| #44 | 小さな表示・文言の不備（`quota check` の要求量、`--color` の `W1001`、`--help` の `<TARGET>`、`--limit` の範囲、`history export` の見出し） | `quota check --amount 51` が `session.searches = 0/50`、`--json` では `W1001` が出ない、`<TARGET>` の説明が空 |

---

## 7. 検証で確認できた「正しく動いている」点

- `quota reset --scope` の 5 値（`bogus` / `session` / `day` / `window` / `total` / `all`）の挙動と `E1004`。
- `--json` の stdout / stderr 分離、`at` の `Z` 形式、`done` の `session` / `at`。
- `outline` / `read` の信頼境界（`[untrusted]` / `--- end ---`）、`--section` の各形式と `E1005` / `E1006`。
- 全 `--help` とメッセージの英語化（日本語 0 件）。
- `--no-filter` の `(filters: disabled)`、`--refresh` / `--no-cache` の受け付け。
- 対話メニューの再描画と `W2001` / `E2001` の扱い。

## 8. 検証できなかった範囲（stub の限界）

- 実サイトのリダイレクト追従、`robots.txt` 拒否（`E2003` / `E2006`）、`Retry-After`（`E4003`）。
- `brave`（API キーが無い）、`searxng` の実 API。
- TLS / プロキシ / システム CA の経路（外部不通のため）。

## 9. 次のマイルストーンへの引き継ぎ

M13 では**修正を行っていない**。#37〜#44 は次のマイルストーンで、まず #37 / #38 / #43（同じ「ページ位置とセッション ID の扱い」）をまとめて直すのが影響範囲として小さい。文言系（#41 / #42 / #44）は独立して直せる。
