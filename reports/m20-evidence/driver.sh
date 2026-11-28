#!/bin/bash
# M20 evidence driver. Usage: driver.sh <binary> <label>
# Runs each issue's loopback evidence and prints BEFORE/AFTER-able sections.
set -uo pipefail
BIN="$1"; LABEL="$2"
PORT=$(cat /tmp/m20/port)
RUN=/tmp/m20/runs/$LABEL
rm -rf "$RUN"; mkdir -p "$RUN/state/engines"
E="$RUN/state/engines"; SD="$RUN/state"
ESTUB="$RUN/stub_engines"; mkdir -p "$ESTUB"   # stub (intentionally invalid) isolated

mk(){ tail -n +$((LAST_MARK+1)) /tmp/m20/requests.log 2>/dev/null | grep -cF "$1" || true; }
mark(){ LAST_MARK=$(wc -l < /tmp/m20/requests.log 2>/dev/null || echo 0); }

# ---- engine definitions ----
cat > "$E/enc.json" <<EOF
{"schemaVersion":1,"id":"enc","name":"Enc","enabled":true,
 "request":{"method":"GET","url":"http://127.0.0.1:$PORT/search","encoding":"euc-jp","query":{"k":"","q":"{{query}}"}},
 "response":{"kind":"html","items":{"selector":"li"},"fields":{"title":{"selector":"a"},"url":{"selector":"a","attr":"href"}}},
 "limits":{"maxResults":10,"maxPageSize":10}}
EOF
cat > "$E/disabled.json" <<EOF
{"schemaVersion":1,"id":"disabled","name":"Disabled","enabled":false,
 "request":{"method":"GET","url":"http://127.0.0.1:$PORT/search","query":{"q":"{{query}}"}},
 "response":{"kind":"html","items":{"selector":"li"},"fields":{"title":{"selector":"a"},"url":{"selector":"a","attr":"href"}}},
 "limits":{"maxResults":10,"maxPageSize":10}}
EOF
cat > "$E/loop1.json" <<EOF
{"schemaVersion":1,"id":"loop1","name":"Loop1","enabled":true,
 "request":{"method":"GET","url":"http://127.0.0.1:$PORT/search","query":{"q":"{{query}}"}},
 "response":{"kind":"html","items":{"selector":"li"},"fields":{"title":{"selector":"a"},"url":{"selector":"a","attr":"href"}}},
 "limits":{"maxResults":10,"maxPageSize":10}}
EOF
cat > "$E/err401.json" <<EOF
{"schemaVersion":1,"id":"err401","name":"Err401","enabled":true,
 "request":{"method":"GET","url":"http://127.0.0.1:$PORT/e401","query":{"q":"{{query}}"}},
 "response":{"kind":"html","items":{"selector":"li"},"fields":{"title":{"selector":"a"},"url":{"selector":"a","attr":"href"}}},
 "limits":{"maxResults":10,"maxPageSize":10}}
EOF
cat > "$E/err403.json" <<EOF
{"schemaVersion":1,"id":"err403","name":"Err403","enabled":true,
 "request":{"method":"GET","url":"http://127.0.0.1:$PORT/e403","query":{"q":"{{query}}"}},
 "response":{"kind":"html","items":{"selector":"li"},"fields":{"title":{"selector":"a"},"url":{"selector":"a","attr":"href"}}},
 "limits":{"maxResults":10,"maxPageSize":10}}
EOF
cat > "$ESTUB/stub.json" <<EOF
{"schemaVersion":1,"id":"stub","name":"Stub","enabled":true,
 "request":{"method":"POST","url":"http://127.0.0.1:$PORT/search","contentType":"application/x-www-form-urlencoded","body":"q={{query}}&src={{unknown_in_body}}"},
 "response":{"kind":"html","items":{"selector":"li"},"fields":{"title":{"selector":"a"},"url":{"selector":"a","attr":"href"}}},
 "limits":{"maxResults":10,"maxPageSize":10}}
EOF
ENG=(--engines-dir "$E")
ENGSTUB=(--engines-dir "$ESTUB")

echo "############ $LABEL ($BIN) ############"
echo "PORT=$PORT"

