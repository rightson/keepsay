# 0013 — M5 發佈管道：tag 觸發、人工核准、SHA256SUMS 與 build provenance

- Status：Proposed（workflow 尚未在真正的 tag 上跑過；§16 的 GitHub 設定完成並跑完第一個 tag 後改為 Accepted）
- Date：2026-10-24
- 影響：`.github/workflows/release.yml`（新）、`scripts/release/*.sh`（新）、`docs/release/VERIFYING.md`（新）；對應 INV-8、PROPOSAL §9、§13 M5、THREAT_MODEL T1 / A4、ROADMAP US-M5-01、02、05（部分）、03 / 04（佔位）

## Context

INV-8 要求正式發佈物只能由 CI 產生，必須簽章並附 build provenance。M5 的 DoD 是「任何人能從 tag 驗證 binary 的來源」。M0 只做了 PR CI（`ci.yml`），沒有發佈管道。Apple 與 Windows 的簽章憑證還沒申請（§16 第 1、2 項），App bundle（M2 / M3 / M4a）也還沒開始做。所以這一步先把「可驗證來源」這一段做完，簽章留成停用的佔位步驟。

## Decision

### 觸發與權限

- 只在 push `v*.*.*` tag 時觸發。另有 `workflow_dispatch` 作為 dry run：會建置、打包、產生 SHA256SUMS 並上傳 workflow artifact，但 `attest` 與 `publish` 兩個 job 只在 `meta.publish == 'true'`（真正的 tag push）時執行，dry run 永遠不會簽 attestation 或建立 release。
- `meta` job（`scripts/release/meta.sh`）在 tag push 時檢查三件事，任一不符就失敗：
  1. tag 是嚴格的 `vMAJOR.MINOR.PATCH[-PRE]`。
  2. tag 版本等於 `Cargo.toml` `[workspace.package] version`。
  3. tag 指向的 commit 在 `origin/main` 上，也就是只發佈經過 PR review 的程式碼。
- 最上層 `permissions: contents: read`。只有兩個 job 提高權限：
  - `attest`：`id-token: write`（向 Sigstore 取得簽章憑證）、`attestations: write`。
  - `publish`：`contents: write`（建立 draft release），並設 `environment: release`，必須人工核准。
- 只用 `actions/*` 第一方 action，全部釘完整 commit SHA（2026-10-24 以 `git ls-remote` 取得）：

  | Action | 版本 | SHA |
  |--------|------|-----|
  | actions/checkout | v7.0.1 | `3d3c42e5aac5ba805825da76410c181273ba90b1`（與 ci.yml 相同） |
  | actions/upload-artifact | v7.0.2 | `cf430e030ddbb5b0abf93d22962f4752f3646cd9` |
  | actions/download-artifact | v8.0.2 | `9000827ccba6bdab643e8b6fd33ac0654aef8333` |
  | actions/attest-build-provenance | v4.2.2 | `4d101475d8b20a2381f78447822ac1eab6504dd8` |

  attest-build-provenance v4.2.2 是 composite action，內部以 SHA 釘住 `actions/attest@508db95dd578ae2727ebd6217d5ba78e4fbda05d`（v4.2.1），同屬第一方。
- 不引用任何 repo 或 environment secret。唯一的例外是兩個 `if: false` 的簽章佔位步驟。建立 release 用的是 job 自己的 `github.token`。
- runner 用固定版本的 label（`ubuntu-24.04`、`macos-15`、`windows-2025`），不用 `-latest`，避免 image 在兩次 release 之間悄悄換掉。

### 建置

- 目標：`x86_64-unknown-linux-gnu`、`aarch64-apple-darwin`、`x86_64-pc-windows-msvc`。建置 `keepsay-cli`，使用預設 features（含 `whisper`），因為 CLI 的用途就是 wav → 逐字稿。
- 使用 `rust-toolchain.toml` 釘住的 toolchain，加上 `--release --locked --target <triple>`。
- 建置前執行 `scripts/release/feature-guard.sh`，沿用 ci.yml `release-feature-guard` 的作法：以 `cargo tree -p keepsay-cli --target <triple> -e normal,build,features -i keepsay-core` 檢查，出現 `test-endpoint`（INV-3）或 `test-support`（INV-1 / 2）就失敗。tree 中沒有 keepsay-core 時也算失敗，避免檢查本身失效卻照樣通過。core 的 `compile_error!` 是第二道防線。
- 以 `CARGO_ENCODED_RUSTFLAGS` 設定旗標，並忽略環境中的 `RUSTFLAGS`，避免其他旗標混進 release build。

### 可重現建置（US-M5-05 的第一步）

