#!/usr/bin/env bash
#
# Builds the release bundle and serves it under /angulito/, mirroring the
# GitHub Pages layout. Used as the Playwright web server.
#
# The build leaves the bundle in target/dx/angulito/release/web/public and it
# is served from a copy, so the bundle stays in place after the run.
set -euo pipefail

port="${1:?usage: serve.sh <port>}"
root="$(git rev-parse --show-toplevel)"
cd "$root"

# The `build` target runs `dx build --release --debug-symbols false`. Dropping
# the debug symbols is required, because wasm-opt aborts with SIGABRT when
# asked to preserve this module's DWARF info, which dx requests by default.
# Neither the tests nor the deployed bundle need debug symbols.
make build >&2

stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
# Teardown terminates the whole process group. An untrapped SIGTERM kills this
# shell outright and skips the EXIT trap, which would leave a copy of the
# bundle behind on every run, so catch it and leave through `exit` instead.
trap 'exit 143' INT TERM
mkdir "$stage/angulito"
cp -r target/dx/angulito/release/web/public/. "$stage/angulito/"

# Deliberately not `exec`: this shell has to outlive the server to clean the
# staging directory up. The server still ends up in the same process group, so
# teardown reaches it either way.
python3 -m http.server "$port" --directory "$stage"
