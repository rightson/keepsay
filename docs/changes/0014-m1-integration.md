# 0014 — M1 整合：Keychain 原子取代、VaultNotEntitled、只回 fingerprint、UniFFI 匯出、端到端與金絲雀

- Status：Accepted（程式碼與測試已合併；PROPOSAL 的對應修改見文末，由負責 `docs/specs/*` 的整合者套用；`uniffi/tokio` 與 bindgen 的 feature 待人工決定）
- Date：2026-10-24
- 影響：`core/src/vault/{apple,mod,windows}.rs`、`core/src/api/{error,client}.rs`、`core/src/ffi.rs`、`core/src/panic_hook.rs`（新）、`core/src/lib.rs`、`core/src/polish/{mod,modes}.rs`、`core/src/budget.rs`、`core/src/stt/mod.rs`、`cli/src/main.rs`、`cli/Cargo.toml`、`core/tests/{canary,polish_e2e,vault_platform,no_disk_writes}.rs`、新測試 `core/tests/{no_key_load,ffi_surface,ffi_flow}.rs`、`cli/tests/cli_api.rs`；ROADMAP M1；PROPOSAL §6.1、§6.2、§6.9、§11（待套用）
- 故事：US-M1-06、13、20、21、22、23、25

## Context

五個 M1 工作流（0005–0012）合併後，留下幾件跨模組、只能在整合階段處理的事：

1. 0008：Apple vault 取代 key 是「先刪後加」，新增失敗時舊 key 也沒了。
2. 0008：未簽章 binary 拿不到 data protection keychain 時借用 `CoreError::Unsupported`，UI 無法分辨「平台不支援」與「這個 binary 沒簽章」。
3. 0010：CLI 的 `key status` / `key verify` 為了拿 fingerprint 自己呼叫 `KeyVault::load`，違反 0005「core 內只有 `AnthropicClient` 呼叫 `load`」的精神，也沒有機器檢查。
4. US-M1-22：§6.1 的 UniFFI 匯出還沒寫（0012 已加入 uniffi，但未開 `tokio` feature）。
5. US-M1-13：`core/tests/polish_e2e.rs` 等工作流 A 的 client 而被 ignore；polish 判斷「輸出是否完整」時，`stop_reason` 缺失會被當成完成，與 0006 的 `is_complete()` 不一致。
6. US-M1-23：CLI 的 polish / key / budget 路徑沒有對 API 的整合測試。
7. US-M1-20、21、25：金絲雀只跑了 vault 路徑；core 沒有 panic hook；INV-5 的文字掃描用寫死的檔案清單。

## Decision

### 1. Apple：取代 key 不再有「舊 key 已刪、新 key 未寫」的空窗

security-framework 3.7.0 的 `SecItemUpdate` 包裝（`ItemUpdateOptions`）不能設定 `kSecAttrAccessControl`，所以「原地更新」無法保證使用者切換 userPresence 後屬性跟著變（這也是 0008 改成先刪後加的原因）。改用三步驟：

1. 以全新項目寫入 account `anthropic.pending`（屬性同 0008：ThisDeviceOnly、`synchronizable = false`、DP keychain、label = fingerprint、不設 access group）。失敗 → 回錯，已存的 key 不動。
2. 刪除 account `anthropic` 的舊項目。失敗 → 刪掉 pending，回錯，已存的 key 不動。
3. `SecItemUpdate` 只改 pending 項目的 `kSecAttrAccount` 為 `anthropic`。值、access control、label 都是第 1 步新增時的內容。

- 若程序在第 2、3 步之間結束或第 3 步失敗，新 key 仍在 pending：`load` / `exists` / `key_fingerprint` 會退而讀 pending；下一次 `store` 先執行 `recover()`：pending 與正式項目同時存在 → 刪 pending（舊 key 仍是有效的那把）；只有 pending → 改名提交。`delete` 兩個 account 都刪。
- 測試：`vault_platform.rs` 在 macOS 上斷言取代後沒有殘留 pending、仍是 `aku`、同步 store 沒有項目；未簽章時斷言乾淨的 `VaultNotEntitled` 且沒有 pending。Linux 無法執行，已用只含 vault 模組的暫存 crate 對 `aarch64-apple-darwin`、`aarch64-apple-ios`、`x86_64-pc-windows-msvc` 跑 `cargo clippy -D warnings`（含 `test-support`）通過。
- 待 M2 實機確認：開啟 userPresence 的項目在第 3 步改 account 時是否會跳出驗證提示（屬性更新通常不需要，但未驗證）。

Windows：`CredWriteW` 對同 target / type 一次覆寫，失敗時舊 credential 原封不動，本來就沒有空窗。只在 `windows.rs` 模組文件寫明，不改程式。

