#!/bin/bash
# M19 driver: <binary> <label> — generates history via loopback, then runs Issue #62 evidence.
set -uo pipefail
BIN="$1"
LABEL="$2"
RUN="/tmp/m19/runs/$LABEL"
PORT="$(cat /tmp/m19/port)"
rm -rf "$RUN"; mkdir -p "$RUN/state/engines"
cat > "$RUN/state/engines/loop1.json" <<EOF
{"schemaVersion":1,"id":"loop1","name":"Loop1","request":{"method":"GET","url":"http://127.0.0.1:$PORT/search","query":{"q":"{{query}}"}},"response":{"kind":"html","items":{"selector":"li"},"fields":{"title":{"selector":"a"},"url":{"selector":"a","attr":"href","transform":["absolutize"]}}},"limits":{"maxResults":10,"maxPageSize":10}}
EOF
SD="$RUN/state"
ENG=(--engines-dir "$RUN/state/engines")
DOC1="http://127.0.0.1:$PORT/doc?n=1&token=TOKVAL4821&keep=normal1"

echo "############ generate history (loopback) ############"
# search starts a session; capture its id so refine/read can reuse it (cross-process).
S1="$("$BIN" --state-dir "$SD" "${ENG[@]}" search "climate adaptation" --engine loop1 2>&1 >/dev/null | grep -oE '20[0-9]{6}-[0-9]{6}-[0-9a-f]{4}' | head -1)"
echo "(session S1=$S1)"
"$BIN" --state-dir "$SD" "${ENG[@]}" --session "$S1" outline "$DOC1" >/dev/null 2>&1
"$BIN" --state-dir "$SD" "${ENG[@]}" --session "$S1" read D1:S2 >/dev/null 2>&1
"$BIN" --state-dir "$SD" "${ENG[@]}" --session "$S1" refine "adaptation policy" >/dev/null 2>&1
"$BIN" --state-dir "$SD" "${ENG[@]}" search "coastal defenses" --engine loop1 >/dev/null 2>&1

echo "#### 1) history list (all kinds) ####"
"$BIN" --state-dir "$SD" history list 2>&1

echo
echo "#### 2) refine summary row  [BUG: '0 results (0 filtered out)'] ####"
"$BIN" --state-dir "$SD" history --type refine list 2>&1

echo
echo "#### 3) export --format markdown (non --json) ####"
"$BIN" --state-dir "$SD" history export --format markdown 2>&1

echo
echo "#### 4) export honors --limit and does NOT silently cap at 200 ####"
echo "-- parent form: history --limit 3 export --format jsonl (expect 3 entries after fix) --"
"$BIN" --state-dir "$SD" history --limit 3 export --format jsonl 2>&1 | wc -l
echo "  (parent-form rc=${PIPESTATUS[0]})"
echo "-- export with NO --limit should emit ALL entries (count below) --"
"$BIN" --state-dir "$SD" history export --format jsonl 2>&1 | wc -l

echo
echo "#### 4b) NO 200-cap proof: append 210 fabricated entries, export must return all ####"
cp -r "$SD" "$RUN/st_big"
HJL="$RUN/st_big/history/history.jsonl"
for i in $(seq 1 210); do
  printf '{"id":"HX%03d","at":"2026-06-%02dT00:00:00Z","session":"sx","type":"search","payload":{"query":"bulk %d","results":0,"filtered":0}}\n' "$i" "$((10 + i % 20))" "$i" >> "$HJL"
done
echo "entries on disk after append:"; wc -l < "$HJL"
echo "export --format jsonl row count (expect = disk count, i.e. > 200):"
"$BIN" --state-dir "$RUN/st_big" history export --format jsonl 2>&1 | wc -l

echo
echo "#### 5) clear --before 2000 [subcommand form]  [BUG: clap error before fix] ####"
cp -r "$SD" "$RUN/st_b2000"
"$BIN" --state-dir "$RUN/st_b2000" history clear --before 2000-01-01T00:00:00Z --yes 2>&1
echo "  (rc=$?)"
echo "remaining entries after --before 2000 (expect the 2026 entries kept):"
"$BIN" --state-dir "$RUN/st_b2000" history list 2>&1 | wc -l
"$BIN" --state-dir "$RUN/st_b2000" history list 2>&1 | head -2

echo
echo "#### 6) clear --before 2027 [subcommand form]  [should remove ALL, report real count] ####"
cp -r "$SD" "$RUN/st_b2027"
"$BIN" --state-dir "$RUN/st_b2027" history clear --before 2027-01-01T00:00:00Z --yes 2>&1
echo "  (rc=$?)"
echo "remaining entries after --before 2027 (expect 0 / No history):"
"$BIN" --state-dir "$RUN/st_b2027" history list 2>&1

echo
echo "#### 7) clear via PARENT --before 2000  [BUG: discards --before -> wipes ALL] ####"
cp -r "$SD" "$RUN/st_parent2000"
"$BIN" --state-dir "$RUN/st_parent2000" history --before 2000-01-01T00:00:00Z clear --yes 2>&1
echo "  (rc=$?)"
echo "remaining entries after parent --before 2000 (expect 2026 entries kept):"
"$BIN" --state-dir "$RUN/st_parent2000" history list 2>&1 | wc -l

echo
echo "#### 8) clear WITHOUT --yes  [spec 4.3: stop with E1004] ####"
"$BIN" --state-dir "$SD" history clear 2>&1
echo "  (rc=$?)"

echo
echo "#### 9) state files (layout) ####"
(cd "$SD" && find . -type f | sort)

echo
echo "#### 10) raw jsonl: refine + fetch payloads (as recorded) ####"
echo "-- refine payload (expect {query, previous}) --"
grep '"type":"refine"' "$SD/history/history.jsonl" 2>/dev/null | head -1
echo "-- fetch payload (expect finalUrl/status/bytes/durationMs/extractor present) --"
grep '"type":"fetch"' "$SD/history/history.jsonl" 2>/dev/null | head -1

