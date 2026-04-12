.PHONY: all fmt clippy test test-e2e test-e2e-skip-build clean setup teardown

all: fmt clippy test

setup:
	@echo "Setting up test environment..."
	mkdir -p data/indexes

teardown:
	@echo "Cleaning up test environment..."
	rm -rf data/ test_dir/ test_index/

fmt:
	cargo fmt --all

clippy:
	cargo clippy --all-targets --all-features -- -W clippy::pedantic

test: setup
	cargo test
	$(MAKE) teardown

test-e2e:
	@echo "Running container end-to-end tests (includes docker build)..."
	bash scripts/test-container.sh

test-e2e-skip-build:
	@echo "Running container end-to-end tests (skipping docker build)..."
	bash scripts/test-container.sh --skip-build

clean: teardown
	cargo clean
