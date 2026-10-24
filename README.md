# keepsay

**Speak, and get clean text back, using your own Claude key, which is built to be very hard to steal and cheap to lose.**

keepsay is an open-source voice dictation tool for iOS, macOS, and Windows. Speech recognition runs on your device. When you want it, your own Anthropic API key turns the raw transcript into clean text or a translation, and keepsay inserts it where your cursor is.

> **Status: pre-alpha (milestone M0).** This is the project skeleton, the specs, and the CI. Nothing usable has shipped yet, so don't trust any build with a real key.

## What makes it different

Most dictation tools make you pick one of these:

- **Local-only.** Private, but no LLM cleanup.
- **Bring your own key.** Powerful, but the key usually sits in a plaintext config file, one leaked dotfile away from someone else's bill.

keepsay starts from the second option and makes key safety the main design goal:

| | keepsay |
|---|---|
| **Where the key lives** | OS secure storage only: Keychain set to *this device only, not synced*, or Windows Credential Manager with local persistence. It never touches a config file, the registry, logs, crash output, temp files, or the clipboard. |
| **Who holds it in memory** | Only the Rust core, as a zeroized `SecretString`, and by default only for the duration of one request. Swift and JavaScript see it once, when you type it in. No API returns it; the UI only ever shows `…a1B2`. |
| **Where it can go** | Exactly one destination: `https://api.anthropic.com`. The host is hardcoded and redirects are refused. Only one type in the codebase can attach the key, and tests enforce that. |
| **What the model can do** | Produce text, nothing else. The request type has no `tools` field at all, so prompt injection inside your own dictation can't make the model act. Output also passes a guard before it is inserted. |
| **How much a leak can cost** | You decide in advance. Onboarding walks you through a dedicated workspace with prepaid credit only, no payment method, and a spend limit, so the worst case has a fixed ceiling. |
| **What leaves your device** | Your polish request to Anthropic, and only if you use polish. Otherwise nothing: no telemetry, no analytics, no crash upload, no remote config, no auto-updater. Audio stays in memory and is never written to disk, and no history is kept. |

The same approach covers the rest of the stack: a dependency whitelist, `cargo-deny` and `cargo-vet` in CI, GitHub Actions pinned by commit SHA, and releases built only by CI with signatures and build provenance.

## Features (planned for v1)

- **One key to polish, one to translate.** Press a hotkey to start, speak, and press again. You get cleaned-up text, or cleaned-up text translated into your target language, in a single request.
  - macOS defaults: `fn` + `Z` to polish, `fn` + `Shift` + `Z` to polish and translate.
  - Windows defaults: `Right Alt` to polish, `Right Shift` + `Right Alt` to polish and translate.
  - Both defaults are being validated (see V8 and V9 in the spec), and every hotkey can be changed.
- **Local speech-to-text.** whisper.cpp runs on-device, with Metal on Apple Silicon. Models download only when you ask, and each file is checked against a SHA-256 hash pinned in the repo. You can also import model files by hand.
- **Modes.** raw (no API call, works offline), light (punctuation only), polish (the default), and formal. Any failure falls back to raw.
- **Tuned for Taiwanese Mandarin mixed with English,** with a custom glossary for names and jargon.
- **Budget tracking** from the API's own usage counts. A soft limit warns you, and a hard limit switches you to raw mode.
- **Platforms:**
  - macOS menu bar app (SwiftUI)
  - Windows tray app (Tauri 2). The key is entered in a native dialog and never reaches the WebView.
  - iOS app with App Intents. A keyboard extension comes later; it will never link the networking core.

## The honest limitation

The key lives on your device, so no app can fully protect it from malware running as your own user. keepsay guarantees that the key is never stored in plaintext, is held in memory as briefly as possible, and can only go to one place. Your account settings, not keepsay, set the ceiling on what a leak can cost. See [SECURITY.md](SECURITY.md).

## Documentation

- [Design proposal & spec](docs/specs/PROPOSAL.md) (Traditional Chinese): invariants, architecture, milestones
- [Threat model](docs/specs/THREAT_MODEL.md)
- [Change records](docs/changes/)
- [UI mockups](docs/mockups/): open `mobile.html` or `desktop.html` in a browser
- [Contributor / agent rules](CLAUDE.md)

## Building (developers)

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo deny check && cargo vet --locked
```

The Rust toolchain is pinned in `rust-toolchain.toml`. The Apple and Windows apps arrive in milestones M2 to M4.

## License

See [LICENSE](LICENSE). The final license choice is still open; see PROPOSAL §16.
