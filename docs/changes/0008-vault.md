# 0008 — Vault：Keychain 與 Credential Manager 實作、V6 結論、金絲雀測試

- Status：Accepted
- Date：2026-10-24
- 影響：`core/src/vault/*`、`core/tests/canary.rs`、`core/tests/vault_platform.rs`、ROADMAP（US-M1-02、04、05、06、21）。PROPOSAL §15 V6 的結論由整合階段從本檔搬入；§6.2 的屬性不變。

## Context

M1 工作流 C 要實作 PROPOSAL §6.2 的 vault（INV-1、INV-2）與 §11 的金絲雀測試。V6 的文件研究（security-framework 3.7.0 原始碼）發現兩個陷阱，而且 macOS 的 data protection keychain 需要 entitlement，未簽章的 binary 用不了。使用者已決定：

- 批准 security-framework 的 `OSX_10_15` feature（0005 已啟用），macOS 使用 data protection keychain（`use_protected_keychain`）。
- 永遠不用 `PasswordOptions::set_access_control_options`。
- 未簽章的開發 CLI 無法寫入 data protection keychain 時，回傳明確的 `CoreError`，不加較弱的備援儲存。

## Decision

### V6 結論

可以完全由 Rust（security-framework 3.7.0）寫入，不需要改由 Swift 寫入。

- 保護類別：`PasswordOptions` 沒有單獨設定 `kSecAttrAccessible` 的函式，唯一途徑是 `SecAccessControl::create_with_protection(Some(ProtectionMode::AccessibleWhenUnlockedThisDeviceOnly), flags)` 再 `set_access_control`。預設 `flags = 0`；使用者開啟生物辨識時 `flags = AccessControlOptions::USER_PRESENCE`，保護類別仍是 ThisDeviceOnly。
- 陷阱 1：`set_access_control_options` 內部呼叫 `create_with_flags`，等於 protection = `None`，crate 會改用 `kSecAttrAccessibleWhenUnlocked`（不是 ThisDeviceOnly），違反 INV-1。禁止使用，`apple.rs` 模組文件寫明。
- 陷阱 2：`set_access_synchronized(None)` 代表 `kSecAttrSynchronizableAny`。set、get、delete、搜尋一律明確傳 `Some(false)`，只碰非同步的 store。
- data protection keychain：每個查詢都加 `use_protected_keychain()`（macOS 需 `OSX_10_15`；iOS 一律是 DP keychain）。屬性查詢（`ItemSearchOptions`）在 macOS 用 `ignore_legacy_keychains()`，它在 `OSX_10_15` 下就是 `kSecUseDataProtectionKeychain = true`。舊式 file-based login keychain 不強制保護類別與 synchronizable，所以從不使用。
- 不設 `kSecAttrAccessGroup`（INV-10 前置）。
- 更新語意：`set_generic_password_options` 遇到既有項目會走 `SecItemUpdate`，不保證換掉舊的 access control。`store` 改為「先刪除、再新增」，確保每次寫入都是上述屬性。代價：新增失敗時舊 key 也已刪除，使用者重新輸入即可。
- entitlement：DP keychain 在 macOS 需要 `keychain-access-groups` entitlement（Developer ID 需搭配 provisioning profile）。未簽章的 binary（`keepsay-cli`、`cargo test`）會得到 `errSecMissingEntitlement`（-34018），對應為 `CoreError::Unsupported`，並記一筆只含 OSStatus 數字的 warn。不退回 file-based keychain、不寫檔、不改用環境變數。
- 仍需實機驗證（M2）：簽章後的 App 能否寫入、Keychain Access 顯示的屬性、userPresence 的提示行為、iCloud Keychain 開啟時確實不同步。macOS CI 的整合測試只能驗證「未簽章時乾淨地回 `Unsupported`，且同步 store 與 DP keychain 都沒有殘留」。

### Apple vault（`vault/apple.rs`）

- generic password，service `dev.rightson.keepsay.apikey`，account `anthropic`。
- `kSecAttrLabel` 存 fingerprint（`…a1B2`）。`key_fingerprint` 與 `exists` 只讀屬性，不取回密碼，所以開啟 userPresence 時顯示末四碼不會跳生物辨識提示。label 不符合 `…` 加四個字元的格式時才退回讀取 key。
- `load` 取回的 `Vec<u8>` 包在 `Zeroizing` 裡，複製進 `SecretString` 後清零。
- OSStatus 對應：-25300 → `KeyNotFound`；-34018 → `Unsupported`；其他 → `VaultUnavailable`。`delete` 在沒有項目時視為成功。
- `VaultOptions { require_user_presence }` 與 `platform_vault_with(options)` 提供 userPresence 開關。設定值放在哪裡由 Config（工作流 D）與 FFI 整合決定。§6.2 的「生物辨識時快取 10 分鐘」尚未實作，預設與目前一律不快取。

