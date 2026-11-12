#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
exec cargo run --quiet -p sofel-conformance --bin sofel-svg-conformance
