.PHONY: build test lint fmt release release-x86_64 release-aarch64 release-macos clean

build:
	cargo build

test:
	cargo test

coverage:
	cargo llvm-cov --branch --fail-under-lines 80 --fail-under-regions 80

lint:
	cargo clippy

fmt:
	cargo fmt

release: release-x86_64 release-aarch64 release-macos

release-x86_64:
	cargo build --release --target x86_64-unknown-linux-musl

release-aarch64:
	cargo build --release --target aarch64-unknown-linux-musl

release-macos:
	cargo zigbuild --release --target aarch64-apple-darwin

clean:
	cargo clean
