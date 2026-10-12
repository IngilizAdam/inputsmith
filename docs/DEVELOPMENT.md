# Developing Inputsmith

## Principles

- **Pure Rust**, no C libraries (HID access, the SQLite reader and the settings model are implemented in
  Rust).
- **Measure, don't guess.** When Logi Options+ behaviour matters (values written to a device, input
  injected into the system, timing), observe Options+ from the outside with the tools below and match the
  result. Document it in [`BEHAVIOR.md`](BEHAVIOR.md), and per-model observations in
  [`devices/`](devices/README.md).
- **Original work only.** Inputsmith is written from observed behaviour and public knowledge of the HID++
  protocol. Never contribute Logitech code, decompiled or disassembled code, or files extracted from
  Logitech software, and do not describe vendor internals in the repository. Describe what you observed
  (device traffic, injected input, files Options+ writes to disk) instead.
- **No vendor files in the repository.** Logitech artwork and device databases are only read at runtime
  from a local Options+ installation, when present.
- **Clean behaviour.** Only what users want is reproduced: no telemetry, no network access, no tracking
  (Inputsmith switches off the per-click analytics reporting Options+ enables on devices).
- **Never rely on device persistence.** Devices lose most settings on power-off and deep sleep; the agent
  re-applies everything when a device comes back.
- **No firmware updates.** Too risky to get wrong.

## Layout

| Crate | Binary | Purpose |
|---|---|---|
| `inputsmith-hid` | | HID transport: Windows HID API (overlapped I/O), Linux hidraw (untested). |
| `inputsmith-hidpp` | | HID++ 1.0/2.0: channels and request matching, receivers (Unifying, Bolt), feature tables, typed features (device info, battery, DPI, SmartShift, hi-res wheel, thumbwheel, special keys, config cookie, change host, Fn inversion). |
| `inputsmith-ipc` | | Settings model (`model`), agent API (`api`), local IPC transport (named pipe / Unix socket, length-prefixed JSON). |
| `inputsmith-input` | | Input injection (Windows `SendInput`), foreground application, launching, button swap. |
| `inputsmith-agent` | `inputsmith-agent` | Background service: device runtime, settings, actions, gestures, per-app profiles, backups, Options+ import (with a read-only SQLite reader), IPC server. |
| `inputsmith-gui` | `inputsmith-gui` | Settings window (iced) and tray icon; starts the agent if needed. |
| `inputsmith-cli` | `inputsmith` | Diagnostics and measurement: `list`, `info`, `hid`, `sniff`, `set`, `ipc`. |

State on Windows: settings `%APPDATA%\inputsmith\settings.json`, known devices `devices.json` next to it,
backups `%APPDATA%\inputsmith\backups\`, agent log `%LOCALAPPDATA%\inputsmith\agent.log` (when started
by the GUI).

## Build, test, run

```bash
cargo test --workspace
```

```bash
cargo run -p inputsmith-gui
```

The GUI starts the agent next to its executable and restarts it when the connection drops (stop the GUI
first if you want to run the agent by hand):

```bash
cargo run -p inputsmith-agent -- run
```

Installer (Windows; a per-user MSI in `dist/`, needs the WiX Toolset as described at the top of the
script):

```bash
powershell -ExecutionPolicy Bypass -File tools/package.ps1
```

Useful while developing:

- `inputsmith ipc get /devices` queries the running agent; `inputsmith ipc subscribe /devices/changed`
  prints events (in Git Bash set `MSYS_NO_PATHCONV=1` so paths are not rewritten).
- `tools/shot.sh <buttons|point|settings|app-settings> out.png [slot]` opens the GUI on a page and saves
  a window screenshot (`INPUTSMITH_START_PAGE`, `INPUTSMITH_START_SLOT`).
- Logi Options+ must not run at the same time as the agent: quit it (and its background agent) first.

## Continuous integration and releases

`.github/workflows/build.yml` runs the tests and builds the installers (Windows: `.msi`).
`ci.yml` runs it for every push to `main` and every pull request; the installers are attached to the run
as artifacts but nothing is published. Releases are made only from version tags:

1. Set the new version in `[workspace.package]` in `Cargo.toml` (semantic versioning; `0.2.0-rc.1` for a
   pre-release) and run `cargo build` so `Cargo.lock` follows. Commit and push.
2. Tag that commit and push the tag:

   ```bash
   git tag v0.2.0
   git push origin v0.2.0
   ```

`release.yml` then checks that the tag matches `Cargo.toml`, builds every platform and publishes a GitHub
release with the installers and generated notes (tags with a `-` suffix become
pre-releases). To rebuild an existing tag, run the *Release* workflow by hand from the Actions tab; it
replaces the release's files. A new platform (e.g. Linux `.deb`/`.rpm`) only needs a job in `build.yml`
that uploads its packages as an artifact.

## Observing Options+

1. Run Options+ and stop `inputsmith-agent`.
2. `inputsmith sniff` prints, with millisecond timestamps, HID++ notifications, the device's responses to
   other software's requests (so you see what Options+ reads and writes), raw XY of taken-over buttons,
   and every keystroke, wheel event and mouse button injected into the system.
3. `inputsmith info` reads device state (read-only) at any time; `inputsmith set` writes test values.
4. Change one setting at a time in Options+ and note what changes on the device and in the injected
   input.
5. For timing-sensitive behaviour (gestures), keep the `sniff` log and add the trace to
   `crates/agent/src/testdata/gesture_traces.json` so the replay test covers it.

## Adding support for a device

1. `inputsmith info` with the device connected (feature table, controls, current state).
2. Create `docs/devices/<model>.md` from the template in [`devices/README.md`](devices/README.md),
   including the power-cycle persistence test (results differ between models).
3. If Options+ is available, compare with `inputsmith sniff` what it writes on connect and for each
   setting.
4. Implement missing features in `inputsmith-hidpp` (typed wrapper) and `inputsmith-agent` (settings →
   device).

## Gotchas

- In Git Bash, heredocs eat backslashes in Rust/PowerShell sources; use an editor.
- Rebuilding while `inputsmith-agent`, `inputsmith-gui` or `inputsmith sniff` runs fails (locked
  executables).
