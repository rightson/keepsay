# keepsay

Voice dictation with local STT and optional Claude polish/translate using the user's own API key. The product is the security model: the key must be very hard to steal, and a leak must cost a bounded amount.

- Roadmap and progress: `docs/specs/ROADMAP.md` (user stories US-M<n>-NN, status, open decisions)
- Spec: @docs/specs/PROPOSAL.md (INV-* in §3, milestones in §13, dependency whitelist in §14, open questions in §15, human-only tasks in §16)
- Threat model: @docs/specs/THREAT_MODEL.md
- Decision log: `docs/changes/NNNN-slug.md` (format in docs/changes/README.md)
- UX mockups: `docs/mockups/*.html`

## Commands

- Build: `cargo build --workspace`
- Test: `cargo test --workspace` and `cargo test --workspace --all-features` (wiremock tests need `test-endpoint`)
- One test: `cargo test -p keepsay-core --test egress`
- Lint: `cargo fmt --all` then `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- Supply chain: `cargo deny check` and `cargo vet --locked`
- Before you say you are done, run all of the above. CI (`.github/workflows/ci.yml`) runs the same checks plus a release-feature guard.

## Hard rules

- IMPORTANT: YOU MUST NOT weaken any INV-* in PROPOSAL §3. If a change would weaken one, stop and ask, even when it seems harmless.
- IMPORTANT: YOU MUST NOT add a dependency, or a feature on an existing one, that is not on the §14 whitelist. Ask first, with your reason and the alternatives. `cargo add` is denied in `.claude/settings.json` on purpose.
- IMPORTANT: NEVER add a network request. The only egress points are `AnthropicClient` → `https://api.anthropic.com` and `ModelDownloader` → URLs in `models/manifest.toml` (§8).
- NEVER print, log, or assert on a key, transcript, model output, or glossary. That covers tests, error messages, `Debug` impls, and comments. Log lengths and timings only.
- NEVER add a function or IPC command that returns the key. Only `vault::fingerprint` (`…a1B2`) may leave the core.
- NEVER add `tools`, `tool_choice`, or similar fields to `api::types`.
- NEVER run `cargo vet certify`, `cargo vet trust`, or add exemptions to make vet pass. Auditing is a human decision.

## Workflow

0. Pick work from docs/specs/ROADMAP.md; update story status in the same PR.
1. Explore first. Read the relevant spec section and the existing module before you write anything.
2. Plan. For each milestone or multi-file change, write out the plan before you code.
3. Implement in small commits. Keep the module layout from PROPOSAL §5.2.
4. Verify. Run the commands above, then check the milestone's DoD (§13) item by item.
5. Record. Write a `docs/changes/NNNN-*.md` for any non-trivial decision. If it changes the spec, edit PROPOSAL in the same commit and mark the edit `（vX.Y 變更，見 docs/changes/NNNN-…）`. Write spike conclusions into §15.
6. If something needs a human (accounts, certificates, signing, store submissions, trademark checks), add it to §16 and keep going. Do not stop to ask mid-task.

## Code style

- Rust 2024 edition. The toolchain is pinned in `rust-toolchain.toml`. `unsafe_code` is forbidden workspace-wide; ask before relaxing it, for example for Win32 FFI in vault.
- Errors use `CoreError` (thiserror). Variants never carry secret or user content.
- Secrets are `secrecy::SecretString`. Call `expose_secret()` only at the point of use.
- Security-relevant tests go in `core/tests/<invariant>.rs`. Unfinished ones are `#[ignore = "M<n>: reason"]`. Never delete them, and never ignore one to get CI green.
- Docs and specs are in 台灣繁體中文. Code, comments, README, and SECURITY.md are in English.

## Gotchas

- `test-endpoint` is the only way to point `AnthropicClient` anywhere else. A release build that enables it hits `compile_error!`, so `cargo test --release --all-features` fails by design.
- `core/tests/downloader.rs` text-scans `src/downloader.rs` for `vault`, `secrecy`, `SecretString`, `AnthropicClient`, and `api::`. Do not write those words in non-comment code there.
- `validate_base_url` is an exact string match on purpose. Do not "improve" it with URL parsing.
- Hotkeys (V8/V9 decided, docs/changes/0004): Windows uses RegisterHotKey only (Alt+Shift+Z / Alt+Shift+X); do not add a keyboard hook or Raw Input. macOS fn+Z / fn+Shift+Z uses one active CGEventTap in a single isolated file whose callback reads only the Z keycode and the fn/Shift flags and stores nothing; Carbon ⌥Space / ⌥⇧Space is the fallback. Right Alt is deferred to v1.1+.
- Model IDs and the `anthropic-version` header are 【待驗證 V4】. Check them against official docs before you rely on them.
