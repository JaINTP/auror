# Change Log

All notable shifts in this project's architecture, dependencies, and implementations will be documented in this file.

## [Unreleased]

### Added
- Shared IPC layer (`shared` crate) using newline-delimited JSON stream framing.
- Asynchronous background daemon (`aurord` crate) featuring:
  - Startup lock mutual exclusion on `/tmp/aurord.lock` via `fs4`.
  - Package loader and central configuration manager (`~/.config/auror/config.toml`).
  - Concurrent upstream check pool (GitHub refs, PyPI JSON API, ElectronBuilder YAML).
  - Sequential builder worker task using custom environment-sanitized `makepkg` pipeline.
  - Periodic 3-hour cycle scheduler.
  - UNIX domain socket IPC server listening on `/run/user/1000/aurord.sock`.
- Interactive dashboard TUI (`aurorc` crate) featuring:
  - Ratatui layout split into status metadata bar, local packages table, details card, and bottom compile logs console.
  - Non-blocking input event thread and async IPC connection client.
  - Rich, context-aware styling and color highlights for compile logs and live activity stream.
  - Automatic background reconnection polling when the daemon goes offline or is started after the TUI.
- Test suites covering locking, environment cleaning, PKGBUILD mutations, and JSON framing.
- Systemd user service configuration file (`aurord.service`) supporting standard system daemon management.
- Split Discord notification webhooks (`notification_webhook_url` for successful builds, `error_webhook_url` for failed builds).
- PKGBUILD script for standard Arch Linux packaging from the local repository.

### Changed
- Resolved home directory dynamically using the `home` crate instead of checking the `HOME` environment variable with a hardcoded user fallback in both `aurord` and `aurorc` TUI.
- Resolved user-specific UNIX socket path using `XDG_RUNTIME_DIR` environment variable instead of hardcoding user ID `1000`.
- Corrected repository folder name references from `aur_updater` to `auror` in both `PKGBUILD` and `aurord.service`.
- Updated `PKGBUILD` to build dynamically from the current directory using `$PWD` instead of a hardcoded path.

### Added (Dependencies)
- Added `home = "0.5"` to `aurord` and `aurorc` dependencies.