# ============ #57 ============
echo; echo "===== #57-3 engines validate stub (body {{unknown_in_body}}) ====="
"$BIN" --state-dir "$SD" "${ENGSTUB[@]}" engines validate stub 2>&1
echo "rc=$?"
echo "----- engines test stub --dry-run -----"
"$BIN" --state-dir "$SD" "${ENGSTUB[@]}" engines test stub --query 日本 --dry-run 2>&1 | head -4
echo "rc=${PIPESTATUS[0]}"

echo; echo "===== #57-1 encoding=euc-jp dry-run (expect EUC-JP %C6%FC%CB%DC) ====="
"$BIN" --state-dir "$SD" "${ENG[@]}" engines test enc --query 日本 --dry-run 2>&1 | grep -E "^GET|^POST"

echo; echo "===== #57-2 enabled:false search (expect excluded, rc!=0) ====="
"$BIN" --state-dir "$SD" "${ENG[@]}" search "climate" --engine disabled >/tmp/m20/$LABEL.disabled.out 2>&1
echo "disabled search rc=$?"; sed -n '1,3p' /tmp/m20/$LABEL.disabled.out

# ============ #58 ============
echo; echo "===== #58-1 401 (expect E4003 rc4) ====="
mark; "$BIN" --state-dir "$SD" "${ENG[@]}" search "x" --engine err401 --network self-hosted >/tmp/m20/$LABEL.401.out 2>&1
echo "401 rc=$? /e401 requests=$(mk /e401)"; grep -E "error|E[0-9]{4}" /tmp/m20/$LABEL.401.out | head -2
echo "===== #58-1 403 (expect 1 request, no retry) ====="
mark; "$BIN" --state-dir "$SD" "${ENG[@]}" search "x" --engine err403 --network self-hosted >/tmp/m20/$LABEL.403.out 2>&1
echo "403 rc=$? /e403 requests=$(mk /e403)"; grep -E "error|E[0-9]{4}" /tmp/m20/$LABEL.403.out | head -2

echo; echo "===== #58-2 redaction + relative URL ====="
mark; "$BIN" --state-dir "$SD" "${ENG[@]}" search "climate" --engine loop1 >/tmp/m20/$LABEL.loop.out 2>&1
echo "loop1 search rc=$?"
echo "result urls:"; grep -oE "http://127.0.0.1:[0-9]+/doc[12][^ ]*" /tmp/m20/$LABEL.loop.out

echo; echo "===== #58-5 refine '  hello  ' (expect no W4001 false fire) ====="
S1=$("$BIN" --state-dir "$SD" "${ENG[@]}" search "hello" --engine loop1 2>&1 >/dev/null | grep -oE '20[0-9]{6}-[0-9]{6}-[0-9a-f]{4}' | head -1)
echo "(session S1=$S1)"
"$BIN" --state-dir "$SD" "${ENG[@]}" --session "$S1" refine "  hello  " 2>&1 | grep -E "W4001|warning" | head -3
echo "(refine done; W4001 lines above = false fire)"

# ============ #59 ============
echo; echo "===== #59-1 no-assets (embedded fallback) ====="
NAS="$RUN/noassets"; mkdir -p "$NAS"
echo "-- network list --"; "$BIN" --state-dir "$NAS" network list 2>&1; echo "rc=$?"
echo "-- network show default --"; "$BIN" --state-dir "$NAS" network show default >/tmp/m20/$LABEL.nwshow.out 2>&1; echo "rc=$?"; sed -n '1,2p' /tmp/m20/$LABEL.nwshow.out
echo "-- filters list --"; "$BIN" --state-dir "$NAS" filters list 2>&1; echo "rc=$?"
echo "-- filters show default --"; "$BIN" --state-dir "$NAS" filters show default >/tmp/m20/$LABEL.flshow.out 2>&1; echo "rc=$?"; sed -n '1,2p' /tmp/m20/$LABEL.flshow.out