### 2. `CoreError::VaultNotEntitled`

- 新 variant，不帶任何欄位，訊息 "this build is not entitled to use secure storage (unsigned binary?)"。`apple.rs` 把 `errSecMissingEntitlement`（-34018）對應到它；`Unsupported` 只留給「此平台沒有 vault 後端」（Linux）。
- CLI 的 raw 退回提示區分兩者。
- 另新增 `CoreError::NotInitialized`（FFI 在 `init` 之前被呼叫）。
- 金絲雀：`ALL_ERRORS` 列出每個 variant，另有一個沒有 `_` 分支的 `match`，新增 variant 時不同步更新就編譯失敗。

### 3. 只有 client 與 vault 讀 key（INV-2）

- 新增 `vault::stored_fingerprint(&dyn KeyVault) -> Result<Option<Fingerprint>, CoreError>`（`key_fingerprint` 會吞掉錯誤，這個不會）與 `AnthropicClient::verify_stored_key(&dyn KeyVault)`（每次嘗試才從 vault 讀 key，建好 header 立即 drop，同 `messages`）。
- CLI 的 `key status` 改用 `stored_fingerprint`，`key verify` 改用 `stored_fingerprint` + `verify_stored_key`，`key set` 改走 `vault::store_key`。CLI 不再呼叫 `load`。
- `core/tests/no_key_load.rs`：文字掃描 `core/src`、`cli/src`、`windows/`（日後的 Tauri app）的所有 `.rs`，非註解、非測試區塊中出現 `.load()` 或 `KeyVault::load(` 就失敗。允許：`core/src/api/client.rs`、`core/src/vault/**`、`tests/`、`tests.rs`、檔尾的 `#[cfg(test)] mod …`。`Config::load(path)`、`Budget::load(path, now)`、`WhisperEngine::load(path)`、atomic 的 `.load(ordering)` 都有參數，不會誤判。只靠 `cargo test`，不需要 CI 特別設定。

### 4. UniFFI 匯出（§6.1，`core/src/ffi.rs`）

`lib.rs` 加 `uniffi::setup_scaffolding!()`，namespace 為 `keepsay_core`。匯出：

| 函式 | 回傳 | 說明 |
|------|------|------|
| `init(data_dir)` | `Result<(), CoreError>` | 建立 process-wide core：平台 vault + host 傳入的 App Support 目錄（`config.json`、`budget.json`、`models/`）；安裝 panic hook。可重複呼叫（取代舊狀態） |
| `set_utc_offset(seconds)` | `Result<(), CoreError>` | 預算每月以本地時間歸零（0009）；預設 0 |
| `store_key(key)` | `Result<Fingerprint, CoreError>` | 進來就複製成 `SecretString`，再 `zeroize` 收到的 `String`；格式 → `GET /v1/models` → 存入 |
| `has_key()` | `bool` | |
| `key_fingerprint()` | `Option<Fingerprint>` | |
| `delete_key()` | `Result<(), CoreError>` | 冪等 |
| `verify_key()` | `Result<(), CoreError>` | `verify_stored_key` |
| `polish(transcript, opts)` | `Result<PolishResult, CoreError>` | 只有未 init 會是 `Err`；其他一律退回 raw；用量寫入 `budget.json`；`budget.json` 讀不到時視為已達硬上限（0009） |
| `transcribe(pcm, sample_rate, opts)` | `Result<String, CoreError>` | 僅 feature `whisper`；`Config::stt_model` 的模型第一次載入前做 SHA-256 驗證（INV-7），之後留在記憶體 |
| `budget_status()` | `Result<BudgetStatus, CoreError>` | |
| `download_model(id, progress)` | `Result<(), CoreError>` | `progress` 是 Swift 實作的 `DownloadProgress` trait（`#[uniffi::export(foreign)]`，只收兩個 `u64`） |
| `import_model(id, path)` | `Result<(), CoreError>` | §8 的手動匯入 |

