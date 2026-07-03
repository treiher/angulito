.PHONY: all build check check-rust check-python format test test-rust test-e2e screenshot clean

all: check test

# Builds the release bundle into target/dx/angulito/release/web/public.
#
# --debug-symbols false is required. wasm-opt aborts with SIGABRT when asked
# to preserve this module's DWARF info, which dx requests by default. The
# bundle does not need debug symbols.
build:
	dx build --release --debug-symbols false

check: check-rust check-python

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
