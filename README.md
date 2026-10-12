# Inputsmith

**An open-source replacement for Logi Options+, written in Rust.**

Logitech's Options+ configures their mice and keyboards — button assignments, gestures, pointer speed,
SmartShift scrolling — but it is closed source, needs a background service with network access, and
does not exist for Linux. Inputsmith re-implements it from scratch: same settings, same behaviour on the
device, no account, no telemetry.

> **Status: early.** Works on **Windows** today and fully supports the **MX Anywhere 3** (the device it
> was developed with). Other HID++ mice likely work for the common features but are untested. Linux is
> the next milestone; macOS comes later. See the [feature matrix](docs/FEATURES.md) for exactly what is
> and is not supported.

This is an independent project, not affiliated with or endorsed by Logitech.

## What works

- Pointer speed, SmartShift (sensitivity and scrolling force), scroll direction, smooth scrolling,
  horizontal scrolling with the side buttons, left/right button swap
- Button re-assignment with the Options+ action catalog, per-application profiles
- Gesture button presets (app navigation, media controls, virtual desktops, …); step distances and timing
  measured against Options+
- Settings re-applied automatically when a device wakes up or is switched back on; switched-off devices
  stay listed as inactive
- Settings window and tray icon modelled on Options+
- Import your existing settings straight from Logi Options+; local backups, export and import
- Start at login, per-user install without administrator rights

Behaviour that matters was **measured against Options+** — the same values are written to the device
and the same input reaches your applications. The complete list, including what is partial, untested
or missing, is in [`docs/FEATURES.md`](docs/FEATURES.md).

## What it deliberately does not do

- **No firmware updates** — a failed update can brick a device; use Logitech's tools for that.
- **No network access at all** — no account, no cloud sync, no analytics, no ads or offers.
  Inputsmith even switches off the per-click reporting that Options+ turns on inside your devices.

## Install (Windows)

There are no prebuilt releases yet; build the package from source (needs a Rust toolchain):

```bash
powershell -ExecutionPolicy Bypass -File tools/package.ps1
```

Then unpack `dist/inputsmith-<version>-windows-x64.zip` and run `install.ps1` from it. It installs for the
current user into `%LOCALAPPDATA%\Programs\inputsmith` and adds a Start Menu entry. `uninstall.ps1`
removes it again (your settings are kept).

## Switching from Logi Options+

1. **Quit Logi Options+**, including its background agent — both cannot control the devices at the
   same time (you can uninstall it, or just stop it).
2. Start Inputsmith. In **Application Settings**, use **Import from Logi Options+** to take over your
   existing button assignments, scrolling settings and application profiles.
3. Optionally turn on **Start at login**.

If Options+ is still installed, Inputsmith uses its device pictures, product names and action catalog
(read at runtime; nothing is copied). Without it, devices are shown with generic art and a smaller set of
built-in actions.

## Command-line tools

| Command | Purpose |
|---|---|
| `inputsmith-gui` | Settings window and tray icon (starts the agent automatically). |
| `inputsmith-agent run` | The background service that applies settings and handles buttons. |
| `inputsmith-agent import-optionsplus` | Import settings from the local Options+ installation. |
| `inputsmith-agent backup [label]`, `backups`, `restore <file>` | Local backups. |
| `inputsmith-agent export <file>`, `import <file>` | Settings files. |
| `inputsmith list`, `inputsmith info` | Show receivers, devices, features and current device state (read-only). |
| `inputsmith sniff` | Watch HID++ traffic and injected input (for development and device reports). |

## Documentation

- [Feature matrix](docs/FEATURES.md) — what is supported, partial, untested, missing or not planned
- [Roadmap](docs/ROADMAP.md) — milestones and progress
- [Devices](docs/devices/README.md) — tested devices and how to report a new one
- [Development](docs/DEVELOPMENT.md) — architecture, building, measuring Options+
- [Measured behaviour](docs/BEHAVIOR.md) — how devices are driven and what was measured against Options+

## Contributing

Logitech makes far more devices than any one person owns, so contributions are essential. See
[CONTRIBUTING.md](CONTRIBUTING.md); in short:

- **Device not supported, or a feature missing?** Open a *Device support request* with the output of
  `inputsmith info` (it only reads from the device).
- **Tried Inputsmith with your device?** A *Device test report* helps even when everything works.
- **Found a bug or have an idea?** Open a *Bug report* or *Feature request*.
- **Want to code?** Check the [roadmap](docs/ROADMAP.md) — Linux support is the next big milestone — and
  read the principles in [Development](docs/DEVELOPMENT.md#principles).

## Legal

Inputsmith is an independent project, written from scratch from observed behaviour and public knowledge of
the HID++ protocol. It is not affiliated with, sponsored or endorsed by Logitech. Logitech, Logi,
Logi Options+ and Logitech product names are trademarks of Logitech; they are used here only to describe
what Inputsmith is compatible with.

The repository contains no Logitech code, binaries, artwork or device databases. Where such data is
useful (device pictures, product names, the action catalog), it is read at runtime from a local Logi
Options+ installation, if one is present.

Inputsmith is free software, licensed under the [GNU General Public License v3.0 or later](LICENSE).
