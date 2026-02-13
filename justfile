#!/usr/bin/env just --justfile

# Default recipe
default:
    @just --list

# Get version from Cargo.toml
_version:
    @grep '^version' Cargo.toml | head -1 | sed 's/version = "\([^"]*\)".*/\1/'

# Build release binary for current architecture
build:
    @echo "Building release binary..."
    cargo build --release
    @echo "Build complete"
    @echo ""
    @echo "Binary:"
    @ls -lh target/release/mindful-jira

# Build release binary for Apple Silicon (aarch64)
build-arm:
    @echo "Building for Apple Silicon (aarch64)..."
    cargo build --release --target aarch64-apple-darwin
    @echo "aarch64 complete"
    @echo ""
    @echo "Binary:"
    @ls -lh target/aarch64-apple-darwin/release/mindful-jira

# Build release binary for Intel (x86_64)
build-intel:
    @echo "Building for Intel (x86_64)..."
    cargo build --release --target x86_64-apple-darwin
    @echo "x86_64 complete"
    @echo ""
    @echo "Binary:"
    @ls -lh target/x86_64-apple-darwin/release/mindful-jira

# Create release directory with Apple Silicon binary
release: build-arm
    @VERSION=$(just _version); \
    RELEASE_DIR="release"; \
    mkdir -p "$RELEASE_DIR"; \
    cp target/aarch64-apple-darwin/release/mindful-jira "$RELEASE_DIR/mindful-jira-$VERSION-aarch64-apple-darwin"; \
    echo ""; \
    echo "Release binary created in $RELEASE_DIR/"; \
    echo ""; \
    ls -lh "$RELEASE_DIR/"; \
    echo ""; \
    echo "Upload binary to Forgejo as release asset"

# Build debug version (faster for development)
build-dev:
    cargo build

# Run tests
test:
    cargo test

# Format and lint
lint:
    cargo fmt
    cargo clippy -- -D warnings

# Clean build artifacts
clean:
    cargo clean
    rm -rf release/
    @echo "Cleaned"
