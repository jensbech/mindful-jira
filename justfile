#!/usr/bin/env just --justfile

# Centralized build script (raw URL)
BUILD_SCRIPT := "https://git.bechsor.no/jens/rust-build-tools/raw/branch/main/rust-build"

# Default recipe
default:
    @just --list

# Run centralized build script (local sibling or remote fallback)
[private]
_run *ARGS:
    #!/usr/bin/env bash
    set -e
    [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
    if [ -x "../rust-build-tools/rust-build" ]; then
        ../rust-build-tools/rust-build {{ARGS}}
    else
        SCRIPT=$(mktemp)
        trap 'rm -f "$SCRIPT"' EXIT
        curl -fsSL "{{BUILD_SCRIPT}}" -o "$SCRIPT"
        bash "$SCRIPT" {{ARGS}}
    fi

# Install cross-compilation toolchain and targets
setup: (_run "setup")

# Build release binary for current architecture
build: (_run "build")

# Build for Apple Silicon (aarch64)
build-arm: (_run "build-arm")

# Build for Intel macOS (x86_64)
build-intel: (_run "build-intel")

# Build for Linux x86_64 (static musl)
build-linux-x64: (_run "build-linux-x64")

# Build for Linux ARM64 (static musl)
build-linux-arm: (_run "build-linux-arm")

# Build for Windows x86_64
build-windows: (_run "build-windows")

# Build all 5 targets and create release directory
release-all: (_run "release-all")

# Build ARM binary and create release directory
release: (_run "release")

# Build debug version (faster for development)
build-dev: (_run "build-dev")

# Run tests
test: (_run "test")

# Format and lint
lint: (_run "lint")

# Clean build artifacts
clean: (_run "clean")

# Print version from Cargo.toml
version: (_run "version")
