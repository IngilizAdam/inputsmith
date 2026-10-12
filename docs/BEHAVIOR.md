# Measured behaviour

How Inputsmith drives devices and why. Where Logi Options+ behaviour matters, it was measured from the
outside — the HID++ traffic between Options+ and the device (`inputsmith sniff`), the device state
(`inputsmith info`) and the input Options+ injects into the system — and Inputsmith reproduces the result.
Model-specific observations are in [`devices/`](devices/README.md).

Unless stated otherwise, values were measured with an MX Anywhere 3 on a Unifying receiver, Options+ 2.8,
Windows 11.

## Device identity

- **Model id** = transport bits (hex) followed by the first product id, from HID++ feature 0x0003
  (device information); transport bits: Bluetooth 1, BLE 2, eQuad 4, USB 8. MX Anywhere 3: transport 0x6,
  ids `b025, 4090` → `6b025`. Colour variants report an extended model id (`6b025_ext1`, …).
- **Slot ids** name what an assignment is for: `<prefix>_c<control id>` for buttons,
  `<prefix>_mouse_settings` and `<prefix>_mouse_scroll_wheel_settings` for settings. The prefix is
  `<product-name>-<model id>` (e.g. `mx-anywhere-3-6b025`) for about half of all models; when an Options+
  installation is present, Inputsmith reads the exact prefix from its device database.

## Pointer speed

Options+ stores pointer speed as a value v from −1 to 1 (UI % = (v + 1) · 50) and applies it as sensor DPI
(it does not change the Windows pointer speed). The curve is geometric through the device's minimum,
default and maximum DPI, rounded to the device's DPI step:

- v < 0: `default · (min / default)^(−v)`
- v ≥ 0: `default · (max / default)^v`

| UI % | 0 | 5 | 12 | 15 | 25 | 50 | 75 | 100 |
|---|---|---|---|---|---|---|---|---|
| DPI (MX Anywhere 3: 200–4000, default 1000, step 50) | 200 | 250 | 300 | 300 | 450 | 1000 | 2000 | 4000 |

Without a pointer speed setting the device default DPI is used.

## SmartShift

- Sensitivity s (0–100) → auto-disengage threshold `round(50 − 0.46 · s)` (0 → 50, 50 → 27, 70 → 18,
  100 → 4). SmartShift off = threshold 255 (never switch to free spin).
- Scrolling force 1–100 is written as the torque value unchanged.

## Scroll wheel

The hi-res wheel feature (0x2121) mode byte: bit 0 = report to software, bit 1 = high resolution,
bit 2 = invert. High resolution is never enabled.

| Smooth scrolling | Direction | Wheel mode | Who scrolls |
|---|---|---|---|
| off | standard | `0x00` | the device (120 per notch) |
| off | inverted | `0x04` | the device, inverted |
| on | standard / inverted | `0x01` | software: one wheel event of 120 × delta per wheel report, negated when inverted |

With smooth scrolling on, Options+ injects exactly one wheel event per report (no interpolation), and so
does Inputsmith.

### Horizontal scrolling with a side button

The button is taken over (diverted) with the "raw wheel" reporting flag of the special-keys feature
(0x1B04). While it is held, wheel turns arrive as raw-wheel events instead of normal scrolling, and
Inputsmith injects one horizontal wheel event per report: standard direction −120 × delta (wheel down
scrolls right), inverted +120 × delta. Pressing and releasing the button without turning the wheel runs
the button's own action at release.

## Swap left/right buttons

A system setting (on Windows `SPI_SETMOUSEBUTTONSWAP`, persisted), applying to every mouse. Nothing is
sent to the device.

## Gestures

Gesture presets are defined by parameters per axis; the values used by Options+:

| Preset | Horizontal | Vertical |
|---|---|---|
| App navigation | repeats, 100 ms action duration | fires once |
| Media controls | repeats, 400 ms action duration | repeats, speed factor 0.5 |
| Window management, Arrange windows, Virtual desktops | repeats, 400 ms action duration | fires once |
| Custom gesture | repeats, 200 ms | repeats, 200 ms |
| Zoom / rotate, Pan | built-in behaviours | |

The engine (`crates/agent/src/gesture.rs`) reproduces Options+ on eight recorded gestures (same actions
in the same order; timing within 100 ms, fast gestures within ~20 ms). The recordings are replayed in its
tests (`crates/agent/src/testdata/gesture_traces.json`). Distances are raw sensor counts:

- A gesture starts after 40 counts of movement (the axis action, e.g. Alt for app switching, is pressed).
- An action fires after a speed-dependent distance: `clamp(118 / √v, 70, 190 × speed factor)` at speed v
  (counts per ms over the last 64 ms) — about 200 counts for slow movement, 70 for fast flicks.
- After an action, no action fires for the axis' action duration, and movement during that time is
  discarded. Without a duration the remaining distance carries over.
- Moving back fires the opposite action after 150 counts.
- An axis without an action duration or speed factor fires once per gesture.

## Device lifecycle

- Devices lose most of their configuration when switched off and in **deep sleep** (on the MX Anywhere 3
  after ~10 minutes idle, when the receiver reports the link as lost). Inputsmith re-applies everything
  when the device comes back.
- To avoid needless writes, Inputsmith stores a configuration cookie in the device (HID++ feature 0x0020)
  after configuring it; the device resets it to 0 when it loses its configuration. If the cookie is still
  there — and every taken-over button still reads back as taken over — reconfiguration is skipped.
- On reconnect, Options+ was observed to read device information, read DPI (1000 after power loss),
  set its own cookie, reset and re-apply all button reporting flags, the wheel mode and the DPI, and to only
  read SmartShift (it skips writes that would not change anything).
- Options+ enables "analytics" key events on every control, including left and right click: the device
  then reports every press to the host. Inputsmith switches this off on every control that supports it.
- While a device is off, Options+ shows it as INACTIVE with its settings still editable; Inputsmith does
  the same.

## Options+ settings import

Options+ keeps its settings in `%LOCALAPPDATA%\LogiOptionsPlus\settings.db`, an SQLite database whose
`data` table holds JSON documents; the newest row is the current settings (recent changes may still be in
the `-wal` file). Profiles are stored under `profile_keys` / `profile-<id>` and applications under
`applications.applications`. Inputsmith reads this with its own small SQLite reader and keeps the same
JSON layout for its settings, so imported settings carry over unchanged.
