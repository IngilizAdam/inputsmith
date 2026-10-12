# Contributing to Inputsmith

Thanks for helping! Logitech makes hundreds of devices and Inputsmith's maintainers own only a few of
them, so reports and contributions from people with other hardware are what moves the project forward.

## Ways to help

| You want to… | Do this |
|---|---|
| Report something that does not work | Open a **Bug report** issue. |
| Ask for support for your device or a device feature | Open a **Device support request** issue and include the output of `inputsmith info`. |
| Tell us how Inputsmith works with your device | Open a **Device test report** issue — positive reports are just as useful. |
| Suggest a feature or an Options+ feature that is missing | Open a **Feature request** issue; check the [feature matrix](docs/FEATURES.md) and [roadmap](docs/ROADMAP.md) first. |
| Write code or documentation | Read on, then open a pull request. |

Before opening an issue, search existing ones — someone may already have reported the same device or
idea; add your details there instead.

### Device information

`inputsmith info` lists your receivers and devices, their HID++ features and their current state. It only
reads from the devices and is safe to run at any time, even while Logi Options+ is running. Its output
contains serial numbers; remove them if you prefer.

For device support, the most helpful additional information is what Options+ does with the device. If
you can, run `inputsmith sniff` while you change a setting in Options+ and attach the output (see
[Observing Options+](docs/DEVELOPMENT.md#observing-options)).

## Contributing code

1. Read [Development](docs/DEVELOPMENT.md) — especially the **principles**.
2. For anything bigger than a small fix, open an issue first so we can agree on the approach.
3. Keep pull requests focused; one feature or fix per pull request.
4. Run `cargo fmt --all` and `cargo test --workspace`; new behaviour should come with tests.
5. Update the [feature matrix](docs/FEATURES.md), [roadmap](docs/ROADMAP.md) and, for device behaviour,
   [measured behaviour](docs/BEHAVIOR.md) or the [device notes](docs/devices/README.md).

### Original work only

Inputsmith must stay free of Logitech's intellectual property:

- **Do not contribute** Logitech code, decompiled or disassembled code, files or data extracted from
  Logitech software or firmware, or Logitech artwork.
- **Do** describe and implement behaviour you observed from the outside: what a device sends and
  receives, what input Options+ produces, what files it writes to disk, and public HID++ knowledge.
- How you investigate is up to you, but only your own original code and observations belong in a pull
  request. If in doubt, ask in an issue first.

### Licensing of contributions

Inputsmith is licensed under the [GNU General Public License v3.0 or later](LICENSE). By submitting a
contribution you agree that it is licensed under the same terms and that you have the right to submit it.

## Conduct

Be kind and constructive. Everyone here is volunteering their time.