- `SOURCE_DATE_EPOCH` 取 tag commit 的 committer time。
- `--remap-path-prefix` 把 checkout 路徑換成 `/keepsay`，`CARGO_HOME` 換成 `/cargo`。非 MSVC 平台的 C/C++（whisper.cpp）加 `-ffile-prefix-map`；MSVC 加 `-Clink-arg=/Brepro`，讓 PE timestamp 固定。
- 封存檔：固定檔案順序、固定權限、owner 0、mtime 取 `SOURCE_DATE_EPOCH`、`gzip -n`。Windows 用 `zip -X -D`，沒有 zip 時改用 7-Zip（`-mtc=off`）。
- 封存檔內附 `BUILDINFO.txt`：commit、target、cargo 參數、`SOURCE_DATE_EPOCH`、`cargo -V`、`rustc -Vv`。只放由（commit、toolchain、target、參數）決定的內容；runner image 版本與 cmake 版本寫在 job summary，不放進封存檔，否則封存檔的雜湊會隨 runner 變動。
- 尚未完成：「兩次建置雜湊比對」的實測（US-M5-05 驗收 ②）。已知差異來源：C/C++ 編譯器與 SDK 版本、macOS 的 bsdtar 與 Xcode、Windows 的 MSVC 與 zip / 7-Zip 版本。這些寫在 VERIFYING.md。

### 雜湊、provenance 與發佈

- `sums` job 收集三個封存檔，確認數量正好是 3，產生 `SHA256SUMS`（`sha256sum` 格式，依名稱排序）。
- `attest` job 先以 `sha256sum --strict -c` 重新比對，再用一次 `attest-build-provenance` 為所有封存檔和 `SHA256SUMS` 產生同一份 SLSA provenance（多個 subject）；對其中任何一個檔案，`gh attestation verify` 都能通過。Sigstore bundle 另存為 `keepsay-cli-<version>.sigstore.json` 附在 release，供離線驗證。
- `publish` job 經人工核准後再比對一次雜湊，然後以 `gh release create --draft --verify-tag` 建立 **draft** release。最後的「Publish release」按鈕由人按，等於第二道人工確認。同一 tag 已有 release 時直接失敗，不覆寫既有資產。
- 使用者驗證：`scripts/release/verify.sh`（POSIX sh，只需 awk 與 sha256sum / shasum）檢查 SHA256SUMS；有安裝 `gh` 時，再以 `--signer-workflow …/release.yml --deny-self-hosted-runners [--source-ref refs/tags/<tag>]` 驗證 provenance。`gh` 是選用工具：沒有 `gh` 時腳本會明確印出「provenance NOT checked」。說明文件：`docs/release/VERIFYING.md`。

### 簽章（佔位，停用）

- macOS codesign + notarytool、Windows Authenticode（雲端 HSM）各有一個 `if: false` 步驟，位置在建置之後、打包之前，確保 SHA256SUMS 與 provenance 涵蓋的是簽過的 bytes。TODO 指向 §16 第 1、2 項。
- 啟用時的規則（寫在 workflow 註解中）：
  - 憑證只能存成 `release` environment 的 secret，不得用 repo secret。
  - build job 要加上 `environment: release`，簽章也要等人工核准。
  - 只用 Apple / Microsoft 官方 CLI（codesign、notarytool、signtool），不用第三方 action。
- 單一 CLI binary 無法 staple notarization ticket；M2 的 DMG 才能 staple。

### 不做的事

- 不使用第三方 action，例如 `softprops/action-gh-release`、`dtolnay/rust-toolchain`、`Swatinem/rust-cache`。
- 不做快取（rust-cache）。快取可能被 PR workflow 污染（cache poisoning），release build 一律從頭編譯。
- 不做自動更新的資訊來源（ADR-5）。

## Consequences

- INV-8 中的「只由 CI 產生」與「附 provenance」有了機制；「簽章」要等 §16 第 1、2 項完成後才能做。在那之前，release notes 與 VERIFYING.md 都明確寫出「尚未簽章」。
- 每次 release 的建置時間取決於 whisper.cpp 與 aws-lc 的 C/C++ 編譯。因為不做快取，時間會比較長，這是刻意的取捨。
- Windows 與 macOS runner 上的 whisper（bindgen 需要 libclang、cmake）還沒在 CI 實際跑過（ci.yml 的 test-os 不開 whisper）。第一次在 tag 上執行前，應先用 `workflow_dispatch` dry run 確認。
- 版本號：發佈前要先在 PR 中更新 `Cargo.toml` 的 workspace version，否則 `meta` 會擋下 tag。
- 未來新增 App bundle（M2 DMG、M3 MSI / NSIS、M4a TestFlight）時，都加在本 workflow，走同一套 sums、attest、publish 流程，並在本檔之後另寫紀錄。
- THREAT_MODEL T1 的「只由 CI 發佈，附簽章與 provenance（INV-8，M5）」部分落實；A4 的保護多了一道 environment 核准與 tag 必須在 main 上的檢查。

### 需要人工處理（建議加入 PROPOSAL §16 第 3 項）

1. 在 repo Settings → Environments 建立 `release`：
   - 設定 required reviewers，並勾選 prevent self-review。
   - deployment branches and tags 限定為 `v*.*.*` tag。
2. 建立 tag ruleset：只有維護者能建立、刪除或移動 `v*` tag，並禁止 force push 改寫 tag。
3. 在 Actions 設定中：
   - 預設 workflow permissions 設為 read。
   - 只允許 GitHub 擁有的 action，並開啟「require actions pinned to a full-length commit SHA」。
4. 確認 repo 是 public。private repo 的 artifact attestation 需要 GitHub Enterprise Cloud。
5. 簽章 secrets（§16 第 1、2 項完成後）只放在 `release` environment。
6. 第一次 release 前，先用 `workflow_dispatch` 跑一次 dry run，確認三個平台的 whisper 建置都能通過。
