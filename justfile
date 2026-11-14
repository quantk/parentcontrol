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

linux_target := "x86_64-unknown-linux-musl"

# Includes the systemd unit, the tray autostart and install.sh.
# Static Linux binaries of the agent and the tray, in target/dist/linux.
dist-linux:
    cargo build --release --locked --target {{linux_target}} -p parentcontrol-agent -p parentcontrol-tray
    mkdir -p target/dist/linux
    cp target/{{linux_target}}/release/parentcontrol-agent target/{{linux_target}}/release/parentcontrol-tray target/dist/linux/
    cp deploy/linux/* target/dist/linux/

# Release binaries of the agent and the tray for Windows, in target/dist/windows.
dist-windows:
    cargo build --release --locked --target {{windows_target}} -p parentcontrol-agent -p parentcontrol-tray
    mkdir -p target/dist/windows
    cp target/{{windows_target}}/release/parentcontrol-agent.exe target/{{windows_target}}/release/parentcontrol-tray.exe target/dist/windows/

server_host := "quantick@192.168.1.135"

# The data stays in its Docker volume; only the code is replaced.
# Ship the committed tree to the home server and restart the container there.
deploy:
    git diff --quiet HEAD || { echo "Commit first: deploy ships the committed tree." >&2; exit 1; }
    git archive --format=tar HEAD | ssh {{server_host}} 'rm -rf ~/parentcontrol.new && mkdir ~/parentcontrol.new && tar -x -C ~/parentcontrol.new && rm -rf ~/parentcontrol && mv ~/parentcontrol.new ~/parentcontrol'
    ssh {{server_host}} 'cd ~/parentcontrol/deploy && docker compose up -d --build'

# Use the repository's git hooks (run once per clone).
install-hooks:
    git config core.hooksPath .githooks
