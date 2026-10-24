# Verifying a keepsay download

keepsay asks you to trust it with an API key, so you should be able to check that the binary you downloaded is the one GitHub Actions built from a tagged commit of this repository. A copy that someone rebuilt, patched, or swapped on a mirror should fail these checks. This page shows how to run them.

Releases are built only by [`.github/workflows/release.yml`](../../.github/workflows/release.yml), only from a `vMAJOR.MINOR.PATCH` tag on `main`, and a maintainer has to approve each one before it is published (PROPOSAL §3 INV-8, §9).

> **Status.** Release binaries are not code-signed yet. Apple Developer ID / notarization and Windows Authenticode are still to come (PROPOSAL §16 items 1–2). Until then, macOS Gatekeeper and Windows SmartScreen will warn when you open the binary, and the provenance check below is the only proof of where it came from.

## What a release contains

| File | What it is |
|------|------------|
| `keepsay-cli-<version>-x86_64-unknown-linux-gnu.tar.gz` | Linux x86-64 CLI |
| `keepsay-cli-<version>-aarch64-apple-darwin.tar.gz` | macOS Apple Silicon CLI |
| `keepsay-cli-<version>-x86_64-pc-windows-msvc.zip` | Windows x86-64 CLI |
| `SHA256SUMS` | SHA-256 of every archive above |
| `keepsay-cli-<version>.sigstore.json` | Sigstore bundle with the build provenance attestation, for offline checks |

Each archive holds the binary, `LICENSE`, `README.md`, `SECURITY.md` and `BUILDINFO.txt`. `BUILDINFO.txt` records the commit, target, cargo arguments, `SOURCE_DATE_EPOCH` and the full `rustc -Vv` output.

The macOS app (M2), the Windows installer (M3) and the iOS app (M4a) will go through the same pipeline when they exist.

## Quick check with the script

[`scripts/release/verify.sh`](../../scripts/release/verify.sh) runs both checks below. It is a short POSIX shell script, so please read it before you run it. Take it from the tag you are verifying, not from `main`:

```sh
TAG=v1.2.3
curl -fsSLO "https://raw.githubusercontent.com/rightson/keepsay/$TAG/scripts/release/verify.sh"
less verify.sh
sh verify.sh -t "$TAG" SHA256SUMS keepsay-cli-1.2.3-aarch64-apple-darwin.tar.gz
```

It prints `OK` or `FAIL` for each file and exits non-zero on any failure. If the [GitHub CLI](https://cli.github.com/) (`gh`) is not installed, the script checks the hashes and then says provenance was **not** checked. `gh` is optional, but without it you only know the file matches the `SHA256SUMS` you downloaded next to it.

On Windows, run it from Git Bash or WSL, or follow the manual steps.

## Manual check

### 1. Checksums

The checksum shows the download is complete and matches `SHA256SUMS`. On its own it does not show who made either file. Step 2 does that.

```sh
# Linux
sha256sum --ignore-missing -c SHA256SUMS
# macOS
shasum -a 256 --ignore-missing -c SHA256SUMS
```

```powershell
# Windows PowerShell: compare with the matching line in SHA256SUMS
(Get-FileHash .\keepsay-cli-1.2.3-x86_64-pc-windows-msvc.zip -Algorithm SHA256).Hash.ToLower()
```

### 2. Build provenance

Each release carries a [GitHub artifact attestation](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations) in SLSA provenance format, made by `actions/attest-build-provenance` and signed through Sigstore with the workflow's own OIDC identity. A valid attestation shows that the file was built by `release.yml` in `rightson/keepsay`, on a GitHub-hosted runner, from the tag and commit it names.

```sh
TAG=v1.2.3
for f in SHA256SUMS keepsay-cli-*; do
  gh attestation verify "$f" \
    --repo rightson/keepsay \
    --signer-workflow rightson/keepsay/.github/workflows/release.yml \
    --source-ref "refs/tags/$TAG" \
    --deny-self-hosted-runners
done
```

`gh attestation verify` looks up attestations through the GitHub API, so run `gh auth login` first. To skip the lookup, use the bundle from the release instead. Add `--bundle keepsay-cli-<version>.sigstore.json`. gh still fetches Sigstore's trust root unless you also pass `--custom-trusted-root`.

To see what was attested, including the commit SHA, the workflow run and the runner, add `--format json`.

## Rebuilding it yourself (reproducibility)

The release build aims to be reproducible. Two honest builds of the same tag, with the same toolchain and target, should produce the same binary. The pipeline does the following:

- It uses the Rust toolchain pinned in `rust-toolchain.toml` and builds with `cargo build --locked`.
- It sets `SOURCE_DATE_EPOCH` to the tag commit's committer time.
- It passes `--remap-path-prefix` for the checkout and `CARGO_HOME`. It adds `-ffile-prefix-map` for the C/C++ in whisper.cpp and `/Brepro` on Windows.
- It builds archives with a fixed file order, fixed modes, owner 0, mtimes from `SOURCE_DATE_EPOCH`, and `gzip -n`.

To try it on the same OS and architecture as the artifact:

```sh
git clone https://github.com/rightson/keepsay && cd keepsay
git checkout v1.2.3
rustup toolchain install          # reads rust-toolchain.toml
scripts/release/build.sh aarch64-apple-darwin
scripts/release/package.sh aarch64-apple-darwin 1.2.3 dist
scripts/release/checksums.sh dist
```

Then compare the binary inside your archive with the one inside the release archive, and compare the `BUILDINFO.txt` files. Known gaps that can still cause differences:

- C/C++ compiler and SDK versions on the build machine (whisper.cpp and aws-lc are compiled from source). CI records the runner image in the job summary.
- On macOS, the system `tar` and Xcode version. On Windows, MSVC and `zip`/7-Zip versions.
- Once binaries are code-signed, the signature will differ. Compare with the signature removed.

If your rebuild differs and you cannot explain why, please report it. See below.

## If a check fails

Do not run the file. Download it again from the [Releases page](https://github.com/rightson/keepsay/releases). If it still fails, report it privately as described in [SECURITY.md](../../SECURITY.md). A release that fails verification is a security issue (INV-8).
