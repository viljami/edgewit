.PHONY: all fmt clippy test clean setup teardown

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

clean: teardown
	cargo clean
