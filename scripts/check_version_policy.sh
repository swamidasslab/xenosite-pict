#!/usr/bin/env bash
# Enforce xpict version policy:
#   - major.minor lockstep across JS / Python / Rust public packages
#   - patch may differ per language
#   - major/minor cuts are all-or-none via release/v* (not js/v* / py/v* / …)
#
# Usage:
#   scripts/check_version_policy.sh
#       Assert every in-tree package shares the same X.Y.
#   scripts/check_version_policy.sh 0.1.7
#       Also assert that version's X.Y matches the lockstep line.
#   scripts/check_version_policy.sh --patch-only 0.1.7
#       As above, and reject X.Y.0 (language tags cannot cut a minor/major).
#   scripts/check_version_policy.sh --product 0.2.0
#       For release/v* all-or-none publishes (X.Y must match lockstep).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

MODE=default
VER_ARG=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --patch-only) MODE=patch-only; shift ;;
    --product) MODE=product; shift ;;
    -*)
      echo "error: unknown flag $1" >&2
      exit 1
      ;;
    *)
      VER_ARG="$1"
      shift
      ;;
  esac
done

major_minor() {
  local v="$1"
  if [[ ! "$v" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)([.-].*)?$ ]]; then
    echo "error: not semver X.Y.Z: $v" >&2
    exit 1
  fi
  echo "${BASH_REMATCH[1]}.${BASH_REMATCH[2]}"
}

patch_num() {
  local v="$1"
  if [[ ! "$v" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)([.-].*)?$ ]]; then
    echo "error: not semver X.Y.Z: $v" >&2
    exit 1
  fi
  echo "${BASH_REMATCH[3]}"
}

js_ver="$(python3 -c "import json; print(json.load(open('js/package.json'))['version'])")"
py_ver="$(python3 -c "
import re
from pathlib import Path
m = re.search(r'(?m)^version = \"([^\"]+)\"', Path('pyproject.toml').read_text())
assert m, 'pyproject version missing'
print(m.group(1))
")"
core_ver="$(python3 -c "
import re
from pathlib import Path
m = re.search(r'(?m)^version = \"([^\"]+)\"', Path('crates/xpict-core/Cargo.toml').read_text())
assert m
print(m.group(1))
")"
rust_ver="$(python3 -c "
import re
from pathlib import Path
m = re.search(r'(?m)^version = \"([^\"]+)\"', Path('crates/xpict/Cargo.toml').read_text())
assert m
print(m.group(1))
")"

echo "Package versions (patch may differ):"
echo "  js:         $js_ver"
echo "  python:     $py_ver"
echo "  xpict-core: $core_ver"
echo "  xpict:      $rust_ver"

line="$(major_minor "$js_ver")"
for pair in "python:$py_ver" "xpict-core:$core_ver" "xpict:$rust_ver"; do
  name="${pair%%:*}"
  v="${pair#*:}"
  mm="$(major_minor "$v")"
  if [[ "$mm" != "$line" ]]; then
    echo "error: major.minor mismatch — js is $line, $name has $mm (full $v)" >&2
    echo "Bump all languages to the same X.Y before tagging (patches may stay independent)." >&2
    exit 1
  fi
done

echo "Lockstep major.minor: $line"

if [[ -z "$VER_ARG" ]]; then
  exit 0
fi

tag_mm="$(major_minor "$VER_ARG")"
if [[ "$tag_mm" != "$line" ]]; then
  echo "error: tag/version $VER_ARG has major.minor $tag_mm, repo lockstep is $line" >&2
  echo "For a new minor/major: bump every package's X.Y in-tree, then tag release/vX.Y.0" >&2
  exit 1
fi
echo "Tag/version $VER_ARG matches lockstep $line"

if [[ "$MODE" == "patch-only" ]]; then
  p="$(patch_num "$VER_ARG")"
  if [[ "$p" == "0" ]]; then
    echo "error: language tags cannot publish X.Y.0 ($VER_ARG)" >&2
    echo "Major/minor cuts are all-or-none: tag release/v${tag_mm}.0 instead." >&2
    echo "Language tags (js/v*, py/v*, rust/v*, rust-core/v*) are for patches only (Z>=1)." >&2
    exit 1
  fi
  echo "Patch-only OK (Z=$p)"
fi

if [[ "$MODE" == "product" ]]; then
  echo "Product release OK (all-or-none at $VER_ARG)"
fi
