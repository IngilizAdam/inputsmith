# Built-in data

Everything the app shows about devices and actions lives here and is compiled into the programs; nothing
is read from a Logi Options+ installation.

| Path | Contents |
|---|---|
| `devices/*.json` | One file per device model: names, slot prefix, default scroll settings, art and hotspots. |
| `art/*.svg` | Device drawings (top view). |
| `actions.json` | The action catalog offered in the button picker. |
| `controls.json` | Per control id: the actions recommended first and the action tags allowed. |
| `strings.json` | English UI text for the keys used above (`SLOT_NAME_…`, `ASSIGNMENT_NAME_…`). |

New files in `devices/` and `art/` are picked up automatically when building. `cargo test -p
inputsmith-data` checks that every file parses, that referenced art exists and that every key has text.

## Matching Options+

Settings use the same layout as Options+ (so they can be imported), which makes some values identifiers
rather than choices. Take these from what Options+ uses for the device, unchanged:

- `modelId` and `slotPrefix` — settings slots are named `<slotPrefix>_c<control id>`; a different prefix
  would orphan imported settings.
- `scrollDefaults` — the scroll settings Options+ starts with for the model.
- `controls` — the control ids Options+ lists as programmable for the model.
- `thumbWheelDefaults` — the thumb wheel settings Options+ starts with.
- `highResolutionSensor` — the DPI ranges (normal and high-resolution mode) and whether the mode starts
  on, for models with a high-resolution sensor.
- Card ids, action definitions and tags in `actions.json`, and the lists in `controls.json`.

Names and all UI text are written for Inputsmith. Do not copy Logitech text files, images or other files
into this directory.

## Device files

```json
{
  "modelId": "6b025",
  "displayName": "MX Anywhere 3",
  "extendedDisplayNames": { "4": "MX Anywhere 3 for Mac" },
  "slotPrefix": "mx-anywhere-3-6b025",
  "type": "MOUSE",
  "scrollDefaults": { "dir": "STANDARD", "isSmooth": false, "smartshift": { ... }, ... },
  "controls": [82, 83, 86, 196],
  "art": "mx-anywhere-3.svg",
  "buttons": [
    { "slot": "c82", "name": "SLOT_NAME_MIDDLE_BUTTON", "x": 50.0, "y": 19.7, "dx": 1, "dy": 0 }
  ],
  "pointScroll": [
    { "slot": "mouse_scroll_wheel_settings", "name": "SLOT_NAME_SCROLL_WHEEL_SETTINGS", "x": 50.0, "y": 19.7, "dx": 1 }
  ]
}
```

- Files are named after the slot prefix. The descriptions of the mice Options+ 2.8 lists were written
  from the facts above; only the MX Anywhere 3 has been tested (see docs/devices/README.md).
- `extendedDisplayNames`: names of variants by extended model id (reported by the device).
- `defaults`: card per control id for controls whose default is not their own function. The gesture
  button (195) does nothing by itself and defaults to *Gestures*, as in Options+; every other control
  keeps its own function (middle click, back, forward, tilt, SmartShift toggle, …) until re-assigned.
- A drawing must have a hotspot for every control in `controls` (checked by the tests), otherwise the
  device gets the button list.
- `buttons` / `pointScroll`: markers on the drawing for the *Buttons* and *Point & scroll* pages. `x`/`y`
  are percent of the drawing's `viewBox`; `dx`/`dy` say which side the label goes (-1, 0, 1).
- Without `art` and hotspots the pages show a plain list of the device's buttons instead.

## Art

Drawings are flat, light grey top views, cropped to the device, with no labels or leader lines.

| File | Source | License |
|---|---|---|
| `art/mx-anywhere-3.svg` | MX Anywhere 3 / 3S (and for Business) — [Piper](https://github.com/libratbag/piper) `data/svgs/logitech-mx-anywhere3.svg` by the Piper contributors (erdincay, christiangnrd); device layer only, cropped | GPL-2.0-or-later |
| `art/mx-anywhere-2.svg` | MX Anywhere 2 / 2S — [Piper](https://github.com/libratbag/piper) `data/svgs/logitech-mx-anywhere2.svg` by the Piper contributors (christiangnrd, whot); device layer only, cropped | GPL-2.0-or-later |
| `art/mx-master-2s.svg` | MX Master, MX Master 2S — [Piper](https://github.com/libratbag/piper) `data/svgs/logitech-mx-master-2s.svg` by the Piper contributors (whot); device layer only, cropped | GPL-2.0-or-later |
| `art/mx-master-3.svg` | MX Master 3 / 3S (and for Business) — [Piper](https://github.com/libratbag/piper) `data/svgs/logitech-mx-master-3.svg` by the Piper contributors (emericg, aymanbagabas); device layer only, cropped | GPL-2.0-or-later |
| `art/mx-ergo.svg` | MX Ergo — [Piper](https://github.com/libratbag/piper) `data/svgs/logitech-mx-ergo.svg` by the Piper contributors (xoascf); device layer only, cropped | GPL-2.0-or-later |
| `art/mx-vertical.svg` | MX Vertical — [Piper](https://github.com/libratbag/piper) `data/svgs/logitech-mx-vertical.svg` by the Piper contributors (Doomsdayrs, sketchbuch); device layer only, cropped | GPL-2.0-or-later |
| `art/m720.svg` | M720 Triathlon — [Piper](https://github.com/libratbag/piper) `data/svgs/logitech-m720.svg` by the Piper contributors (koefmatt); device layer only, cropped | GPL-2.0-or-later |
| `art/m500s.svg` | M500s — [Piper](https://github.com/libratbag/piper) `data/svgs/logitech-m500s.svg` by the Piper contributors (FoxVK); device layer only, cropped | GPL-2.0-or-later |

Piper has drawings for many more Logitech mice (`data/svgs/`); when adding one, keep only the `Device`
layer, crop the `viewBox` to the device, keep the attribution comment at the top of the file and add a row
above. New drawings for devices Piper does not have are welcome here and upstream.