- 型別：`Fingerprint` 以 `uniffi::custom_type!` 對應成字串；從外部 lift 時只接受 `…` + 4 字元的形狀，不可能夾帶 key。`CoreError` 是 `flat_error`，外部只看到 variant 與固定訊息。`Mode`、`TargetLang`、`FallbackReason` 為 enum；`PolishOptions`、`PolishResult`、`Usage`、`BudgetStatus`、`SttOptions` 為 record（手寫的 `Debug` 不變）。
- async 做法：匯出函式都是**同步阻塞**。每次網路呼叫建立一個 current-thread tokio runtime，並在其中建立新的 `AnthropicClient` / `ModelDownloader`（reqwest 的連線池綁在建立它的 runtime 上，跨 runtime 共用 client 會出錯，所以不共用）。理由：reqwest 需要 tokio reactor，uniffi 的 async 匯出要開 `uniffi/tokio` feature，不在 0012 批准的範圍。Swift 端從 `Task.detached` 呼叫，不在 main thread。每次多一個 runtime 與 TLS 設定的成本，相對一次聽寫請求可以忽略。這些函式不能在 tokio runtime 內呼叫；Rust host（CLI、Tauri）直接用各模組的 async API。若之後要改成真正的 async 匯出，需批准 `uniffi/tokio`（新 D 項目）。
- `crate-type` 不變（只有 `lib`）。proc-macro 匯出不需要改；`cargo build -p keepsay-core --release` 在 Linux 通過，`nm` 可看到 `uniffi_keepsay_core_fn_func_*` 與 `UNIFFI_META_*` 符號。M2 打包 xcframework 時再加 `staticlib`（以及 bindgen library mode 用的 `cdylib`），屆時評估改由一個薄的 FFI crate 承擔。
- Swift binding（M2 再做，本次沒有產生）：產生器需要 `uniffi` 的 `cli` feature（會帶入 `uniffi_bindgen`、clap 等新 crate），本次不加。M2 的建議做法：
  1. 在 workspace 外安裝（不進 Cargo.lock）：`cargo install uniffi --version 0.32.2 --features cli --locked`，或在 workspace 內加一個只有 `fn main() { uniffi::uniffi_bindgen_main() }` 的 `uniffi-bindgen` bin crate（`uniffi = { version = "0.32.2", features = ["cli"] }`）。兩者都需要依賴批准。
  2. `cargo build -p keepsay-core --release --target aarch64-apple-darwin`（crate-type 含 `cdylib`）。
  3. `uniffi-bindgen-swift target/aarch64-apple-darwin/release/libkeepsay_core.dylib apple/KeepsayKit/Sources/KeepsayCoreFFI --swift-sources --headers --modulemap`（library mode，從 dylib 讀 metadata）。
- 測試用掛勾 `ffi::testing`（`init_with(vault, dir, api_base)`、`set_api_base`、`reset`、`download_entry`）只在 `test-support` + `test-endpoint` 同時開啟時存在，不匯出到 FFI，release build 兩者都會 `compile_error!`。
- 測試：
  - `core/tests/ffi_surface.rs`：文字掃描 `ffi.rs`，匯出清單與每個回傳型別都釘死；名稱含 `key` / `fingerprint` 的只能回 `Result<Fingerprint, _>`、`Option<Fingerprint>`、`Result<(), _>`、`bool`；除 `transcribe` 外不得回傳 `String`；不得出現 `expose_secret`、`.load()`、`SecretString` 回傳；foreign trait 只有一個且只收進度數字。
  - `core/tests/ffi_flow.rs`：未 init 的行為、Linux 上 `init` 回 `Unsupported`、存 key → fingerprint、401 的取代不動舊 key 且不自動刪除、polish + 記帳、刪除後 NoKey、損毀的 ledger 視為硬上限、下載進度與雜湊不符不留檔、下載請求不帶 `x-api-key`。

### 5. polish 端到端

- `polish` 改用 `MessagesResponse::is_complete()`：只有 `end_turn` / `stop_sequence` 算完成；缺少、`max_tokens`、`refusal`、未知值一律退回 raw（保留 usage 供記帳）。
- `core/tests/polish_e2e.rs` 解除 ignore，並新增：請求 JSON 剛好五個 key、`thinking = {"type":"disabled"}`、沒有 `temperature` / `top_p` / `top_k` / `tools` / `tool_choice`、`anthropic-version` 為 `2026-10-21`、`max_tokens` / `refusal` 退回、每次請求各讀一次 vault。

### 6. CLI

- `core/` 與 `cli/` 的非測試程式碼沒有 `todo!` / `unimplemented!`（工作流合併後已是 0，本次確認）。
- `cli/Cargo.toml` 新增 feature `test-endpoint = ["keepsay-core/test-endpoint"]`（預設關，release build 由 core 的 `compile_error!` 擋下）；開啟時環境變數 `KEEPSAY_DEV_API_ENDPOINT` 可指定 loopback base URL（只接受 `http://127.0.0.1:<port>` / `http://localhost:<port>`）。這個變數只指定 endpoint，key 仍然只從 stdin / vault 取得。
- dev-dependencies 加入 `wiremock`、`serde_json`（皆在 §14 白名單、已在 Cargo.lock，只新增依賴邊，沒有新 crate）。
- `cli/tests/cli_api.rs`：`polish --text-stdin` 經 wiremock 回潤稿結果、只寫出 `budget.json`、`budget` 顯示記到的 token；`--translate en` 收到未翻譯輸出時印 raw；`key verify` / `key status` 只顯示 fingerprint；401 的 `key verify` 失敗；非 loopback endpoint 被拒時 polish 仍印 raw。stdout / stderr / 資料目錄都不含金絲雀。

