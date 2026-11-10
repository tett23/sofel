#!/bin/sh
# Print the public functions of the library crates, one path per line,
# sorted (ADR 0015): free functions, inherent methods of public types,
# and the methods of public traits, at the paths users write (through
# `pub use`). Read from rustdoc's JSON output.
#
# Usage: scripts/public-api.sh [WORKSPACE]
#   WORKSPACE  default: the repository root
#
# The library crates are the workspace members except the tools, the C
# ABI and the tests (sofel-cli, sofel-capi, sofel-conformance). The JSON
# output is unstable in rustdoc; RUSTC_BOOTSTRAP lets the pinned stable
# toolchain (rust-toolchain.toml) write it, so its format changes only
# when the toolchain does. Needs cargo and jq.

set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
ws=${1:-$root}
not_libraries='sofel-cli sofel-capi sofel-conformance'

meta=$(cargo metadata --format-version 1 --no-deps --manifest-path "$ws/Cargo.toml")
doc=$(echo "$meta" | jq -r .target_directory)/doc

for crate in $(echo "$meta" | jq -r '.packages[].name' | sort); do
  case " $not_libraries " in *" $crate "*) continue ;; esac
  RUSTC_BOOTSTRAP=1 cargo rustdoc --quiet --manifest-path "$ws/Cargo.toml" -p "$crate" --lib \
    -- -Z unstable-options --output-format json
  jq -r '
    . as $d
    | def item($p; $id; $alias):
        ($d.index[$id | tostring]) as $it
        | if $it == null or $it.visibility != "public" then empty
          else ($p + ($alias // $it.name)) as $q
          | ($it.inner | keys[0]) as $k
          | if $k == "module" then ($it.inner.module.items[] as $c | item($q + "::"; $c; null))
            elif $k == "use" then
              ($it.inner.use) as $u
              | if $u.id == null then empty
                elif $u.is_glob then
                  ($d.index[$u.id | tostring].inner.module.items[]? as $c | item($p; $c; null))
                else item($p; $u.id; $u.name) end
            elif $k == "function" then $q
            elif $k == "struct" or $k == "enum" then
              ($it.inner[$k].impls[] as $i
               | $d.index[$i | tostring].inner.impl
               | select(.trait == null and .is_synthetic == false)
               | .items[] as $m
               | $d.index[$m | tostring]
               | select(.visibility == "public" and .inner.function != null)
               | $q + "::" + .name)
            elif $k == "trait" then
              ($it.inner.trait.items[] as $m | $d.index[$m | tostring]
               | select(.inner.function != null) | $q + "::" + .name)
            else empty end
          end;
      item(""; $d.root; null)
  ' "$doc/$(echo "$crate" | tr - _).json"
done | sort -u
