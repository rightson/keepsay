# apple/

SwiftUI apps for macOS (menu bar, M2) and iOS (M4a, keyboard extension M4b).
Nothing lives here yet. The planned layout is in `docs/specs/PROPOSAL.md` §5.2 and §7.1–7.2.

The rules that apply here:
- Use Apple system frameworks only. No third-party Swift packages in v1.
- Swift touches the key exactly once: it calls `store_key`, then clears the field (INV-2).
- The keyboard extension must not link keepsay-core, must contain no networking, and must have no Keychain access group shared with the app (INV-10).
