#!/usr/bin/env bash
# The editor for a browser, as a static site in one directory: what the
# Pages workflow publishes, and what any static server shows locally.
#
#     apps/dottune/web/build.sh [out]        # default out: target/web
#     python3 -m http.server -d target/web
#
# Needs wasm-bindgen-cli at the version Cargo.lock has for wasm-bindgen, as
# the bindings and the crate must match.
set -euo pipefail
root=$(cd "$(dirname "$0")/../../.." && pwd)
out=${1:-$root/target/web}

want=$(awk '/^name = "wasm-bindgen"$/ { getline; gsub(/version = |"/, ""); print; exit }' "$root/Cargo.lock")
have=$(wasm-bindgen --version 2>/dev/null | awk '{ print $2 }')
if [ "$want" != "$have" ]; then
  echo "wasm-bindgen ${have:-(none)} is on PATH; Cargo.lock wants $want:" >&2
  echo "  cargo install --locked wasm-bindgen-cli --version $want" >&2
  exit 1
fi

cargo build --manifest-path "$root/Cargo.toml" -p dottune --release \
  --target wasm32-unknown-unknown --no-default-features --features web

rm -rf "$out"
mkdir -p "$out"
wasm-bindgen --target web --no-typescript --out-dir "$out" \
  "$root/target/wasm32-unknown-unknown/release/dottune.wasm"
cp "$root/apps/dottune/web/index.html" "$out/"
# Pages runs Jekyll over a site unless told not to.
touch "$out/.nojekyll"
du -sh "$out"
