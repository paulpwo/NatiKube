#!/usr/bin/env bash
# F0 measurements: binary size, time to first frame, idle RSS and pod-list
# parity with kubectl. Read-only against the cluster.
# Usage: scripts/measure.sh [context] [fake_rows]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$ROOT/target/release/natikube"
CONTEXT="${1:-dev-cluster}"
FAKE_ROWS="${2:-10000}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

with_alarm() { perl -e 'alarm shift; exec @ARGV' "$@"; }

echo "== binary"
file "$BIN"
ls -l "$BIN" | awk '{printf "size_bytes=%s size_mb=%.1f\n", $5, $5/1048576}'

echo "== first frame, --fake-rows $FAKE_ROWS"
for i in 1 2 3; do
  with_alarm 60 "$BIN" --fake-rows "$FAKE_ROWS" --exit-after-first-frame 2>&1 | grep -E '^first_' | paste -sd' ' -
done

echo "== first frame, --context $CONTEXT"
for i in 1 2 3; do
  with_alarm 60 "$BIN" --context "$CONTEXT" --exit-after-first-frame 2>&1 | grep -E '^first_' | paste -sd' ' -
done

idle_rss() {
  "$BIN" "$@" >/dev/null 2>&1 &
  local pid=$!
  sleep 10
  local rss_kb
  rss_kb="$(ps -o rss= -p "$pid" | tr -d ' ')"
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  awk -v kb="$rss_kb" 'BEGIN { printf "rss_kb=%s rss_mb=%.1f\n", kb, kb/1024 }'
}

echo "== idle RSS after 10 s, --context $CONTEXT"
idle_rss --context "$CONTEXT"
echo "== idle RSS after 10 s, --fake-rows $FAKE_ROWS"
idle_rss --fake-rows "$FAKE_ROWS"

echo "== pod list parity with kubectl"
with_alarm 60 "$BIN" --context "$CONTEXT" --exit-after-first-frame --dump-rows 2>"$TMP/app.err" || true
grep '^row=' "$TMP/app.err" | sed 's/^row=//' | sort > "$TMP/app.txt"
kubectl --context "$CONTEXT" get pods -A --no-headers -o custom-columns=NS:.metadata.namespace,NAME:.metadata.name \
  | awk '{print $1 "/" $2}' | sort > "$TMP/kubectl.txt"
echo "app_rows=$(wc -l < "$TMP/app.txt" | tr -d ' ') kubectl_rows=$(wc -l < "$TMP/kubectl.txt" | tr -d ' ')"
if diff -u "$TMP/kubectl.txt" "$TMP/app.txt"; then echo "parity=match"; else echo "parity=DIFF"; fi
