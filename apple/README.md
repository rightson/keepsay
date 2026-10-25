# apple/

SwiftUI apps for macOS (menu bar, M2) and iOS (M4a; keyboard extension in M4b). The design is in `docs/specs/PROPOSAL.md` §7.1–7.2 and `docs/changes/0015-macos-app.md`.

| Path | What |
|------|------|
| `KeepsayKit/` | SwiftPM library shared by macOS and iOS: core bridge, in-memory recorder, settings model, dictation state machine |
| `KeepsayMac/` | SwiftPM package for the menu bar app (`KeepsayMac`) and its testable system layer (`KeepsayMacSupport`) |
| `scripts/build-core.sh` | Builds keepsay-core into `KeepsayCoreFFI.xcframework` (macOS, iOS, iOS Simulator) and generates the UniFFI Swift bindings |
| `scripts/build-mac-app.sh` | Builds `Keepsay.app` with Hardened Runtime (ad-hoc signed by default) |
| `scripts/check-fn-tap.sh` | THREAT_MODEL T7 scan of the fn+Z event tap |

```sh
apple/scripts/build-core.sh          # optional: link the Rust core
swift test --package-path apple/KeepsayKit
swift test --package-path apple/KeepsayMac
apple/scripts/build-mac-app.sh       # -> apple/build/Keepsay.app
```

The rules that apply here:
- Use Apple system frameworks only. No third-party Swift packages in v1.
- Swift touches the key exactly once: it calls `store_key`, then clears the field (INV-2).
- `KeepsayMacSupport/FnKeyTap.swift` is the only global key hook. It reads only the Z keycode and the fn and Shift flags, and it stores and logs nothing (T7).
- The keyboard extension must not link keepsay-core, must contain no networking, and must have no Keychain access group shared with the app (INV-10).
