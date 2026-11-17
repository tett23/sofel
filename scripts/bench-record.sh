#!/bin/sh
# Measure natively and on wasm, and write one benchmark record (ADR 0080).
#
# Usage: scripts/bench-record.sh --subject SUBJECT [--filter TEXT] [--out DIR]
#   SUBJECT  lowercase letters and digits joined by hyphens
#   TEXT     passed to both measurements as --filter
#   DIR      where the record goes; default docs/benchmarks
# Writes DIR/<YYYY-MM-DD>-<SUBJECT>-ci-<runner>.md (date in UTC) and prints
# its path. Exits 2 on wrong usage, and with a measurement's status, writing
# nothing, when a measurement fails. SOFEL_BENCH_NATIVE and SOFEL_BENCH_WASM
# replace the measurement commands.

set -u

usage() {
  echo "usage: scripts/bench-record.sh --subject SUBJECT [--filter TEXT] [--out DIR]" >&2
  exit 2
}

subject=
filter=
have_filter=0
out=docs/benchmarks
while [ $# -gt 0 ]; do
  case "$1" in
    --subject) [ $# -ge 2 ] || usage; subject=$2; shift 2 ;;
    --filter) [ $# -ge 2 ] || usage; filter=$2; have_filter=1; shift 2 ;;
    --out) [ $# -ge 2 ] || usage; out=$2; shift 2 ;;
    *) usage ;;
  esac
done
if ! printf '%s\n' "$subject" | grep -Eq '^[a-z0-9]+(-[a-z0-9]+)*$'; then
  echo "bench-record.sh: the subject must match ^[a-z0-9]+(-[a-z0-9]+)*\$" >&2
  usage
fi

native=${SOFEL_BENCH_NATIVE:-cargo run --release --manifest-path bench/Cargo.toml --}
wasm=${SOFEL_BENCH_WASM:-wasmtime run -W exceptions=y --dir target/testdata bench/target/wasm32-wasip1/release/sofel-bench.wasm}

# The first line of a command's output, or 未取得.
first() {
  v=$("$@" 2>/dev/null | head -n 1) || v=
  [ -n "$v" ] && printf '%s' "$v" || printf '未取得'
}

has() {
  command -v "$1" >/dev/null 2>&1
}

os=$(uname -s 2>/dev/null | tr 'A-Z' 'a-z')
[ "$os" = darwin ] && os=macos
arch=$(uname -m 2>/dev/null)
case "$arch" in
  x86_64) arch=x64 ;;
  aarch64 | arm64) arch=arm64 ;;
esac
runner=$os-$arch
day=$(date -u +%Y-%m-%d)

tmp=$(mktemp -d "${TMPDIR:-/tmp}/bench-record.XXXXXX") || exit 1
trap 'rm -rf "$tmp"' EXIT

# Values of the environment, read before measuring.
image=未取得
[ -n "${ImageOS:-}" ] && image="${ImageOS} ${ImageVersion:-}"
osname=未取得
if [ -r /etc/os-release ]; then
  osname=$(sed -n 's/^PRETTY_NAME="\{0,1\}\([^"]*\)"\{0,1\}$/\1/p' /etc/os-release)
  [ -n "$osname" ] || osname=未取得
elif has sw_vers; then
  osname="$(sw_vers -productName 2>/dev/null) $(sw_vers -productVersion 2>/dev/null)"
fi
cpu=未取得
if has lscpu; then
  cpu=$(lscpu 2>/dev/null | sed -n 's/^Model name: *//p' | head -n 1)
  [ -n "$cpu" ] || cpu=未取得
elif has sysctl; then
  cpu=$(first sysctl -n machdep.cpu.brand_string)
fi
cpus=未取得
if has nproc; then
  cpus=$(first nproc)
elif has sysctl; then
  cpus=$(first sysctl -n hw.ncpu)
fi
memory=未取得
if [ -r /proc/meminfo ]; then
  memory=$(sed -n 's/^MemTotal: *//p' /proc/meminfo)
  [ -n "$memory" ] || memory=未取得
