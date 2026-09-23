# xpict — local / CI / agent entry points
#
#   make help          list targets
#   make agent-install bootstrap deps (Cloud Agents, CONTRIBUTING)
#   make test          run the usual local suite
#
# Heavy logic stays in scripts/; this file is the documented front door.
# GitHub Actions should call these targets after toolchain setup (Rust,
# Node, uv). Prefer `make` on Linux/macOS runners; Windows jobs keep
# direct commands (GNU make is not always present).
#
# Publish to registries is tag-driven — see .github/PUBLISH.md. Local
# targets only dry-run / bump / check policy.

SHELL := /usr/bin/env bash
.SHELLFLAGS := -euo pipefail -c

ROOT := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
OUT ?= $(ROOT)/_site
RUST_COV_MIN ?= 97
VERSION ?=

.DEFAULT_GOAL := help

.PHONY: help
help: ## Show documented targets
	@awk 'BEGIN {FS = ":.*##"; printf "xpict targets:\n\n"} \
		/^[a-zA-Z0-9_-]+:.*?##/ { printf "  %-22s %s\n", $$1, $$2 } \
		/^##@/ { printf "\n%s\n", substr($$0, 5) }' $(MAKEFILE_LIST)
	@printf '\nVariables: OUT=%s  RUST_COV_MIN=%s  VERSION=%s\n' \
		'$(OUT)' '$(RUST_COV_MIN)' '$(or $(VERSION),<required for bump>)'
	@printf 'Publish tags / registries: .github/PUBLISH.md\n'

##@ Setup

.PHONY: sync
sync: ## Python venv: uv sync --group dev --extra rdkit
	cd "$(ROOT)" && uv sync --group dev --extra rdkit

.PHONY: sync-docs
sync-docs: ## Python venv: docs + dev + rdkit (Pages / MkDocs)
	cd "$(ROOT)" && uv sync --group docs --group dev --extra rdkit

.PHONY: js-deps
js-deps: ## npm ci in js/
	cd "$(ROOT)/js" && npm ci

.PHONY: agent-install
agent-install: ## Cloud Agent / fresh clone: sync Python + build native ext
	@$(MAKE) sync
	@$(MAKE) build-python
	@echo "agent-install done (JS: make js-deps && make build-js)"
	@echo "Tip: set Cursor Cloud install to: make agent-install"

##@ Build

.PHONY: build
build: build-bindings ## Python maturin + JS wasm bindings

.PHONY: build-bindings
build-bindings: ## ./scripts/build_bindings.sh all
	cd "$(ROOT)" && ./scripts/build_bindings.sh all

.PHONY: build-python
build-python: ## maturin develop (xpict._native)
	cd "$(ROOT)" && ./scripts/build_bindings.sh python

.PHONY: build-wasm
build-wasm: ## wasm-pack → js/src/wasm
	cd "$(ROOT)" && ./scripts/build_bindings.sh wasm

.PHONY: build-js
build-js: ## npm run build (wasm + tsc + copy); needs js-deps + wasm-pack
	cd "$(ROOT)/js" && npm run build

.PHONY: pages
pages: ## MkDocs + autodoc + demo → OUT (default _site)
	cd "$(ROOT)" && bash scripts/build_pages.sh "$(OUT)"

##@ Test / check

.PHONY: test
test: test-rust test-python test-js ## Core + Python + JS (local full pass)

.PHONY: test-rust
test-rust: ## cargo test -p xpict-core
	cd "$(ROOT)" && cargo test -p xpict-core

.PHONY: test-rust-native
test-rust-native: ## cargo test -p xpict (needs system RDKit)
	cd "$(ROOT)" && cargo test -p xpict

.PHONY: test-python
test-python: ## uv run pytest -q
	cd "$(ROOT)" && uv run pytest -q

.PHONY: test-js
test-js: ## JS check + build + API smoke (runs npm ci)
	@$(MAKE) js-deps
	@$(MAKE) js-ci

.PHONY: js-check
js-check: ## tsc --strict (+ pocs); assumes js-deps already
	cd "$(ROOT)/js" && npm run check && npm run check:pocs

.PHONY: js-test-api
js-test-api: ## JS API smoke scripts; assumes build
	cd "$(ROOT)/js" && npm run test:api

.PHONY: js-ci
js-ci: js-check build-js js-test-api ## CI JS job body (after js-deps + wasm-pack)

.PHONY: check
check: check-pyright check-versions ## pyright + version policy

.PHONY: check-pyright
check-pyright: ## Strict pyright on python/
	cd "$(ROOT)" && uv run pyright

.PHONY: check-versions
check-versions: ## major.minor lockstep (scripts/check_version_policy.sh)
	cd "$(ROOT)" && bash scripts/check_version_policy.sh

.PHONY: cov
cov: ## llvm-cov on xpict-core (fails under RUST_COV_MIN)
	cd "$(ROOT)" && RUST_COV_MIN="$(RUST_COV_MIN)" bash scripts/rust_coverage.sh

.PHONY: cov-lcov
cov-lcov: ## llvm-cov → lcov.info (CI artifact shape)
	cd "$(ROOT)" && cargo llvm-cov -p xpict-core --lcov --output-path lcov.info \
		--fail-under-lines "$(RUST_COV_MIN)"

##@ Schema / version / publish helpers

.PHONY: types
types: ## Regenerate live TS + JSON Schema from xpict-core (schemars/ts-rs)
	cd "$(ROOT)" && bash scripts/generate_live_types.sh

.PHONY: types-check
types-check: ## Fail if live generated TS/schemas drift from Rust
	@$(MAKE) types
	@cd "$(ROOT)" && git diff --exit-code -- \
		js/src/generated \
		schema/edge-plan.schema.json \
		schema/edge-result.schema.json \
		schema/scene.schema.json \
		schema/xpict.schema.json \
		|| { echo "error: live types drift — run make types and commit" >&2; exit 1; }

.PHONY: schema
schema: ## Future PictSpec schema (Pydantic) + live schemas (Rust)
	@$(MAKE) types
	cd "$(ROOT)" && uv run xpict-export-schema

.PHONY: bump
bump: ## Set all package versions: make bump VERSION=0.4.0
	@test -n "$(VERSION)" || { echo "Usage: make bump VERSION=X.Y.Z" >&2; exit 1; }
	cd "$(ROOT)" && python3 scripts/bump_version.py "$(VERSION)"
	cd "$(ROOT)" && bash scripts/check_version_policy.sh --product "$(VERSION)"

.PHONY: publish-dry-core
publish-dry-core: ## cargo publish -p xpict-core --dry-run
	cd "$(ROOT)" && cargo publish -p xpict-core --dry-run

.PHONY: publish-dry-rust
publish-dry-rust: ## cargo publish -p xpict --dry-run (needs RDKit headers)
	cd "$(ROOT)" && CPLUS_INCLUDE_PATH="$(ROOT)/crates/xpict/compat/rdkit" \
		cargo publish -p xpict --dry-run

.PHONY: publish-dry-js
publish-dry-js: ## npm pack in js/ (no registry upload)
	cd "$(ROOT)/js" && npm pack

.PHONY: publish-dry-python
publish-dry-python: ## maturin build --release (wheels under target/wheels)
	cd "$(ROOT)" && uv run maturin build --release -m crates/xpict-py/Cargo.toml
