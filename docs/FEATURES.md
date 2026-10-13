# Feature support

What Inputsmith can do today compared with Logi Options+, and what it will never do. The
[roadmap](ROADMAP.md) tracks the work on everything that is not supported yet.

| Mark | Meaning |
|---|---|
| ✅ | Supported and verified on real hardware (see [tested devices](devices/README.md)) |
| 🟡 | Partly supported — see the note |
| 🔬 | Implemented but not yet verified on hardware — reports welcome |
| ❌ | Not supported yet (planned) |
| 🚫 | Not planned (see [Not planned](#not-planned)) |

Behaviour marked ✅ was measured against Options+ 2.8 (same values sent to the device, same input
injected into the system) rather than guessed. Details are in
[Measured behaviour](BEHAVIOR.md).

## Platforms

| Platform | Status | Notes |
|---|---|---|
| Windows 10 / 11 (x64) | ✅ | Developed and tested on Windows 11. |
| Linux | ❌ | The main goal after Windows. Device access (hidraw) is written but untested; input injection and foreground-app detection are missing. |
| macOS | ❌ | Later. |

## Connections

| Feature | Status | Notes |
|---|---|---|
| Unifying receiver | ✅ | Tested with C52B. |
| Bolt receiver | 🔬 | Pairing table and device access implemented. |
| Bluetooth (direct) | 🔬 | HID++ over the Bluetooth collection is implemented. |
| USB cable (HID++ device on USB) | 🔬 | |
| Device switched off / asleep / woken up | ✅ | Shown as **INACTIVE**; settings are re-applied when it comes back (devices lose them on power-off and deep sleep). |
| Receiver plugged in / out while running | 🔬 | Hot-plug is implemented; unplugging is untested. |
| Pairing new devices ("Add device") | ❌ | |
| Easy-Switch | 🟡 | Number of channels is shown; switching hosts from the app is not implemented. |

## Mouse settings (Point & Scroll)

| Feature | Status | Notes |
|---|---|---|
| Pointer speed | ✅ | Applied as sensor DPI with the same curve as Options+. |
| Pointer speed on high-resolution sensors (MX Master 3S / 4, MX Anywhere 3S) | ❌ | These use the extended DPI feature (0x2202), which is not implemented. |
| Pointer speed on mice without adjustable DPI | ❌ | Options+ changes the Windows pointer speed for these; Inputsmith does not yet. |
| SmartShift on/off, sensitivity | ✅ | |
| Scrolling force (SmartShift with tunable torque) | ✅ | |
| Scroll direction (standard / inverted) | ✅ | |
| Smooth scrolling | ✅ | Same input as Options+ (one 120-unit event per wheel report). |
| Horizontal scrolling with side buttons (hold + wheel) | ✅ | |
| Swap left/right buttons | ✅ | System setting, applies to every mouse (as in Options+). |
| Per-model defaults | ✅ | From the built-in device description (same values as Options+). |
| Scroll speed | ❌ | |
| Thumbwheel (MX Master etc.) | ❌ | Feature access exists; settings and actions not wired up. The wheel still scrolls horizontally on its own. |
| Report rate, onboard profiles (gaming mice) | ❌ | |

## Buttons and actions

| Feature | Status | Notes |
|---|---|---|
| Re-assign buttons (middle, back, forward, top / mode-shift, gesture button) | ✅ | Left and right click are never taken over. The picker offers the common actions (28, incl. gestures); the rest of Options+'s list is still to come. |
| Default bindings (gesture button → Gestures) | 🔬 | As in Options+; other buttons keep their own function until re-assigned. |
| Per-application profiles | 🔬 | Switches with the foreground application; covered by unit tests, not yet verified end-to-end. |
| Keyboard shortcut | 🟡 | Runs; the GUI offers presets only — a shortcut recorder is missing. |
| Mouse actions (click, back/forward, wheel, middle) | ✅ | Back/forward verified; the others use the same path. |
| Media keys, volume | 🔬 | |
| DPI up/down/cycle, SmartShift toggle | 🔬 | |
| System actions (task view, desktops, snap, lock, show desktop, copy/paste/undo, zoom, screenshot, …) | 🟡 | Common ones are implemented; brightness, sleep/shutdown and some window actions are missing. |
| Open application / file / folder / web page, text block | 🟡 | Run when present in imported settings; the GUI cannot create them yet (no app picker, URL or text input). |
| Delays and sequences | 🔬 | From imported settings. |
| Macro / keystroke recording | ❌ | |
| Predefined application profiles (Chrome, Office, Adobe, …) | ❌ | |

## Gestures (gesture button)

| Preset | Status | Notes |
|---|---|---|
| App navigation | ✅ | Replay-tested against recorded Options+ gestures (speed-dependent steps, action timing). |
| Media controls | 🔬 | Replay-tested against recorded Options+ gestures; not yet tried on hardware with Inputsmith. |
| Virtual desktops, Window management, Arrange windows | 🟡 | Same engine and parameters as the presets above; not recorded against Options+. |
| Zoom / rotate, Pan | 🟡 | Implemented; speed and scaling not measured. |
| Custom gesture | 🟡 | Runs from imported settings; no editor. |

## Desktop features

| Feature | Status | Notes |
|---|---|---|
| Tray icon with devices and battery | ✅ | |
| Start at login, single instance | ✅ | |
| Low-battery notifications | ❌ | |
| On-screen displays (Caps/Num lock, DPI, mic mute, battery) | ❌ | |
| Actions Ring (radial menu) | ❌ | |
| Smart Actions (triggered action sequences) | ❌ | |
| Flow (one mouse and keyboard across computers) | ❌ | Needs research with two computers. |

## Settings and data

| Feature | Status | Notes |
|---|---|---|
| Local settings in the Options+ profile format | ✅ | `%APPDATA%\inputsmith\settings.json` |
| Import from a local Logi Options+ installation | ✅ | Reads its `settings.db` directly. |
| Export / import settings file | ✅ | |
| Local backups and restore | ✅ | A backup is made before every import and restore. |
| Reset device to defaults | 🔬 | |

## Other device types

Mice and trackballs: see the [device list](devices/README.md) for every model and its status.

| Device type | Status | Notes |
|---|---|---|
| Older HID++ 1.0 mice (Performance MX, MX Anywhere 1st gen, M570, early M705) | ❌ | Configured through registers instead of features; not implemented. |
| Haptic feedback, Actions Ring (MX Master 4) | ❌ | The thumb panel is offered for re-assignment like other buttons; haptics and the Actions Ring are not implemented. |
| Keyboards (Fn lock, backlight, disable keys, key remapping) | ❌ | Fn inversion feature access exists, untested; no UI. |
| Dials, crowns, multi-rollers, haptics, force-sensing buttons | ❌ | |
| Presenters, touchpads | ❌ | |
| Webcams, headsets | ❌ | |

## Not planned

These Options+ features are left out on purpose:

| Feature | Why |
|---|---|
| 🚫 Firmware updates | A faulty update can brick a device; use Logi Options+ or Logitech's firmware tool. |
| 🚫 Logitech account, cloud backup and sync | Inputsmith works offline; use local backups and export instead. |
| 🚫 Analytics and telemetry | Inputsmith makes no network connections. It also switches off the per-click reporting Options+ enables on devices. |
| 🚫 Marketplace, offers, device recommendations | |
| 🚫 AI Prompt Builder, Logi Voice | Cloud services. |
| 🚫 Third-party plugin integrations (Adobe, Office, Logi Plugin Service) | Not planned for now. |

## Requirements and limitations

- **Do not run Logi Options+ at the same time** — both would configure the devices. Quit it (including
  its background agent) before starting Inputsmith.
- **Options+ is not needed.** Device pictures, names, defaults and the action list are built in. Devices
  without a built-in description still work, with names from firmware and a plain list instead of a
  picture. Options+ files are only read by the optional settings import.
