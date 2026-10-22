#!/usr/bin/env bash
# Install/activate emsdk in the user cache (CoordGen wasm C++ needs em++).
set -euo pipefail
VER="${EMSDK_VERSION:-6.0.13}"
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/emsdk"

if [[ ! -d "$CACHE/.git" ]]; then
  echo "==> clone emsdk → $CACHE"
  git clone --depth 1 https://github.com/emscripten-core/emsdk.git "$CACHE"
fi

if [[ ! -x "$CACHE/upstream/emscripten/em++" ]]; then
  echo "==> emsdk install $VER"
  "$CACHE/emsdk" install "$VER"
  "$CACHE/emsdk" activate "$VER"
fi

# shellcheck disable=SC1091
source "$CACHE/emsdk_env.sh" >/dev/null
export EMSDK_QUIET=1
if ! command -v em++ >/dev/null 2>&1; then
  echo "ensure_emsdk: em++ not on PATH after activate" >&2
  exit 1
fi
echo "$CACHE"
