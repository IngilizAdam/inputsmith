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
  "art": "mx-anywhere-3.svg",
  "buttons": [
    { "slot": "c82", "name": "SLOT_NAME_MIDDLE_BUTTON", "x": 50.0, "y": 19.7, "dx": 1, "dy": 0 }
  ],
  "pointScroll": [
    { "slot": "mouse_scroll_wheel_settings", "name": "SLOT_NAME_SCROLL_WHEEL_SETTINGS", "x": 50.0, "y": 19.7, "dx": 1 }
  ]
}
```

- `extendedDisplayNames`: names of variants by extended model id (reported by the device).
- `buttons` / `pointScroll`: markers on the drawing for the *Buttons* and *Point & scroll* pages. `x`/`y`
  are percent of the drawing's `viewBox`; `dx`/`dy` say which side the label goes (-1, 0, 1).
- Without `art` and hotspots the pages show a plain list of the device's buttons instead.

## Art

Drawings are flat, light grey top views, cropped to the device, with no labels or leader lines.

| File | Source | License |
|---|---|---|
| `art/mx-anywhere-3.svg` | [Piper](https://github.com/libratbag/piper) `data/svgs/logitech-mx-anywhere3.svg` by the Piper contributors; device layer only, cropped | GPL-2.0-or-later |

Piper has drawings for many more Logitech mice (`data/svgs/`); when adding one, keep only the `Device`
layer, crop the `viewBox` to the device, keep the attribution comment at the top of the file and add a row
above. New drawings for devices Piper does not have are welcome here and upstream.
