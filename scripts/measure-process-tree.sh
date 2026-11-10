#!/bin/sh
# Observe the command's aggregate descendant RSS for ADR 0033's CI runs.
# RSS includes shared pages in each process; this is a sampled maximum.
# Usage: sh scripts/measure-process-tree.sh COMMAND [ARG...]
set -u
if [ "$#" -eq 0 ]; then
  echo 'measure-process-tree.sh: a command is required' >&2
  exit 2
fi
"$@" &
command_pid=$!
peak=0
samples=0
status=0
while kill -0 "$command_pid" 2>/dev/null; do
  snapshot=$(ps -axo pid=,ppid=,rss=) || { status=1; break; }
  rss=$(printf '%s\n' "$snapshot" | awk -v root="$command_pid" '
    { parent[$1] = $2; memory[$1] = $3 }
    END {
      included[root] = 1
      do {
        changed = 0
        for (pid in parent)
          if (!included[pid] && included[parent[pid]]) {
            included[pid] = 1
            changed = 1
          }
      } while (changed)
      total = 0
      for (pid in memory) if (included[pid]) total += memory[pid]
      printf "%.0f\n", total
    }') || { status=1; break; }
  if [ "$rss" -gt "$peak" ]; then peak=$rss; fi
  samples=$((samples + 1))
  sleep 0.1
done
wait "$command_pid" || status=$?
printf 'peak_tree_rss_kib=%s samples=%s poll_seconds=0.1 exit_code=%s\n' \
  "$peak" "$samples" "$status" >&2
exit "$status"