elif has sysctl; then
  memory=$(first sysctl -n hw.memsize)
  [ "$memory" = 未取得 ] || memory="$memory bytes"
fi
load=未取得
has uptime && load=$(first uptime)
rust=未取得
has rustc && rust=$(first rustc -V)
compilers=未取得
if has cc || has c++; then
  ccv=未取得
  cxxv=未取得
  has cc && ccv=$(first cc --version)
  has c++ && cxxv=$(first c++ --version)
  compilers="cc: $ccv; c++: $cxxv"
fi
wasmtime=未取得
has wasmtime && wasmtime=$(first wasmtime --version)
wasisdk=未取得
if [ -r bench/wasi-sdk.lock ]; then
  wasisdk=$(grep -o 'wasi-sdk-[0-9][0-9.]*' bench/wasi-sdk.lock | head -n 1)
  [ -n "$wasisdk" ] || wasisdk=未取得
fi
refs=未取得
if [ -r testdata.lock ]; then
  refs=$(awk '$1 == "freetype" || $1 == "harfbuzz" { printf "%s%s %s", sep, $1, $3; sep = ", " }' testdata.lock)
  [ -n "$refs" ] || refs=未取得
fi
commit=未取得
has git && commit=$(first git rev-parse HEAD)
url=未取得
if [ -n "${GITHUB_SERVER_URL:-}" ] && [ -n "${GITHUB_REPOSITORY:-}" ] && [ -n "${GITHUB_RUN_ID:-}" ]; then
  url="$GITHUB_SERVER_URL/$GITHUB_REPOSITORY/actions/runs/$GITHUB_RUN_ID"
fi

measure() {
  # $1: command words, $2: output file.
  if [ "$have_filter" = 1 ]; then
    # shellcheck disable=SC2086 # the command is a list of words
    $1 --filter "$filter" >"$2"
  else
    # shellcheck disable=SC2086
    $1 >"$2"
  fi
}

measure "$native" "$tmp/native.md" || exit $?
measure "$wasm" "$tmp/wasm.md" || exit $?

name=$day-$subject-ci-$runner.md
{
  printf '# %s の計測（CI、%s、%s）\n\n' "$subject" "$runner" "$day"
  printf '[ADR 0080](../adr/0080-ci-benchmark-records.md) のワークフローで取った記録である。\n'
  printf '時間は1回あたりの中央値（±中央絶対偏差）、単位はマイクロ秒で、比が1未満なら sofel が速い。\n'
  [ "$have_filter" = 1 ] && printf '項目は `--filter %s` で絞った。\n' "$filter"
  printf '\n## 環境\n\n'
  printf '| 項目 | 値 |\n| --- | --- |\n'
  printf '| ランナーのイメージと版 | %s |\n' "$image"
  printf '| OS の名前と版 | %s |\n' "$osname"
  printf '| CPU の名前 | %s |\n' "$cpu"
  printf '| 論理 CPU の数 | %s |\n' "$cpus"
  printf '| メモリ | %s |\n' "$memory"
  printf '| 計測の直前の負荷平均 | %s |\n' "$load"
  printf '| Rust の版 | %s |\n' "$rust"
  printf '| C と C++ のコンパイラの版 | %s |\n' "$compilers"
  printf '| wasmtime の版 | %s |\n' "$wasmtime"
  printf '| wasi-sdk の版 | %s |\n' "$wasisdk"
  printf '| FreeType と HarfBuzz の版 | %s |\n' "$refs"
  printf '| sofel のコミット | %s |\n' "$commit"
  printf '| 実行の URL | %s |\n' "$url"
  printf '\n## ネイティブ\n\n'
  cat "$tmp/native.md"
  printf '\n## wasm\n\n'
  cat "$tmp/wasm.md"
} >"$tmp/$name" || exit 1
mkdir -p "$out" && mv "$tmp/$name" "$out/$name" || exit 1
printf '%s\n' "$out/$name"
