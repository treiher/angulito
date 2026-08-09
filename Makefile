.PHONY: all build check check-versions check-rust check-python format test test-rust test-e2e screenshot clean

all: check test

# Builds the release bundle into target/dx/angulito/release/web/public.
#
# --debug-symbols false is required. wasm-opt aborts with SIGABRT when asked
# to preserve this module's DWARF info, which dx requests by default. The
# bundle does not need debug symbols.
build:
	dx build --release --debug-symbols false

check: check-versions check-rust check-python

# Compares the dioxus-cli and wasm-bindgen-cli of the dev shell against the
# exact dioxus and wasm-bindgen pins in Cargo.toml. The pins must match the
# CLIs, which come from nixpkgs-unstable, so refreshing flake.lock can pull
# them apart. Without this the mismatch surfaces as an unrelated build or
# test failure.
check-versions:
	@dx_pin=$$(sed -n 's/^dioxus = { version = "=\([^"]*\)".*/\1/p' Cargo.toml); \
	wb_pin=$$(sed -n 's/^wasm-bindgen = "=\([^"]*\)".*/\1/p' Cargo.toml); \
	dx_version=$$(dx --version | awk '{print $$2}'); \
	wb_version=$$(wasm-bindgen --version | awk '{print $$2}'); \
	if [ -z "$$dx_pin" ] || [ -z "$$wb_pin" ] || [ -z "$$dx_version" ] || [ -z "$$wb_version" ]; then \
		echo "Cannot read a pin from Cargo.toml or a version from a CLI, so the pins are unchecked."; \
		exit 1; \
	fi; \
	status=0; \
	if [ "$$dx_pin" != "$$dx_version" ]; then \
		echo "dioxus is pinned to $$dx_pin, but dioxus-cli is $$dx_version"; \
		status=1; \
	fi; \
	if [ "$$wb_pin" != "$$wb_version" ]; then \
		echo "wasm-bindgen is pinned to $$wb_pin, but wasm-bindgen-cli is $$wb_version"; \
		status=1; \
	fi; \
	if [ $$status -ne 0 ]; then \
		echo "Set the pins in Cargo.toml to the CLI versions, then run cargo update -p <crate> for each."; \
	fi; \
	exit $$status

check-rust:
	cargo fmt --check
	cargo clippy --all-targets -- --warn clippy::pedantic --deny warnings

check-python:
	ruff format --check
	ruff check
	ty check

format:
	cargo fmt
	ruff format
	ruff check --fix-only

test: test-rust test-e2e

test-rust:
	cargo test

test-e2e:
	pytest

# Regenerates docs/screenshot.jpg for the README.
screenshot:
	SCREENSHOT=1 pytest tests/e2e/test_screenshot.py

# Removes the build output and the tool caches. The Nix dev shell in .direnv
# is kept, because discarding it means refetching the flake.
clean:
	rm -rf target dist test-results .pytest_cache .ruff_cache
	find tests -type d -name __pycache__ -prune -exec rm -rf {} +
