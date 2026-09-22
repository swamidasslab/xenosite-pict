#!/usr/bin/env bash
# Enforce xpict version policy:
#   - major.minor lockstep across JS / Python / Rust public packages
#   - patch may differ per language
#
# Usage:
#   scripts/check_version_policy.sh
#       Assert every in-tree package shares the same X.Y.
#   scripts/check_version_policy.sh 0.1.7
#       Also assert that version's X.Y matches the lockstep line (for tag publishes).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

major_minor() {
  local v="$1"
  if [[ ! "$v" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)([.-].*)?$ ]]; then
    echo "error: not semver X.Y.Z: $v" >&2
    exit 1
  fi
  echo "${BASH_REMATCH[1]}.${BASH_REMATCH[2]}"
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

if [[ "${1:-}" != "" ]]; then
  tag_mm="$(major_minor "$1")"
  if [[ "$tag_mm" != "$line" ]]; then
    echo "error: tag/version $1 has major.minor $tag_mm, repo lockstep is $line" >&2
    echo "For a new minor/major, bump every package's X.Y in-tree first, then tag." >&2
    exit 1
  fi
  echo "Tag/version $1 matches lockstep $line"
fi
