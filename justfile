# Developer commands. Run `just` to list them.

windows_target := "x86_64-pc-windows-gnu"
# The server runs only in a Linux container; everything else ships to Windows too.
windows_crates := "-p parentcontrol-core -p parentcontrol-agent -p parentcontrol-tray"

[private]
default:
    @just --list

# Every automated check; must be green before each commit.
check: fmt-check boundaries clippy clippy-windows test build-windows deny

# Format all code in place.
fmt:
    cargo fmt --all

# Fail if any file is not formatted.
fmt-check:
    cargo fmt --all --check

# Keep OS-specific cfg inside platform/ modules.
boundaries:
    scripts/check-cfg-boundaries.sh

# Lint for the host (Linux) with warnings as errors.
clippy:
    cargo clippy --workspace --all-targets --locked -- -D warnings

# Lint the Windows code paths without leaving Linux.
clippy-windows:
    cargo clippy {{windows_crates}} --all-targets --locked --target {{windows_target}} -- -D warnings

# Run the test suite.
test:
    cargo test --workspace --locked --quiet

# Cross-compile the Windows binaries to prove they link.
build-windows:
    cargo build {{windows_crates}} --locked --target {{windows_target}}

# Licenses, advisories, duplicate crates and sources.
deny:
    cargo deny --locked check

# Use the repository's git hooks (run once per clone).
install-hooks:
    git config core.hooksPath .githooks
