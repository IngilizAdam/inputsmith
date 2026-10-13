# Devices

## Tested devices

| Device | Model id | Connection | Status | Notes |
|---|---|---|---|---|
| MX Anywhere 3 | `6b025` | Unifying receiver | ✅ Fully supported | [mx-anywhere-3.md](mx-anywhere-3.md) |

Other Logitech mice that speak HID++ 2.0 (most devices supported by Options+) should already get the
common features — pointer speed, SmartShift, button re-assignment, gestures — but have not been tested.
Without a built-in description they are shown with their firmware name and a plain list of buttons
instead of a picture. Keyboards and other device types are not supported yet; see the
[feature matrix](../FEATURES.md).

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
