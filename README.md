# Auror

`auror` is a modern, high-performance Arch Linux local AUR repository package manager. Written in Rust, it consists of an asynchronous background daemon (`aurord`) and an interactive terminal dashboard companion TUI (`aurorc`). Together, they provide automated updates, package integrity checks, custom build environment isolation, and instant Discord alerts.

---

## Architecture Overview

The project is structured as a Cargo workspace with three crates:
- **`shared`**: Defines the shared IPC data payload structures and handles JSON-line framed protocol serialization over UNIX Domain Sockets.
- **`aurord`**: The background daemon. It coordinates repository locking, schedules periodic checks, pulls upstream metadata, builds packages using `makepkg`, and broadcasts events to IPC clients and Discord channels.
- **`aurorc`**: The terminal companion TUI dashboard. Built with `ratatui` and `crossterm`, it provides a responsive layout to monitor progress, trigger manual updates, and browse real-time compilation log streams.

```
aur_updater/
├── Cargo.toml
├── PKGBUILD                  # Arch Linux package build script
├── aurord.service            # systemd user service template
├── shared/                   # Common IPC structs and protocol
├── aurord/                   # Asynchronous background daemon
└── aurorc/                   # Interactive TUI dashboard
```

---

## Features

### 📦 Daemon (`aurord`)
- **Process Mutual Exclusion**: Uses file locking via `fs4` (`/tmp/aurord.lock`) to prevent multiple daemon instances from running.
- **Upstream Version Checkers**: Supports checking upstream versions via three distinct checker pipelines:
  - **GitHub**: Queries git tags/refs asynchronously (`git ls-remote`).
  - **PyPI**: Fetches package metadata from PyPI's JSON API.
  - **Electron-Builder**: Parses Electron-Builder release YAML metadata directly.
- **Environment Sanitization**: Safely purges Python virtual environments (`VIRTUAL_ENV`) and cleans `PATH` during `makepkg` execution to enforce Arch Linux system Python environments.
- **Git Automation**: Stashes local modifications, rebases upstream commits, updates checksums (`updpkgsums`), regenerates metadata (`.SRCINFO`), and pushes version increments.
- **IPC UNIX Domain Socket**: Binds to `/run/user/1000/aurord.sock` using a lightweight newline-delimited JSON protocol.
- **Split Discord Alerts**: Triggers notifications complete with diff information:
  - `notification_webhook_url`: Posts successes, completions, and version bumps.
  - `error_webhook_url`: Posts compilation failure traces and git conflicts.

### 🖥️ Dashboard TUI (`aurorc`)
- **Modular Responsive Dashboard**: Split into a status/uptime top bar, a scrollable select package list, a package information card, and a tall live log panel.
- **Dynamic Log Coloring**: Renders incoming compilation log lines contextually:
  - `makepkg` stage headers (`==>`) highlighted dynamically based on phase (Success in green, structural changes in blue, progress in cyan, errors in red).
  - `makepkg` sub-headers (`  ->`) highlighted based on results (Passed tests in green, cleanup in dark gray, check queries in cyan).
  - Package-specific bracketed tags (e.g. `[python-mempalace]`) highlighted based on activity status (Success in green, building in blue, error in red, generic in magenta).
  - Daemon/system messages highlighted in magenta.
- **Automatic Connection Resilience**: Polls the socket automatically every 1 second in the background if the daemon is offline, reconnecting instantly once the UDS socket becomes available without requiring a TUI relaunch.
- **Zero CPU Idle Spinning**: Uses Tokio select branch guards to disable reading from the channel when disconnected.

---

## Installation & Setup

### 1. Build and Install via PKGBUILD (Recommended)
This packages the workspace using Arch's standard `makepkg` tool, placing `aurord` and `aurorc` in `/usr/bin/` and registering the systemd service.

```bash
# 1. Add and commit files to git (required by VCS PKGBUILD)
git add -A
git commit -m "initial commit"

# 2. Package and install
makepkg -si
```

### 2. Configure the Daemon
Create your configuration file at `~/.config/auror/config.toml`:

```toml
[packages.python-mempalace]
type = "pypi"
package = "mempalace"

[packages.panoptic]
type = "github"
repo = "JaINTP/Panoptic"

[packages.devpod-community-bin]
type = "github"
repo = "skevetter/devpod"

[packages.capacities-appimage]
type = "electron-builder"
url = "https://2vks4.upcloudobjects.com/capacities-desktop-app/latest-linux.yml"

[discord]
notification_webhook_url = "https://discord.com/api/webhooks/your-success-channel-webhook"
error_webhook_url = "https://discord.com/api/webhooks/your-error-channel-webhook"
```

### 3. Run the Service
Manage the background daemon using `systemd` user services:

```bash
# Reload user-space systemd configurations
systemctl --user daemon-reload

# Start and enable the service immediately
systemctl --user enable --now aurord.service

# Check status and watch live service journals
systemctl --user status aurord.service
journalctl --user -u aurord.service -f
```

---

## TUI Keyboard Map

Launch the dashboard via:
```bash
aurorc
```

- **`Up` / `Down` or `j` / `k`**: Navigate and select packages in the list.
- **`u`**: Trigger a manual update check (and forced rebuild) for the selected package.
- **`a`**: Trigger a manual update check for **all** monitored packages.
- **`q` or `Esc`**: Gracefully quit the TUI dashboard.

---

## Development

To build or run tests manually without packaging:

```bash
# Run all workspace unit tests
cargo test

# Run code style checks (non-negotiable)
cargo fmt --all
cargo clippy -- -D warnings

# Build release targets
cargo build --release
```
