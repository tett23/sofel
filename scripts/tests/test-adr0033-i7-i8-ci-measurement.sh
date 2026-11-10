#!/bin/sh
# ADR 0033 I7-I8: aggregate descendants only and preserve failures.
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sofel-measure.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
mkdir "$tmp/bin"
export MEASURE_TEST_PID=$tmp/pid
cat >"$tmp/command" <<'EOF'
#!/bin/sh
echo $$ >"$MEASURE_TEST_PID"
echo command-output
sleep 0.4
exit "$1"
EOF
cat >"$tmp/bin/ps" <<'EOF'
#!/bin/sh
while [ ! -s "$MEASURE_TEST_PID" ]; do sleep 0.01; done
pid=$(cat "$MEASURE_TEST_PID")
printf '%s 1 100\n900001 %s 200\n900002 900001 300\n900003 1 99999\n' "$pid" "$pid"
EOF
chmod +x "$tmp/bin/ps"
for result in 0 17; do
  rm -f "$MEASURE_TEST_PID"
  got=0
  PATH="$tmp/bin:$PATH" sh "$root/scripts/measure-process-tree.sh" sh "$tmp/command" "$result" \
    >"$tmp/stdout" 2>"$tmp/stderr" || got=$?
  [ "$got" = "$result" ] || { echo 'FAIL: child status changed' >&2; exit 1; }
  grep -qx command-output "$tmp/stdout"
  grep -q 'peak_tree_rss_kib=600 ' "$tmp/stderr"
done
echo 'ok: ADR 0033 I7-I8 CI process-tree measurement'
