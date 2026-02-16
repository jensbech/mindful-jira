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

# Build all 5 targets, create release directory, and publish to GitHub
release-all: (_run "release-all")
    #!/usr/bin/env bash
    set -e
    VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
    TAG="v${VERSION}"
    echo "Publishing ${TAG} to GitHub..."
    if gh release view "$TAG" --repo jensbech/mindful-jira &>/dev/null; then
        echo "Release ${TAG} already exists, uploading assets..."
        gh release upload "$TAG" release/* --repo jensbech/mindful-jira --clobber
    else
        gh release create "$TAG" release/* \
            --repo jensbech/mindful-jira \
            --title "$TAG" \
            --notes "Release ${VERSION}" \
            --latest
    fi
    echo "Done: https://github.com/jensbech/mindful-jira/releases/tag/${TAG}"

# Build ARM binary, create release directory, and publish to GitHub
release: (_run "release")
    #!/usr/bin/env bash
    set -e
    VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
    TAG="v${VERSION}"
    echo "Publishing ${TAG} to GitHub..."
    if gh release view "$TAG" --repo jensbech/mindful-jira &>/dev/null; then
        echo "Release ${TAG} already exists, uploading assets..."
        gh release upload "$TAG" release/* --repo jensbech/mindful-jira --clobber
    else
        gh release create "$TAG" release/* \
            --repo jensbech/mindful-jira \
            --title "$TAG" \
            --notes "Release ${VERSION}" \
            --latest
    fi
    echo "Done: https://github.com/jensbech/mindful-jira/releases/tag/${TAG}"

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
