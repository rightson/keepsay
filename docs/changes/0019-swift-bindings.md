# 0019 — Swift binding 產生器、core staticlib、設定與模型清單 FFI（M2）

- Status：Accepted（依 0018 D-12 的批准實作；第 3 節兩項 deny.toml 調整待人工複核）
- Date：2026-10-25
- 影響：`Cargo.toml`（workspace members）、`uniffi-bindgen/`（新）、`core/Cargo.toml`（crate-type）、`core/src/ffi.rs`（新增 3 個匯出）、`core/tests/ffi_surface.rs`、`core/tests/ffi_flow.rs`、`deny.toml`、`supply-chain/`、`apple/scripts/build-core.sh`、`apple/KeepsayKit/`、`.github/workflows/macos-app.yml`、ROADMAP M2。取代 0015 第 2 節「`cargo install uniffi-bindgen-swift`」與「core/Cargo.toml 的 crate-type 維持不變」兩點。

## Context

0015 把 Swift binding 產生隔離成待批准的一步，CI 只能交叉編譯 static lib，`UniffiCore.swift` 是依命名慣例手寫、從未編譯過的對應。0018（D-12）批准：workspace 內新增只做 binding 產生的 bin crate，只有它開 uniffi 的 `cli`；keepsay-core 可加 `staticlib`。0015 也列出 App 需要但 core 沒有的 FFI：讀寫設定、模型清單與安裝狀態。

## Decision

### 1. `keepsay-uniffi-bindgen`（`uniffi-bindgen/`）

- workspace member，`publish = false`，bin 名 `uniffi-bindgen-swift`，`main` 只呼叫 `uniffi::uniffi_bindgen_swift()`（uniffi 自己的 `uniffi-bindgen-swift.rs` 同一行）。
- 依賴：`uniffi = { version = "=0.32.2", default-features = false, features = ["cli"] }`。版本釘死與 keepsay-core 的 uniffi 相同（產生的 binding 要與 scaffolding 的 metadata、checksum 一致）。不開 `cargo-metadata`：library mode 不需要讀 `uniffi.toml`。
- 套件名不用 `uniffi-bindgen`：crates.io 把 `-` 與 `_` 視為同名，會與上游 `uniffi_bindgen` 混淆。
- keepsay-core 本身不開 `cli`。cargo 的 feature unification 在 `cargo build --workspace` 時會讓 uniffi 帶著 `cli` 編譯一次，所以 App 與發佈用的 library 一律以 `-p keepsay-core` 單獨建置（`build-core.sh`；`scripts/release/` 建的是 `keepsay-cli`，同樣是 `-p`）。
- 新進 Cargo.lock 的 crate（全部只經由 `uniffi/cli`）：anstyle 1.0.14、askama / askama_derive / askama_macros / askama_parser 0.16.1、basic-toml 0.1.10、clap / clap_builder / clap_derive 4.6.7、clap_lex 1.1.1、goblin 0.8.2、plain 0.2.3、scroll 0.12.0、scroll_derive 0.12.1、smawk 0.3.3、strsim 0.11.1、textwrap 0.16.4、unicode-width 0.2.2、uniffi_bindgen 0.32.2、uniffi_udl 0.32.2、weedle2 5.0.0。

### 2. keepsay-core `crate-type = ["lib", "staticlib"]`

- 不加 `cdylib`：`uniffi-bindgen-swift` 的 library mode 直接讀 static lib 內的 UniFFI metadata（Linux 上以 `libkeepsay_core.a` 實測可產生 `keepsay_core.swift`、`keepsay_coreFFI.h`、`module.modulemap`）。
- 代價：每次建置 keepsay-core 都多封存一個 `.a`（0012 原本為此避免）。換來的是 `build-core.sh` 改用一般的 `cargo build -p keepsay-core --lib --target …`，不需要 `cargo rustc --crate-type`。

### 3. supply chain

- cargo-vet：先以 `cargo vet`（非 locked）更新匯入，6 個新 crate 由既有匯入（mozilla、google、bytecode-alliance…）涵蓋（basic-toml、strsim、unicode-width、uniffi_bindgen、uniffi_udl、weedle2，含 delta）；其餘以 `cargo vet regenerate exemptions` 產生一次性 exemption（依 0011 / 0018 批准的流程）。沒有舊 exemption 可以 prune。結果：`Vetting Succeeded (53 fully audited, 21 partially audited, 176 exempted)`，exemption 條目 181 → 196。

  | crate | exemption 版本 | 準則 | 備註 |
  |-------|----------------|------|------|
  | anstyle | 1.0.14 | safe-to-deploy | |
  | askama | 0.16.1 | safe-to-deploy | |
  | askama_derive | 0.16.1 | safe-to-deploy | |
  | askama_macros | 0.16.1 | safe-to-deploy | |
  | askama_parser | 0.16.1 | safe-to-deploy | |
  | clap | 4.6.7 | safe-to-deploy | |
  | clap_builder | 4.6.7 | safe-to-deploy | |
  | clap_derive | 4.6.7 | safe-to-deploy | |
  | clap_lex | 1.1.1 | safe-to-deploy | |
  | goblin | 0.1.3 | safe-to-deploy | delta 基準；lock 為 0.8.2 |
  | plain | 0.2.3 | safe-to-deploy | |
  | scroll | 0.10.2 | safe-to-deploy | delta 基準；lock 為 0.12.0 |
  | scroll_derive | 0.12.1 | safe-to-deploy | |
  | smawk | 0.3.3 | safe-to-deploy | |
  | textwrap | 0.16.4 | safe-to-deploy | |

  準則是 safe-to-deploy 而不是 safe-to-run：cargo-vet 以全 feature 的依賴圖計算，uniffi 是 keepsay-core 的依賴，所以它在 `cli` 下的依賴也被要求 safe-to-deploy（實際的 App / 發佈建置不會編譯它們）。試過 `[policy.keepsay-uniffi-bindgen] criteria = "safe-to-run"`，因上述原因無效，未保留。