### Windows vault（`vault/windows.rs`）

- `CredWriteW` / `CredReadW` / `CredDeleteW`，`CRED_TYPE_GENERIC`，`CRED_PERSIST_LOCAL_MACHINE`，target `dev.rightson.keepsay/anthropic`，blob 為 key 的 UTF-8 bytes。
- `CredReadW` 的結果由 `ReadCredential` 持有，`Drop` 時先以 `zeroize` 清零 blob，再 `CredFree`。交給 `CredWriteW` 的 key 副本是 `Zeroizing<Vec<u8>>`。
- 共 7 個 `unsafe` 區塊（其中 1 個只在 `test-support` 編譯），各有 `// SAFETY:` 註解。`ERROR_NOT_FOUND` → `KeyNotFound`，其他 → `VaultUnavailable`（warn 只記 HRESULT 數字）。

### 共用（`vault/mod.rs`）

- `check_format`：`sk-ant-` 前綴、長度 20–512 bytes、只允許 ASCII 英數、`-`、`_`。擋下貼上時夾帶的空白、換行、引號。三個 backend 的 `store` 都會再檢查一次。
- `store_key(vault, key, verify)`：格式檢查 → `verify`（正式環境一律是 `AnthropicClient::verify_key`）→ 存入 → 回傳 fingerprint。任一步失敗都不存，也不動已存的 key。`verify` 做成參數，讓這段流程不連網就能測，也不必等工作流 A。
- `has_key`、`key_fingerprint`、`delete_key`（冪等）。§6.1 的 FFI 由 US-M1-22 包裝這些函式。
- `KeyVault::key_fingerprint` 的預設實作會在 vault 模組內 `load` 後立即 drop。這是 0005「core 內只有 `AnthropicClient` 呼叫 `load`」的唯一例外，只有末四碼離開。
- `MockVault`：與真實 backend 同樣拒絕格式錯誤、`delete` 冪等；`MockVault::failing(err)` 用來測錯誤路徑，`load_count()` 用來測「每次請求都重新讀取、不快取」。

### 測試

- `core/tests/canary.rs`（`test-support`）：`Canary` harness 以 TRACE 等級的 tracing subscriber 把所有輸出收進記憶體 buffer，提供 tempdir 當設定目錄，收集公開型別的 `Debug` / `Display`，最後掃描 log、所有 rendering、tempdir 下所有檔案（UTF-8 與 UTF-16LE）。目前涵蓋 vault 的正常路徑、驗證失敗、格式錯誤、vault 失敗（`VaultUnavailable`、`Unsupported`）、無 key 時的 load、全部 `CoreError` variant。另有一個測試確認掃描器真的會抓到外洩。其他工作流與整合階段把自己的路徑加進 `vault_paths_never_leak_the_canary` 或另寫測試共用 harness；`write_config_artifacts` 是給 Config / Budget 寫檔的掛勾。
- `core/tests/vault_platform.rs`（`test-support`，僅 Apple / Windows）：使用含 process id 的測試用 service / target，不會碰到開發者真正的 key。Windows 驗證 Type = GENERIC、Persist = LOCAL_MACHINE、取代 key、刪除冪等。Apple 驗證 `pdmn = aku`（WhenUnlockedThisDeviceOnly）、label、同步 store 沒有項目；未簽章時只接受乾淨的 `Unsupported`。

### 開發 CLI 的 key

未簽章的 CLI 在 macOS 上 `platform_vault()` 會回 `Unsupported`（寫入時）。CLI 測試改用 mock vault，或由工作流 E 決定一個明確標示的開發用路徑；vault 模組不提供較弱的備援。Linux 上 `platform_vault()` 一律是 `Unsupported`。

## Consequences

- 交叉編譯檢查：完整的 keepsay-core 在 Linux 上無法為 `x86_64-pc-windows-msvc` / `aarch64-apple-darwin` 做 `cargo check`，因為 aws-lc-sys 的 C 程式碼需要目標平台的 SDK。改用只含 vault 模組（`#[path]` 引入）與相同依賴版本的暫存 crate，在 `x86_64-pc-windows-msvc`、`aarch64-apple-darwin`、`aarch64-apple-ios` 上 `cargo clippy -D warnings`（含 `test-support` 與 `vault_platform.rs`）通過。真正的建置與執行由 CI 的 macOS / Windows job 負責。
- 建議新增 `CoreError` variant（例如 `VaultNotEntitled`）取代目前借用的 `Unsupported`，讓 UI 能提示「此 binary 未簽章」。`CoreError` 凍結，交由整合階段決定。
- Apple 的 `store` 先刪後加，不是原子操作。
- 待人工：M2 實機驗證（上列 V6 項目）；Developer ID 的 `keychain-access-groups` entitlement 與 provisioning profile 列入 §16 第 1 項。
