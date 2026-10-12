# MX Anywhere 3 (`6b025`)

Tested: 2026-10-12, main application firmware MPM24.01_B0015 (bootloader BL113.01_B0015), via
Unifying receiver C52B (firmware RQR24.11_B0036), Windows 11 VM, Options+ 2.8.981479.
Wireless PID 0x4090 (Unifying), Bluetooth PID 0xB025; transport bits 0x0006 (BTLE + eQuad).

## HID++ features (HID++ 4.5)

Public: 0x0000 Root, 0x0001 FeatureSet, 0x0003 DeviceInformation v4, 0x0005 DeviceNameType,
0x1D4B WirelessStatus, 0x0020 ConfigChange, 0x0021 CryptoIdentifier, 0x0007 DeviceFriendlyName,
0x1004 UnifiedBattery, 0x1B04 SpecialKeys v5, 0x1814 ChangeHost (3 hosts), 0x2250 AnalysisMode,
0x2201 AdjustableDPI, 0x2111 SmartShiftWithTunableTorque, 0x2121 HiResWheel, 0x2251 (unknown; not used by
Options+), 0x00C2 DFUControl. Plus hidden/engineering features (0x18xx, 0x1E00, 0x9001, …).

- DPI: range 200–4000, step 50, default 1000.
- SmartShift capabilities: tunable torque, default auto-disengage 10, default torque 60, max force 14.
- Hi-res wheel: multiplier 15, has invert, has ratchet switch.

## Persistence across power cycle

Measured with no host software running: values written, mouse switched off for 5 s and on, then read with
`inputsmith info`.

| State | Persists? | Evidence |
|---|---|---|
| SmartShift auto-disengage threshold (0x2111) | **Yes** | Wrote 30 (non-default; default 10) → still 30 after power cycle |
| SmartShift torque / scrolling force (0x2111) | **Yes** | Wrote 50 (default 60) → still 50 |
| SmartShift mode (ratchet/free-spin) | Probably (stayed Ratchet; not tested with Free-spin) | — |
| DPI (0x2201) | **No** | 450 → 1000 (default) after power cycle, both tests |
| Control diversion (0x1B04), e.g. gesture button | **No** | Gesture button inactive after power cycle until host software re-diverts it |

## Controls (0x1B04)

Controls 0x53/0x56 have flags 0x0d31: bit 0x0800 is presumably "raw wheel capable" (only these two
controls have it, and only they are offered horizontal scrolling).

| CID | Slot name (Options+) | Reprogrammable | Divertable | Raw XY | Notes |
|---|---|---|---|---|---|
| 0x50 | Left button | no | no | no | |
| 0x51 | Right button | no | no | no | |
| 0x52 | Middle button | yes | yes | yes | |
| 0x53 | Back button | yes | yes | yes | Options+ diverts it even for the default "Back" action |
| 0x56 | Forward button | yes | yes | yes | Options+ diverts it even for the default "Forward" action |
| 0xC4 | Mode shift / top button | yes | yes | yes | Options+ default: gesture button (app navigation) |
| 0xD7 | Virtual gesture button | no | yes | yes | virtual control |

## Options+ behaviour observed

- On connect, Options+ reads SmartShift capabilities and mode repeatedly and **skips the write when the
  device already matches** (no 0x2111 writes seen in a sniff session).
- Pointer speed is applied as sensor DPI (not Windows mouse speed).
- Gesture behaviour: see [Measured behaviour](../BEHAVIOR.md#gestures).
- Defaults (`devices.json` `mouseScrollWheelOverride`): standard direction, smooth off, SmartShift on,
  sensitivity 83, scroll force 50, **horizontal scrolling enabled on Back and Forward** (hold + wheel).
- Smooth scrolling diverts the wheel (`setWheelMode 0x01`); smooth off uses the native wheel
  (`0x00`, or `0x04` for inverted). Hi-res mode is never used. Details: [Measured behaviour](../BEHAVIOR.md#scroll-wheel).
- Horizontal scrolling on a side button: button diverted with the 0x1B04 *raw wheel* flag; wheel turns
  while held arrive as 0x1B04 event 4.
- Switched off → shown as INACTIVE (`LINK_LOSS`), settings stay editable.
- Reconnect after power loss: DPI read back as 1000, ConfigChange cookie 0 → Options+ sets cookie
  `0x4825`, resets and re-applies all diversions, wheel mode and DPI ([Measured behaviour](../BEHAVIOR.md#device-lifecycle)).

## Quirks

- Light sleep keeps the receiver link: after 9 minutes idle the mouse still answered HID++ and kept its
  configuration.
- **Deep sleep after ~10 minutes idle** (measured 609 s): the receiver reports link loss, HID++ requests go
  unanswered, and on wake (link-up 0x41) the configuration is gone like after a power cut (DPI 1000, cookie 0,
  no diversions). Options+ shows the device as INACTIVE meanwhile and reconfigures everything on wake.