- cargo-deny（**待人工複核**，屬 D-12 的直接後果，但仍是政策調整）：
  - `licenses.exceptions` 加 `uniffi_bindgen`、`uniffi_udl` 兩個 MPL-2.0 逐 crate 例外（0012 已對另外六個 uniffi crate 批准 MPL-2.0）。
  - `advisories.ignore` 加 `RUSTSEC-2026-0374`（basic-toml 已停止維護，不是漏洞）。只經由 `uniffi_bindgen → askama 的 config feature` 進來，沒有不帶它的 uniffi / askama 版本。uniffi 換掉它之後移除。

### 4. 新 FFI：`get_settings`、`save_settings`、`list_models`

- `Settings` record：`mode`、`translate_to`、`stt_model`、`glossary`、`input_usd_per_mtok`、`output_usd_per_mtok`、`soft_limit_usd`、`hard_limit_usd`、`hotkey_polish`、`hotkey_polish_translate`。不含 `model`（allowlist 內固定）與 `proxy`（URL 可能帶帳密）；`save_settings` 保留檔案中原有的值。`Debug` 只印 glossary 長度。`core/tests/ffi_surface.rs` 釘住匯出清單與 `Settings` 的欄位名。
- `save_settings` 走 `Config::save`（驗證 + temp + rename）。現有檔案無法讀取或 schema 較新時回 `Config`，不覆寫（§10）。
- `list_models() -> Vec<ModelStatus { id, name, size, installed }>`：`installed` 只看 `models/<id>.bin` 是否存在，SHA-256 仍在載入時驗證（INV-7）。
- `core/tests/ffi_flow.rs` 覆蓋：預設值、來回、無效值拒絕、較新 schema 不被覆寫、模型清單與安裝狀態。

### 5. Swift 端

- `build-core.sh`：每個 target `cargo build -p keepsay-core --lib --locked --release --features whisper --target …`，lipo 成 macos / ios / ios-sim 三份；`cargo build -p keepsay-uniffi-bindgen --release --locked`，以第一個平台第一個 target 的 `.a` 產生 binding（所有 slice features 相同）；`xcodebuild -create-xcframework`；複製到 `KeepsayKit/Frameworks/` 與 `Sources/KeepsayCoreBindings/`（仍 gitignore，不提交產生的檔案）。`KEEPSAY_SKIP_BINDGEN` 保留給只想確認交叉編譯的人；`UNIFFI_BINDGEN_SWIFT` 環境變數移除。
- `Package.swift`：linker 另加 Foundation、Metal、MetalKit（staticlib 不帶 whisper-rs-sys 的 link 指示）；測試 target 在 core 存在時也依賴 `KeepsayCoreBindings`。
- `UniffiCore.swift` 改為對照實際產生的名稱（`CoreError` 為 flat error，`ApiStatus` 沒有狀態碼，對應為 `.apiStatus(0)`）。網路、STT、下載、匯入一律 `Task.detached`；`has_key`、`key_fingerprint`、`budget_status`、`get/save_settings`、`list_models` 只讀 Keychain 屬性或小 JSON 檔，直接呼叫。`budget_status` 失敗時視為已達硬上限（0009），不再回「沒有用量」。
- 資料目錄：`Application Support/dev.rightson.keepsay`，與 keepsay-cli 在 macOS 的預設相同（§10），取代 0015 的 `…/keepsay`。
- 快捷鍵在 `config.json` 的表示：macOS 的 `hotkeys.polish` / `polish_translate` 存 Carbon 組合，格式 `Option+Shift+Space`、`Control+Option+VK6`（`KeyCombo.configString`，與 core 的 `MACOS_FALLBACK_HOTKEYS` 同風格）；`None` 表示預設 ⌥Space / ⌥⇧Space。fn+Z tap 在 v1 不可改鍵。「同時啟用備援組合」不在 config schema 內，只存在記憶體（每次啟動回到關閉），避免為此改 schema。
- CI `core-linked` job（取代 `core-staticlibs`）：建 XCFramework → 檢查三份 `.a` 含 UniFFI 符號 → 檢查產生的 Swift 公開函式正好是預期清單（INV-2：沒有回傳 key 的函式）→ KeepsayKit 測試（含 `UniffiCoreTests`，真的呼叫 FFI）→ KeepsayKit 建置 iOS Simulator 與 iOS device → KeepsayMac 測試 → 打包連結 core 的 ad-hoc App。產物 `KeepsayCoreFFI-xcframework`（XCFramework + `keepsay_core.swift`，給 iOS 工作流）與 `Keepsay-macOS-adhoc`。

## Consequences

- iOS（M4a）可從 CI 產物或在 macOS 上跑 `apple/scripts/build-core.sh` 取得同一份 binding 與 XCFramework。
- D-11（`uniffi/tokio`）仍未決定，FFI 仍是阻塞呼叫。
- ad-hoc 簽章的 App 寫入 data protection keychain 會得到 `VaultNotEntitled`（0008），所以 CI 只能測到格式檢查為止；真正的 store / verify / polish 需要 §16 第 1 項的簽章與 entitlement。
- THREAT_MODEL：新增的三個 FFI 函式不經手 key，只碰非機密設定與模型清單；§6 維護規則要求記錄新 FFI 函式，B1 敘述不變。
