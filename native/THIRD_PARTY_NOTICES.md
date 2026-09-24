# Native application dependencies

ONIX sources remain MIT licensed. Dependency licenses apply independently.

- Slint 1.17.1: LicenseRef-Slint-Royalty-free-2.0 (desktop application).
  Before public release, the Slint attribution badge must be placed on the
  public GitHub download page under license condition 2(b). It is intentionally
  not displayed in the application's About window.
  https://slint.dev/terms-and-conditions
- Roboto: SIL Open Font License 1.1 — native/ui/fonts/OFL.txt
  https://fonts.google.com/specimen/Roboto
- nusb: MIT OR Apache-2.0 — https://github.com/kevinmehall/nusb
- Winit: Apache-2.0 — https://github.com/rust-windowing/winit
- FemtoVG: MIT — https://github.com/femtovg/femtovg
- Interprocess: MIT OR Apache-2.0 — https://github.com/kotauskas/interprocess
- windows-sys: MIT OR Apache-2.0 — https://github.com/microsoft/windows-rs
- wdi-rs: MIT OR Apache-2.0 — https://github.com/piersfinlayson/wdi-rs
- libwdi 1.5.1 (embedded in the Windows binary): LGPL-3.0-or-later — https://github.com/pbatard/libwdi
- ashpd (Linux): MIT — https://github.com/bilelmoussaoui/ashpd
- futures-lite and async-io (Linux): MIT OR Apache-2.0 — https://github.com/smol-rs
- objc2 framework bindings (macOS): Zlib OR Apache-2.0 OR MIT — https://github.com/madsmtm/objc2
- core-foundation (macOS): MIT OR Apache-2.0 — https://github.com/servo/core-foundation-rs

The packages include THIRD-PARTY-LICENSES.html, generated with cargo-about from
the locked dependency graph for all four target architectures, and the original
SLINT-LICENSE.md. The separate Slint file is intentional: cargo-about cannot
associate its custom LicenseRef with a standard SPDX license text.
Cargo.lock records the exact resolved dependencies. No .NET, Qt, Chromium, or
external libusb runtime is used by the native application.
