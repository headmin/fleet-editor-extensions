.PHONY: help build build-release test clean install run-example generate-all

help:
	@echo "Fleet Schema Generator - Make targets"
	@echo ""
	@echo "  build            Build debug binary"
	@echo "  build-release    Build optimized release binary"
	@echo "  test             Run all tests"
	@echo "  clean            Clean build artifacts"
	@echo "  install          Install binary to ~/.cargo/bin"
	@echo "  run-example      Run example generation"
	@echo "  generate-all     Generate all schema formats"
	@echo ""

build:
	cargo build

build-release:
	cargo build --release

test:
	cargo test

clean:
	cargo clean
	rm -rf output/
	rm -rf schema-defs/base.yml

install: build-release
	cargo install --path .

run-example:
	cargo run -- generate \
		--fleet-version latest \
		--output ./output \
		--editor all

generate-all: build-release
	./target/release/fleet-schema-gen generate \
		--output ./output \
		--editor all
	@echo ""
	@echo "✅ Schemas generated in ./output/"
	@echo ""
	@echo "  VSCode:        ./output/vscode/"
	@echo "  Sublime Text:  ./output/sublime/"
	@echo "  Strict:        ./output/strict/"
	@echo ""

check:
	cargo check
	cargo clippy -- -D warnings
	cargo fmt -- --check

format:
	cargo fmt

watch:
	cargo watch -x build

dev: build
	cargo run -- --help
