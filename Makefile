# Lockra repository tasks. Every target is a thin wrapper so a shell and CI run the same thing.
# Variables use `:=` on purpose: an environment variable must not be able to lower a floor.

SHELL := /usr/bin/env bash
.DEFAULT_GOAL := help

CARGO := cargo
PNPM  := pnpm
# Line coverage floor of crates/* (the desktop shell is covered by its own IPC tests and the smoke run).
CRATE_COVERAGE_MIN := 90

.PHONY: help
help: ## List targets
	@grep -E '^[a-zA-Z0-9_.-]+:.*## ' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*## "}; {printf "  %-22s %s\n", $$1, $$2}'

.PHONY: fmt
fmt: ## Format Rust, then everything else oxfmt reads (TS, JSON, YAML, TOML, Markdown)
	$(CARGO) fmt --all
	$(PNPM) run fmt

.PHONY: fmt-check
fmt-check: ## Verify formatting without writing (CI gate)
	$(CARGO) fmt --all -- --check
	$(PNPM) run fmt:check

.PHONY: lint
lint: ## clippy -D warnings, then oxlint + tsc per web package
	$(CARGO) clippy --workspace --all-targets -- -D warnings
	$(PNPM) -r run lint

.PHONY: test
test: ## Rust and web tests
	$(CARGO) test --workspace --all-targets
	$(PNPM) -r run test

.PHONY: build
build: ## The shipped artifact: the release app with its web bundle embedded, not packaged
	$(PNPM) tauri build --no-bundle

.PHONY: check
check: ## The local CI gate: every gate, stopping at the first failure (scripts/verify-all.sh)
	scripts/verify-all.sh

.PHONY: pre-ci
pre-ci: smoke-desktop linux-x64 windows-x64 ## Packaged-artifact preflight: the real app under Xvfb, the deb, the cross-built installer

.PHONY: hooks
hooks: ## Install the pre-commit (make fmt) and pre-push (make lint test) hooks
	@command -v pre-commit >/dev/null 2>&1 \
		|| { echo "pre-commit is required: pipx install pre-commit (https://pre-commit.com)"; exit 1; }
	pre-commit install --hook-type pre-commit --hook-type pre-push

.PHONY: coverage
coverage: ## Rust line coverage of crates/*, failing under CRATE_COVERAGE_MIN
	$(CARGO) llvm-cov --workspace --exclude lockra-desktop --all-targets --summary-only --fail-under-lines $(CRATE_COVERAGE_MIN)

.PHONY: verify
verify: check ## Alias of check

.PHONY: deny
deny: ## Licences, bans and sources of the dependency graph
	$(CARGO) deny check licenses bans sources

.PHONY: smoke-desktop
smoke-desktop: ## Drive the real app under Xvfb; screenshots to docs/acceptance/screens/desktop
	scripts/smoke-desktop-linux.sh

.PHONY: showcase
showcase: ## Screenshot the component showcase in the four themes
	scripts/screenshot-showcase.sh

.PHONY: site-screens
site-screens: ## The documentation site's screenshots, from the real app under Xvfb (docs/site/README.md)
	scripts/capture-site-screens.sh docs/site/public/screens

.PHONY: linux-x64
linux-x64: ## Build and check the deb (LOCKRA_APPIMAGE=1 adds an AppImage)
	scripts/build-linux-x64.sh

.PHONY: windows-x64
windows-x64: ## Cross-build the Windows NSIS installer with cargo-xwin
	scripts/build-windows-x64.sh
