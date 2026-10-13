# Roadmap

Where Inputsmith is heading, in order. Each item corresponds to a row in the
[feature support matrix](FEATURES.md); when an item lands, tick it here and update its status there.
Technical background for finished items is in [Measured behaviour](BEHAVIOR.md).

Contributions are welcome — especially **testing with devices other than the MX Anywhere 3** (see
[devices](devices/README.md)) and Linux work.

## Milestone 1 — Windows, mice: replace Options+ for everyday use ✅

Verified on an MX Anywhere 3 (Unifying receiver), behaviour measured against Options+ 2.8.

- [x] Device discovery, receivers, hot-plug, power-off / sleep / wake handling, inactive devices
- [x] Re-apply settings after a device loses them (power-off, deep sleep), detected with the device's
      configuration cookie
- [x] Pointer speed (DPI curve identical to Options+)
- [x] SmartShift: on/off, sensitivity, scrolling force
- [x] Scroll direction, smooth scrolling, horizontal scrolling with side buttons
- [x] Swap left/right buttons
- [x] Button re-assignment, per-application profiles
- [x] Self-contained: own device descriptions, device art (from Piper) and action list; no Options+ needed
- [x] Gesture button with speed-dependent steps and action timing matching Options+
- [x] Per-model defaults
- [x] Settings window and tray icon in the style of Options+
- [x] Local settings, backups, export/import, import directly from Options+
- [x] Start at login, single instance, per-user installer (MSI), releases built by GitHub Actions
- [x] No telemetry; device-side click analytics switched off

## Milestone 2 — Complete the app on Windows

- [ ] Action editors in the GUI: keyboard shortcut recorder, application picker, file/folder and web page
      inputs, text block
- [ ] Full action catalog: the built-in list (`crates/data/actions.json`) has the common actions; add the
      rest of what Options+ offers
- [ ] Remaining system actions (brightness, sleep/shutdown with confirmation, more window actions)
- [ ] Verify the remaining gesture presets against Options+ (virtual desktops, window management, arrange
      windows; zoom/rotate and pan scaling)
- [ ] Verify end-to-end: per-application profile switching, DPI/SmartShift-toggle actions, media actions,
      reset to defaults
- [ ] Low-battery notifications
- [ ] On-screen displays (DPI change, Caps/Num lock, battery)
- [ ] Actions Ring (radial menu)
- [ ] Predefined application profiles (Chrome, Office, Adobe, …)
- [ ] UI polish: dark title bar, action icons, connector lines to hotspots, Options+ slot names, local time
- [ ] Long-running stability (days of uptime, receiver unplug/replug)

## Milestone 3 — Linux

- [ ] Device access through hidraw (implemented, untested) and a udev rule for non-root access
- [ ] Input injection through uinput (keys, buttons, wheel, media, text)
- [ ] Foreground application detection (X11; Wayland per compositor)
- [ ] Agent as a systemd user service; tray icon via StatusNotifierItem
- [ ] Packages (`.deb`, `.rpm`, possibly AppImage) built and released by the CI (`build.yml`)

## Milestone 4 — More devices

Needs hardware or testers; each newly tested model gets a page in [`devices/`](devices/README.md).

- [ ] Bolt receivers and direct Bluetooth connections (implemented, untested)
- [ ] Pairing new devices ("Add device")
- [x] Built-in descriptions for every HID++ 2.0 mouse and trackball Options+ lists (60), drawings for 22
- [ ] Verify the described mice on hardware (device reports)
- [ ] Drawings for the remaining mice (MX Master 4, LIFT, Signature, POP, Pebble, M575, …)
- [ ] Mice with a thumbwheel (MX Master series): thumbwheel settings and actions
- [ ] Extended DPI (0x2202) for high-resolution sensors (MX Master 3S / 4, MX Anywhere 3S)
- [ ] Windows pointer speed for mice without adjustable DPI
- [ ] Older HID++ 1.0 mice (register-based configuration)
- [ ] Keyboards: Fn lock, backlight, disable keys, key re-assignment
- [ ] Easy-Switch: switch hosts from the app
- [ ] Dials, crowns, multi-rollers, haptics, force-sensing buttons
- [ ] Presenters, touchpads
- [ ] Gaming features found in Options+ (report rate, onboard profiles)

## Milestone 5 — macOS

- [ ] Device access (IOHIDManager), input injection (CGEvent), accessibility permission, launch agent

## Later / maybe

- [ ] Smart Actions (triggered action sequences) and macro recording
- [ ] Flow (one mouse and keyboard across computers) — needs protocol research with two computers
- [ ] Webcams, headsets

## Not planned

Firmware updates, Logitech accounts and cloud sync, analytics, marketplace and offers, AI Prompt Builder,
Logi Voice — see [Not planned](FEATURES.md#not-planned).
