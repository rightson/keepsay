# Security Policy

keepsay's main design goal is that your Anthropic API key is very hard to steal, and that a leak can only cost a bounded amount. The full threat model is in [`docs/specs/THREAT_MODEL.md`](docs/specs/THREAT_MODEL.md). The hard rules (INV-1 … INV-10) are in [`docs/specs/PROPOSAL.md`](docs/specs/PROPOSAL.md) §3.

## Status

keepsay is pre-alpha (milestone M0). Nothing has been released yet, and no binary should be trusted with a real key.

## Reporting a vulnerability

Please report privately through GitHub's **[private vulnerability reporting](https://github.com/rightson/keepsay/security/advisories/new)** ("Report a vulnerability" on the Security tab). Do not open a public issue for anything that could expose keys, transcripts, or the release pipeline.

Please include:
- the affected component (core, CLI, macOS, iOS, Windows, CI or release)
- the version or commit
- steps to reproduce
- the impact you see

This is a volunteer project, so there are no fixed response times. We aim to acknowledge reports within a week. There is no bug bounty.

## What is in scope

We especially want to hear about any of these:

- The API key ending up anywhere other than OS secure storage: logs, files, clipboard, crash output, settings, or across FFI/IPC (INV-1, INV-2).
- The key being sent to any host other than `https://api.anthropic.com`, including through redirects or proxies (INV-3).
- A request to the model that can include tools or otherwise let the model take actions (INV-4).
- Audio being written to disk, or transcripts being persisted (INV-5).
- Any telemetry, analytics or remote configuration (INV-6).
- The model downloader reaching the key, or accepting a file whose hash does not match (INV-7).
- Weaknesses in CI, signing, or provenance (INV-8).

## Fundamental limitation

The key lives on your device, so keepsay **cannot** fully defend against malware running as your own user. Such malware can debug the process, inject code, or ask the OS keystore for the item while you are logged in.

What keepsay does guarantee:
- The key is never stored as plaintext on disk.
- The key is held in memory as briefly as possible.
- The key can only be sent to one hardcoded destination.

Cap your possible loss on the account side:
- Use a dedicated Anthropic workspace for keepsay.
- Keep only prepaid credit in it, with no payment method and auto-reload off.
- Set a spend limit.
- Revoke the key the moment you suspect a leak.

Reports that come down to "malware running as the same user can read the key" are known and accepted. Reports showing a way to make that easier than it needs to be are welcome.
