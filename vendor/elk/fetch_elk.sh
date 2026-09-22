#!/usr/bin/env bash
# Packaging hook: download ELK standalone JAR into vendor/elk/ for the future V8/JAR bridge.
# Not required for the current grid/row diagram fallback.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
OUT="${ROOT}/elk.jar"
# Eclipse ELK releases — pin when the bridge lands. Placeholder URL documents the hook.
ELK_URL="${ELK_URL:-https://repo1.maven.org/maven2/org/eclipse/elk/org.eclipse.elk.alg.layered/0.9.3/org.eclipse.elk.alg.layered-0.9.3.jar}"
echo "Fetching ELK artifact into ${OUT}"
curl -fsSL -o "${OUT}" "${ELK_URL}"
echo "Wrote ${OUT} ($(wc -c < "${OUT}") bytes)"
