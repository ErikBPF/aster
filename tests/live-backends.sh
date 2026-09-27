#!/usr/bin/env bash
# Binding for features/live-backends.feature.
#
# These are tooling assertions, so they are checked here instead of through a
# Gherkin runner: every check prints what it verified and any failure exits
# non-zero. Unlike the other scripts they need a reachable cluster, a built
# server binary and two port-forwards, so they are run explicitly and are not
# part of `just ci`:
#
#   just stack-bootstrap        # self-contained minikube stack (default context)
#   tests/live-backends.sh
#
# Against another cluster, set ASTER_LIVE_CONTEXT (and the namespace/port
# overrides below).
set -euo pipefail

cd "$(dirname "$0")/.."

context=${ASTER_LIVE_CONTEXT:-aster}
trino_namespace=${ASTER_LIVE_TRINO_NAMESPACE:-trino}
spark_namespace=${ASTER_LIVE_SPARK_NAMESPACE:-spark-jobs}
gateway_port=${ASTER_LIVE_GATEWAY_PORT:-18092}
spark_port=${ASTER_LIVE_SPARK_PORT:-15092}
server_port=${ASTER_LIVE_SERVER_PORT:-18090}
binary=${ASTER_SERVER_BIN:-target/debug/aster-server}

work=$(mktemp -d)
pids=()
cleanup() {
  for pid in ${pids[@]+"${pids[@]}"}; do
    kill "$pid" 2>/dev/null || true
  done
  rm -rf "$work"
}
trap cleanup EXIT

failures=0
ok() { echo "ok: $1"; }
bad() {
  echo "FAIL: $1" >&2
  failures=$((failures + 1))
}
check() {
  local description="$1"
  shift
  if "$@" >/dev/null 2>&1; then ok "$description"; else bad "$description"; fi
}

wait_for_http() {
  for _ in $(seq 1 60); do
    curl -sf --max-time 3 "$1" >/dev/null 2>&1 && return 0
    sleep 1
  done
  echo "timed out waiting for $1" >&2
  return 1
}

# One query attempt as a signed-in subject; writes the body, prints the status.
query() {
  local subject="$1" roles="$2" body="$3" out="$4"
  curl -s --max-time 180 -o "$out" -w '%{http_code}' \
    -X POST "http://127.0.0.1:${server_port}/api/query" \
    -H 'content-type: application/json' \
    -b "aster_subject=${subject}" -b "aster_roles=${roles}" \
    -d "$body"
}

# A 200 whose body matches the jq filter counts as a pass.
query_check() {
  local description="$1" subject="$2" roles="$3" body="$4" filter="$5"
  local status
  status=$(query "$subject" "$roles" "$body" "$work/query.json")
  if [ "$status" = 200 ] && jq -e "$filter" "$work/query.json" >/dev/null 2>&1; then
    ok "$description"
  else
    bad "$description (status ${status})"
  fi
}

gateway_backends_lab_only() {
  curl -s --max-time 30 "http://127.0.0.1:${gateway_port}/gateway/backend/active" \
    >"$work/backends.json" &&
    jq -e 'length == 1 and .[0].name == "trino-lab" and .[0].routingGroup == "lab"' \
      "$work/backends.json" >/dev/null
}

gateway_answers_absent_group() {
  local status
  status=$(curl -s -o "$work/absent.json" -w '%{http_code}' --max-time 120 \
    -X POST -H 'X-Trino-User: aster' -H 'X-Trino-Routing-Group: absent' \
    --data-binary 'select 1' "http://127.0.0.1:${gateway_port}/v1/statement")
  [ "$status" = 200 ] && jq -e '.nextUri // .id' "$work/absent.json" >/dev/null
}

test -x "$binary" || {
  echo "build the server first (just build) or set ASTER_SERVER_BIN" >&2
  exit 2
}

kubectl --context "$context" get namespace "$trino_namespace" >/dev/null
kubectl --context "$context" get namespace "$spark_namespace" >/dev/null

kubectl --context "$context" -n "$trino_namespace" \
  port-forward svc/trino-gateway "${gateway_port}:8080" >"$work/gateway-forward.log" 2>&1 &
pids+=($!)
kubectl --context "$context" -n "$spark_namespace" \
  port-forward svc/spark-connect-aster "${spark_port}:15002" >"$work/spark-forward.log" 2>&1 &
pids+=($!)

wait_for_http "http://127.0.0.1:${gateway_port}/v1/info"

ASTER_BIND="127.0.0.1:${server_port}" \
ASTER_ENGINES="trino-gw;trino;http://127.0.0.1:${gateway_port};lab,spark-live;spark;http://127.0.0.1:${spark_port}" \
ASTER_CATALOG_BINDINGS="polaris;trino-gw;polaris;unprotected,polaris;spark-live;polaris;unprotected" \
ASTER_GRANTS="alice:trino-gw,alice:spark-live" \
"$binary" >"$work/server.log" 2>&1 &
pids+=($!)

wait_for_http "http://127.0.0.1:${server_port}/healthz"

# --- a cell runs on Trino through the gateway --------------------------------
query_check "trino through the gateway returns the tpch row count" \
  alice editor '{"engine":"trino-gw","sql":"select count(*) as n from tpch.tiny.orders"}' \
  '.columns[0].name == "n" and .rows == [[15000]] and (.truncated | not)'

# --- a cell runs on the Spark cluster ----------------------------------------
query_check "the spark cluster returns three rows" \
  alice editor '{"engine":"spark-live","sql":"select explode(sequence(1, 3)) as n"}' \
  '.columns[0].name == "n" and (.rows | length) == 3'

# --- the routing group reaches the gateway -----------------------------------
check "the gateway lists exactly one active backend in routing group lab" \
  gateway_backends_lab_only

# --- an absent routing group falls back instead of refusing ------------------
check "an absent routing group is answered from the default group" \
  gateway_answers_absent_group

# --- a subject without a grant is refused and the refusal is audited ---------
status=$(query bob editor '{"engine":"trino-gw","sql":"select 1"}' "$work/refused.json")
check "an ungranted subject is refused with 403" test "$status" = 403
check "the refusal names the missing grant" grep -qi grant "$work/refused.json"

# --- both live runs are audited ----------------------------------------------
curl -s --max-time 30 -o "$work/audit.json" \
  -b 'aster_subject=bob' -b 'aster_roles=admin' \
  "http://127.0.0.1:${server_port}/api/audit"
check "the audit trail lists a successful trino run for alice" \
  jq -e 'any(.[]; .subject == "alice" and .engine == "trino-gw" and .ok)' "$work/audit.json"
check "the audit trail lists a successful spark run for alice" \
  jq -e 'any(.[]; .subject == "alice" and .engine == "spark-live" and .ok)' "$work/audit.json"
check "the audit trail lists bob's refusal" \
  jq -e 'any(.[]; .subject == "bob" and .ok == false)' "$work/audit.json"

if [ "$failures" -ne 0 ]; then
  echo "${failures} live check(s) failed; server log:" >&2
  tail -20 "$work/server.log" >&2
  exit 1
fi
echo "live-backends OK"
