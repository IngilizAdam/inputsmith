# Devices

## Supported mice and trackballs

Every mouse and trackball that Logi Options+ 2.8 knows about. Each one that speaks HID++ 2.0 has a
built-in description (name, settings key, default scroll settings, buttons, default bindings) in
[`crates/data/devices`](../../crates/data/README.md); the values match Options+.

| Mark | Meaning |
|---|---|
| ✅ Tested | Verified on real hardware; see the notes page. |
| 🔬 Untested | Everything below is implemented, but nobody has tried it on this model yet — [reports welcome](#reporting-a-device). Features listed as *not implemented* in the notes are missing. |
| ❌ Not implemented | Needs support that Inputsmith does not have yet. |

What every 🔬 model gets, where the model has the hardware: pointer speed (through adjustable DPI,
extended DPI with the high-resolution sensor mode, or the mouse's own speed scaling), scroll wheel settings
and SmartShift, thumb wheel direction, speed and smooth scrolling, haptic feedback on/off and strength,
button re-assignment, gestures on gesture buttons (the default binding, as in Options+), and horizontal
scrolling with the side buttons.
*Drawing* means the Buttons and Point & Scroll pages show a picture with hotspots; *button list* means
they show a list instead. Only Unifying connections are tested so far (see the
[feature matrix](../FEATURES.md#connections)).

| Device | Model id | Connections | Status | Notes |
|---|---|---|---|---|
| Ergo M575 (trackball) | `6b027` | Unifying, Bluetooth | 🔬 Untested | button list (no drawing yet) |
| Ergo M575 for Business | `2b02f` | Bolt, Bluetooth | 🔬 Untested | button list (no drawing yet) |
| ERGO M575S Trackball | `2b041` | Bolt, Bluetooth | 🔬 Untested | button list (no drawing yet) |
| LIFT | `2b031` | Bolt, Bluetooth | 🔬 Untested | button list (no drawing yet) |
| LIFT for Business | `2b033` | Bolt, Bluetooth | 🔬 Untested | button list (no drawing yet) |
| M240 Silent | `2b03a` | Bolt, Bluetooth | 🔬 Untested | button list (no drawing yet) |
| M240 Silent for Business | `2b03b` | Bolt, Bluetooth | 🔬 Untested | button list (no drawing yet) |
| M275/M280/M320/M330/M331 | `44057` | Unifying | 🔬 Untested | button list (no drawing yet) |
| M275/M280/M320/M330/M331 | `4040` | Unifying | 🔬 Untested | button list (no drawing yet) |
| M275/M280/M320/M330/M331 | `44058` | Unifying | 🔬 Untested | button list (no drawing yet) |
| M325 | `44094` | Unifying | 🔬 Untested | button list (no drawing yet) |
| M335 | `44050` | Unifying | 🔬 Untested | button list (no drawing yet) |
| M336 / M337 / M535 | `1b016` | Bluetooth | 🔬 Untested | button list (no drawing yet) |
| M336/M337/M535 | `1b014` | Bluetooth | 🔬 Untested | button list (no drawing yet) |
| M500s | `8c093` | USB cable | 🔬 Untested | drawing |
| M510 | `44051` | Unifying | 🔬 Untested | button list (no drawing yet) |
| M545/M546 | `44052` | Unifying | 🔬 Untested | button list (no drawing yet) |
| M570 (trackball) | `41028` | Unifying | ❌ Not implemented | older HID++ 1.0 device: needs register-based support |
| M585/M590 | `6b01b` | Unifying, Bluetooth | 🔬 Untested | button list (no drawing yet) |
| M705 | `4101b` | Unifying | ❌ Not implemented | older HID++ 1.0 device: needs register-based support |
| M705 | `4406d` | Unifying | 🔬 Untested | button list (no drawing yet) |
| M720 TRIATHLON | `6b015` | Unifying, Bluetooth | 🔬 Untested | drawing |
| Mobi Fold | `2b045` | Bolt, Bluetooth | 🔬 Untested | button list (no drawing yet) |
| Mobi Fold for Business | `2b047` | Bolt, Bluetooth | 🔬 Untested | button list (no drawing yet) |
| MX Anywhere | `41017` | Unifying | ❌ Not implemented | older HID++ 1.0 device: needs register-based support |
| MX Anywhere 2 | `6b013` | Unifying, Bluetooth | 🔬 Untested | drawing |
| MX Anywhere 2 | `6b018` | Unifying, Bluetooth | 🔬 Untested | drawing |
| MX Anywhere 2 | `6b01f` | Unifying, Bluetooth | 🔬 Untested | drawing |
| MX Anywhere 2S | `6b01a` | Unifying, Bluetooth | 🔬 Untested | drawing |
| MX Anywhere 3 | `6b025` | Unifying, Bluetooth | ✅ Tested | [notes](mx-anywhere-3.md); drawing |
| MX Anywhere 3 for Business | `2b02d` | Bolt, Bluetooth | 🔬 Untested | drawing |
| MX Anywhere 3S | `2b037` | Bolt, Bluetooth | 🔬 Untested | drawing |
| MX Anywhere 3S for Business | `2b038` | Bolt, Bluetooth | 🔬 Untested | drawing |
| MX Ergo (trackball) | `6b01d` | Unifying, Bluetooth | 🔬 Untested | drawing |
| MX Ergo S (trackball) | `2b03e` | Bolt, Bluetooth | 🔬 Untested | button list (no drawing yet) |
| MX Master | `6b012` | Unifying, Bluetooth | 🔬 Untested | drawing; thumb wheel actions other than horizontal scrolling not implemented |
| MX Master | `6b01e` | Unifying, Bluetooth | 🔬 Untested | drawing; thumb wheel actions other than horizontal scrolling not implemented |
| MX Master | `6b017` | Unifying, Bluetooth | 🔬 Untested | drawing; thumb wheel actions other than horizontal scrolling not implemented |
| MX Master 2S | `6b019` | Unifying, Bluetooth | 🔬 Untested | drawing; thumb wheel actions other than horizontal scrolling not implemented |
| MX Master 3 | `6b023` | Unifying, Bluetooth | 🔬 Untested | drawing; thumb wheel actions other than horizontal scrolling not implemented |
| MX Master 3 B | `2b028` | Bolt, Bluetooth | 🔬 Untested | drawing; thumb wheel actions other than horizontal scrolling not implemented |
| MX Master 3S | `2b034` | Bolt, Bluetooth | 🔬 Untested | drawing; thumb wheel actions other than horizontal scrolling not implemented |
| MX Master 3S | `2b043` | Bolt, Bluetooth | 🔬 Untested | drawing; thumb wheel actions other than horizontal scrolling not implemented |
| MX Master 3s for Business | `2b035` | Bolt, Bluetooth | 🔬 Untested | drawing; thumb wheel actions other than horizontal scrolling not implemented |
| MX Master 3s for Business | `2b044` | Bolt, Bluetooth | 🔬 Untested | drawing; thumb wheel actions other than horizontal scrolling not implemented |
| MX Master 4 | `2b042` | Bolt, Bluetooth | 🔬 Untested | button list (no drawing yet); thumb wheel actions other than horizontal scrolling not implemented; haptic feedback on events and the Actions Ring not implemented |
| MX Master 4 for Business | `2b048` | Bolt, Bluetooth | 🔬 Untested | button list (no drawing yet); thumb wheel actions other than horizontal scrolling not implemented; haptic feedback on events and the Actions Ring not implemented |
| MX Vertical | `eb020` | Unifying, Bluetooth, USB cable | 🔬 Untested | drawing |
| Pebble Mouse 2 M350s | `2b036` | Bolt, Bluetooth | 🔬 Untested | button list (no drawing yet) |
| Pebble Up | `2b04c` | Bolt, Bluetooth | 🔬 Untested | button list (no drawing yet) |
| Performance MX | `4101a` | Unifying | ❌ Not implemented | older HID++ 1.0 device: needs register-based support |
| POP Mouse | `2b030` | Bolt, Bluetooth | 🔬 Untested | button list (no drawing yet) |
| Signature AI Edition M750 | `2b040` | Bolt, Bluetooth | 🔬 Untested | button list (no drawing yet) |
| Signature Comfort Plus M850 L | `2b049` | Bolt, Bluetooth | 🔬 Untested | button list (no drawing yet) |
| Signature Comfort Plus M850 L for Business | `2b04b` | Bolt, Bluetooth | 🔬 Untested | button list (no drawing yet) |
| Signature M550 L | `2b02b` | Bolt, Bluetooth | 🔬 Untested | button list (no drawing yet) |
| Signature M650 For Business | `2b032` | Bolt, Bluetooth | 🔬 Untested | button list (no drawing yet) |
| Signature M650 L | `2b02a` | Bolt, Bluetooth | 🔬 Untested | button list (no drawing yet) |
| Signature Plus M750 L | `2b02c` | Bolt, Bluetooth | 🔬 Untested | button list (no drawing yet) |
| Signature Plus M750 L For Business | `2b03d` | Bolt, Bluetooth | 🔬 Untested | button list (no drawing yet) |
| Signature Wired M520 L for Business | `8c0a2` | USB cable | 🔬 Untested | button list (no drawing yet) |
| Wireless Mouse (M185) | `4091` | Unifying | 🔬 Untested | button list (no drawing yet) |
| Wireless Mouse (M187) | `4054` | Unifying | 🔬 Untested | button list (no drawing yet) |
| Wireless Mouse (M310) | `44055` | Unifying | 🔬 Untested | button list (no drawing yet) |

Keyboards (69 models in Options+), presenters, touchpads, dials and webcams are not supported yet.
Logitech mice that Options+ does not list but that speak HID++ 2.0 should still get the common features,
with their firmware name and a button list.

## Adding a device

A device gets its picture, name, defaults and hotspots from a file in
[`crates/data/devices`](../../crates/data/README.md) plus a drawing in `crates/data/art`. Add a notes file
here as well once the device has been tested.

## Reporting a device

Tried Inputsmith with another device? Please open an issue with:

1. The output of `inputsmith info` (run it with the device connected; it only reads).
2. What works and what does not.
3. Ideally, the power-cycle persistence test from the template below.

`inputsmith info` prints serial numbers; remove them if you prefer.

## Rules for device notes

One file per model with behaviour **observed on that model**. Do not generalize from one model to
another: firmware differs per model (and possibly per firmware version), so each entry records the exact
model id and firmware it was verified on.

The agent must not depend on these properties — it re-applies every setting whenever a device comes
back. They matter for understanding what Options+ does, for debugging, and for deciding whether a write
is needed.

## Template

```markdown
# <Name> (`<model id>`)

Tested: <date>, firmware <main application version>, connection <receiver/Bluetooth>, host <OS>.

## HID++ features
<output of `inputsmith info` feature table, or a summary>

## Persistence across power cycle
| State | Persists? | Evidence |

## Controls
| CID | Name | Flags (reprogrammable/divertable/raw XY) | Options+ default action |

## Options+ behaviour observed
<what Options+ reads/writes on connect, observed with `inputsmith sniff`>

## Quirks
```
