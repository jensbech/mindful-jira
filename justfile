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

# Build for Linux x86_64 (static musl)
build-linux-x64:
    @echo "Building for Linux x86_64 (musl)..."
    cargo zigbuild --release --target x86_64-unknown-linux-musl
    @echo "Linux x86_64 complete"
    @echo ""
    @echo "Binary:"
    @ls -lh target/x86_64-unknown-linux-musl/release/mindful-jira

# Build for Linux ARM64 (static musl)
build-linux-arm:
    @echo "Building for Linux ARM64 (musl)..."
    cargo zigbuild --release --target aarch64-unknown-linux-musl
    @echo "Linux ARM64 complete"
    @echo ""
    @echo "Binary:"
    @ls -lh target/aarch64-unknown-linux-musl/release/mindful-jira

# Build for Windows x86_64
build-windows:
    @echo "Building for Windows x86_64..."
    cargo zigbuild --release --target x86_64-pc-windows-gnu
    @echo "Windows x86_64 complete"
    @echo ""
    @echo "Binary:"
    @ls -lh target/x86_64-pc-windows-gnu/release/mindful-jira.exe

# Ensure all cross-compilation tooling and targets are installed
setup:
    rustup default stable
    cargo install cargo-zigbuild
    rustup target add aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-musl aarch64-unknown-linux-musl x86_64-pc-windows-gnu

# Build all 5 targets and place named binaries in release/
release-all: setup build-arm build-intel build-linux-x64 build-linux-arm build-windows
    @VERSION=$(just _version); \
    RELEASE_DIR="release"; \
    mkdir -p "$RELEASE_DIR"; \
    cp target/aarch64-apple-darwin/release/mindful-jira "$RELEASE_DIR/mindful-jira-$VERSION-aarch64-apple-darwin"; \
    cp target/x86_64-apple-darwin/release/mindful-jira "$RELEASE_DIR/mindful-jira-$VERSION-x86_64-apple-darwin"; \
    cp target/x86_64-unknown-linux-musl/release/mindful-jira "$RELEASE_DIR/mindful-jira-$VERSION-x86_64-unknown-linux-musl"; \
    cp target/aarch64-unknown-linux-musl/release/mindful-jira "$RELEASE_DIR/mindful-jira-$VERSION-aarch64-unknown-linux-musl"; \
    cp target/x86_64-pc-windows-gnu/release/mindful-jira.exe "$RELEASE_DIR/mindful-jira-$VERSION-x86_64-pc-windows-gnu.exe"; \
    echo ""; \
    echo "Release binaries created in $RELEASE_DIR/"; \
    echo ""; \
    ls -lh "$RELEASE_DIR/"; \
    echo ""; \
    echo "Upload binaries to Forgejo as release assets"

# Create release directory with Apple Silicon binary only
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