### 7. 金絲雀、panic hook、INV-5

- `canary.rs` 新增 `integration_paths_never_leak_the_canary`（需 `test-endpoint`）：mock vault 存金絲雀，跑 401 / 403 / 402 / 400 額度 / 429 / 529 / 500 / 302 / 格式錯誤 / 逾時（`messages`、`verify_key`、`verify_stored_key`）、polish 的各種退回（守門、截斷、refusal、未翻譯、無 key、硬上限、API 錯誤）、Config / Budget 寫檔與損毀檔、下載器錯誤（雜湊、超過大小、redirect 到不允許的 host、404、匯入雜湊不符）、FFI（存 key 失敗、401、polish、budget、fingerprint、verify）。之後掃描 log、所有 rendering、資料目錄下所有檔案；另外斷言金絲雀只出現在送往 mock API 的 `x-api-key` header，body、URL、其他 header 都沒有。
- INV-5：polish / API 路徑跑完後資料目錄必須是空的（US-M1-25 ①）。`no_disk_writes.rs` 改為掃描 `stt/`、`polish/` 底下所有 `.rs`，不再用寫死的清單（含 `polish/tests.rs`）。
- `core::panic_hook::install()`：panic 時只以 tracing 記錄檔名與行號，不記 payload（§6.9，US-M1-20）。`ffi::init` 會安裝；CLI 保留自己的 stderr 版本。金絲雀測試以含金絲雀的 payload 觸發 panic，確認 log 有位置、沒有金絲雀。

## Consequences

- 測試數：`cargo test --workspace --all-features` 由 174 增為 194 個通過（ignored 由 5 個降為 1 個：`polish_corpus.rs` 的校正輸出，本來就是手動執行）。
- `CoreError` 新增兩個 variant（`VaultNotEntitled`、`NotInitialized`）；`PolishOptions` 等型別多了 uniffi derive，形狀不變。
- 沒有新增依賴或 crate：Cargo.lock 只多了 keepsay-cli → serde_json、wiremock 兩條 dev 邊；`cargo deny check`、`cargo vet --locked` 結果不變。
- 新增的網路出口：無。FFI 與 CLI 仍只經 `AnthropicClient`（`https://api.anthropic.com`）與 `ModelDownloader`（manifest URL）。
- 待人工決定（列入 ROADMAP）：
  - 是否批准 `uniffi/tokio`，把 `polish` 等改成真正的 async 匯出（目前為阻塞 + 每次呼叫一個 runtime）。
  - M2 的 Swift binding 產生器：`uniffi` 的 `cli` feature（或 workspace 外安裝）與 crate-type `staticlib` / `cdylib`。
  - Apple 取代流程第 3 步在 userPresence 下的提示行為（M2 實機）。
- 已知限制：`store_key` 會清掉 Rust 收到的 `String`，但 uniffi 從 Swift 傳入時使用的 `RustBuffer` 由 Swift 端配置與釋放，不在 core 能清零的範圍；Swift 端在送出後應立即清空 SecureField（PROPOSAL §7.1 已要求）。
- 既有測試觀察：`core/tests/api_errors.rs`（工作流 A）用 `header("x-api-key", FAKE_KEY)` 比對假 key 的值。這是假 key，不是使用者的 key，但與 CLAUDE.md「不斷言 key」的字面規則有落差；本次新寫的測試都只檢查 header 是否存在。是否改寫由人決定。

### PROPOSAL 待套用的修改（本次不改 `docs/specs/*`）

- §6.1：加入 `init(data_dir)`、`set_utc_offset(seconds)`、`import_model(id, path)`；`polish` 註明為阻塞呼叫（理由同上）且只有未 init 會回 `Err`；`download_model` 的 `progress` 是 foreign trait `DownloadProgress`；錯誤新增 `VaultNotEntitled`、`NotInitialized`。
- §6.2：Apple 取代 key 改為「pending 新增 → 刪舊 → 改名」三步驟；Windows `CredWriteW` 為單次覆寫。
- §6.9：core 提供 `panic_hook::install()`，`init` 會安裝。
- §11：新增 `no_key_load.rs`、`ffi_surface.rs`、`ffi_flow.rs`、`cli_api.rs`；金絲雀涵蓋 API / polish / 設定 / 預算 / 下載器 / FFI 路徑，並斷言金絲雀只出現在 `x-api-key`。
- §15 V6：補上「取代 key 的三步驟」與 userPresence 待驗證項。

## Status

Accepted。
