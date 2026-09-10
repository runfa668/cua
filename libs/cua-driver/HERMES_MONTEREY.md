# Hermes Agent + cua-driver on macOS Monterey

This branch contains a compatibility build of `cua-driver` intended for macOS 12.7 Monterey.

## What is changed

- Rust deployment target lowered to macOS 12.3.
- macOS 15 `SCRecordingOutput` is disabled behind a compatibility shim.
- macOS 14+ `SCScreenshotManager` window screenshots are replaced by the built-in macOS `screencapture` utility.
- Accessibility (AX), keyboard/mouse input, app/window automation and the MCP surface remain available.

Native ScreenCaptureKit video recording is intentionally unavailable in this compatibility build.

## Install for Hermes

Clone this branch and run:

```bash
git clone -b monterey-support-v1 https://github.com/runfa668/cua.git
cd cua
bash libs/cua-driver/scripts/install-hermes-monterey.sh
```

The installer builds the release binary and creates:

```text
~/.local/bin/cua-driver-monterey
~/.hermes/cua-driver-monterey.env
```

It also adds a small source line to your shell startup file so terminal-launched Hermes automatically receives:

```bash
HERMES_CUA_DRIVER_CMD="$HOME/.local/bin/cua-driver-monterey"
```

## Verify

Open a new Terminal window and run:

```bash
hermes computer-use status
hermes computer-use doctor
```

Then start Hermes with computer-use tools:

```bash
hermes -t computer_use chat
```

## macOS permissions

Computer control requires Accessibility permission. Screenshots require Screen Recording permission. Use the process identity/path reported by `hermes computer-use doctor` when granting permissions in macOS System Preferences / Security & Privacy.

## Updating the compatibility build

```bash
cd cua
git pull
bash libs/cua-driver/scripts/install-hermes-monterey.sh
```

The symlink remains stable, so Hermes does not need to be reconfigured after rebuilding.