echo; echo "===== #59-2 invalid network profile (schemaVersion=99, timeout=1, Host=evil) ====="
INV="$RUN/invalid"; mkdir -p "$INV/network"
cat > "$INV/settings.json" <<'EOF'
{"schemaVersion":2,"search":{"limit":10},"cache":{"enabled":true,"ttlHours":24}}
EOF
cat > "$INV/network/default.json" <<EOF
{"schemaVersion":99,"id":"default","timeoutConnectMs":1,"timeoutTotalMs":20000,"maxBytes":5242880,"maxRedirects":5,"redirectPolicy":"follow","userAgent":"llmsearch/0.1.2","headers":{"Host":"evil.example"},"compression":["gzip"],"httpVersion":"auto","minIntervalMs":1000,"maxConcurrency":2,"retry":{"enabled":true,"maxAttempts":3,"backoffMs":500,"backoffFactor":2.0,"maxBackoffMs":8000,"jitterMs":250,"onStatus":[429,502,503,504],"onError":["timeout","connect"],"respectRetryAfter":true},"tls":{"verify":true,"caFile":""},"robots":{"respect":true,"ttlMinutes":60,"userAgentToken":"llmsearch","onError":"allow","maxBytes":524288}}
EOF
mark; "$BIN" --config "$INV/settings.json" --state-dir "$INV/st" "${ENG[@]}" search "x" --engine loop1 >/tmp/m20/$LABEL.inv.out 2>&1
echo "invalid network startup rc=$? /search requests=$(mk /search)"; grep -E "error|E[0-9]{4}" /tmp/m20/$LABEL.inv.out | head -2

# ============ #60 ============
DOC1="http://127.0.0.1:$PORT/doc1"
DOC3="http://127.0.0.1:$PORT/doc3"
echo; echo "===== #60-2 outline doc1 (hashtag must NOT be a heading) ====="
"$BIN" --state-dir "$SD" outline "$DOC1" 2>/dev/null | grep -E "hashtag|S[0-9]" | head -8
echo "----- read doc1 whole (S0) img line (alt must be empty) -----"
"$BIN" --state-dir "$SD" read "$DOC1" --section S0 2>/dev/null | grep -E "img.png|\!\[" | head -2

echo; echo "===== #60-3 ~~~ fence truncation (--max-chars 500) ====="
"$BIN" --state-dir "$SD" read "$DOC1" --section S2 --max-chars 500 2>/dev/null | grep -nE '```|~~~|truncated|end ---' | tail -4

echo; echo "===== #60-4 cross-doc D99:S2 (expect rc!=0) + includeChildren (doc3) ====="
SDC="$RUN/cross"; mkdir -p "$SDC"
S2=$("$BIN" --state-dir "$SDC" outline "$DOC3" 2>&1 >/dev/null | grep -oE '20[0-9]{6}-[0-9]{6}-[0-9a-f]{4}' | head -1)
"$BIN" --state-dir "$SDC" --session "$S2" read D1 --section D99:S2 >/tmp/m20/$LABEL.xdoc.out 2>&1
echo "read D1 --section D99:S2 rc=$?"; grep -E "error|E[0-9]{4}" /tmp/m20/$LABEL.xdoc.out | head -2
# includeChildren: doc3 S1(Top)->S2(Child A),S3(Child B)
DEF_C=$("$BIN" --state-dir "$RUN/incd" read "$DOC3" --section S1 2>/dev/null | grep -c "child a body")
NOC_C=$("$BIN" --state-dir "$RUN/incd" read "$DOC3" --section S1 --no-children 2>/dev/null | grep -c "child a body")
echo "includeChildren default: 'child a body' present=$DEF_C (expect 1); --no-children: present=$NOC_C (expect 0)"
SDI="$RUN/inc"; mkdir -p "$SDI"
echo '{"context":{"includeChildren":false}}' > "$SDI/settings.json"
SET_C=$("$BIN" --state-dir "$SDI" read "$DOC3" --section S1 2>/dev/null | grep -c "child a body")
echo "settings includeChildren=false (no flag): 'child a body' present=$SET_C (1=bug, 0=fixed)"

echo; echo "===== #60-5 --max-chars out of range (expect E1004) ====="
"$BIN" --state-dir "$SD" read "$DOC1" --max-chars 28 >/tmp/m20/$LABEL.maxc.out 2>&1
echo "read --max-chars 28 rc=$?"; grep -E "E[0-9]{4}|error" /tmp/m20/$LABEL.maxc.out | head -2

echo; echo "===== $LABEL DONE ====="
