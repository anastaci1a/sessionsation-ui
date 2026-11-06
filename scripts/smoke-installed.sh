#!/bin/sh
# Post-install smoke test of the INSTALLED sessionsation-ui (never --help alone: it does not
# load the preferences, which is how a stale binary shipped):
#   scripts/smoke-installed.sh BIN [EXPECTED_COMMIT]
# Runs the real binary in a pseudo-terminal against the real config and preferences, from the
# real working directory, for about 3 s (nothing is typed), and fails if it prints "Error:" or
# exits early. A failure means: switch the .runtime link back to the previous release now.
set -eu
bin=${1:?usage: smoke-installed.sh BIN [EXPECTED_COMMIT]}
expected=${2:-}
cd "${SMOKE_CWD:-/mnt/disk1/live/projects/sessionsation-ui}"
ver=$("$bin" --version) || { echo "SMOKE FAIL: --version" >&2; exit 1; }
echo "$ver"
if [ -n "$expected" ]; then
    case "$ver" in
        *"$expected"*) ;;
        *) echo "SMOKE FAIL: embedded commit differs from $expected (stale build)" >&2; exit 1 ;;
    esac
fi
out=$(mktemp)
trap 'rm -f "$out"' EXIT
start=$(date +%s)
set +e
timeout 3 script -qc "$bin" /dev/null </dev/null >"$out" 2>&1
code=$?
set -e
elapsed=$(( $(date +%s) - start ))
if grep -q "Error:" "$out"; then
    echo "SMOKE FAIL: the binary printed an error:" >&2
    grep "Error:" "$out" | head -3 >&2
    exit 1
fi
# 124 = still running when the timeout fired (healthy). Any earlier exit is a failure.
if [ "$code" -ne 124 ] && [ "$elapsed" -lt 3 ]; then
    echo "SMOKE FAIL: exited early with status $code after ${elapsed}s" >&2
    exit 1
fi
echo "SMOKE OK: $bin"
