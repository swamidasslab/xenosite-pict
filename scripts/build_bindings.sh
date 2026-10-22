#!/usr/bin/env bash
# Build shared depiction bindings for Python and/or JS.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

usage() {
  echo "Usage: $0 [python|wasm|all]" >&2
  exit 1
}

target="${1:-all}"

# Prefer rustup's rustc (Homebrew rustc has no wasm32-unknown-emscripten).
if [[ -d "${HOME:-}/.rustup/toolchains" ]]; then
  export PATH="${HOME}/.cargo/bin:${PATH}"
  if command -v rustup >/dev/null 2>&1; then
    rustc_bin="$(rustup which rustc 2>/dev/null || true)"
    if [[ -n "$rustc_bin" ]]; then
      export PATH="$(dirname "$rustc_bin"):${PATH}"
    fi
  fi
fi

build_python() {
  echo "==> maturin develop (xpict._native)"
  if command -v uv >/dev/null 2>&1; then
    uv run maturin develop --manifest-path crates/xpict-py/Cargo.toml
  else
    maturin develop --manifest-path crates/xpict-py/Cargo.toml
  fi
}

build_wasm() {
  echo "==> emsdk + cargo (wasm32-unknown-emscripten) → js/src/wasm"
  bash "$ROOT/scripts/ensure_emsdk.sh" >/dev/null
  # shellcheck disable=SC1091
  source "${XDG_CACHE_HOME:-$HOME/.cache}/emsdk/emsdk_env.sh"
  export EMSDK_QUIET=1
  rustup target add wasm32-unknown-emscripten >/dev/null
  mkdir -p js/src/wasm
  cargo build -p xpict-wasm --target wasm32-unknown-emscripten --release --bin xpict_wasm
  local out="$ROOT/target/wasm32-unknown-emscripten/release/xpict_wasm"
  if [[ ! -f "$out.js" || ! -f "$out.wasm" ]]; then
    echo "missing $out.js / $out.wasm" >&2
    exit 1
  fi
  cp "$out.js" "$ROOT/js/src/wasm/xpict_core.js"
  cp "$out.wasm" "$ROOT/js/src/wasm/xpict_core.wasm"
  # emcc embeds the cargo binary name; rewrite so the checked-in blob matches.
  perl -i -pe 's/xpict_wasm\.wasm/xpict_core.wasm/g' "$ROOT/js/src/wasm/xpict_core.js"
  rm -f "$ROOT/js/src/wasm/xpict_wasm.wasm"
  rm -f js/src/wasm/.gitignore js/src/wasm/package.json js/src/wasm/README.md \
    js/src/wasm/xpict_core_bg.wasm js/src/wasm/xpict_core_bg.wasm.d.ts
}

case "$target" in
  python) build_python ;;
  wasm) build_wasm ;;
  all)
    build_python
    build_wasm
    ;;
  *) usage ;;
esac

echo "Done ($target)."
