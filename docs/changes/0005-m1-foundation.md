# 0005 — M1 地基：unsafe 範圍、免依賴的 WAV 與 manifest、共用型別與模組分工

- Status：Accepted
- Date：2026-10-24
- 影響：PROPOSAL v0.3 → v0.4（§5.2、§6.2、§6.3、§6.7、§8、§11、§14）、THREAT_MODEL T1、CLAUDE.md、ROADMAP（D-2、D-3、D-6，US-M1-03 等）、Cargo.toml（workspace lint、core、cli）、deny.toml、.github/workflows/ci.yml、core/src/**、models/manifest.json

## Context

M1 要由五個 agent 平行實作 keepsay-core 與 CLI。開工前有三件事卡住（ROADMAP D-2、D-3、D-6），而且共用檔案（Cargo.toml、Cargo.lock、lib.rs、錯誤型別）若由多人同時修改，一定會衝突。

1. D-2：Windows vault 要呼叫 CredWriteW / CredReadW，必須用 `unsafe`，但 workspace 是 `unsafe_code = "forbid"`。
2. D-3：§14 白名單沒有 TOML 解析（manifest）與 WAV 解碼（CLI）的套件。
3. D-6：§6.2 的 Windows CredMan target 仍是 `keepsay/anthropic`，與 0004 的 `dev.rightson.keepsay.*` 識別字不一致。

## Decision

使用者於 2026-10-24 批准以下事項。

### 1. `unsafe` 只開放給 `core/src/vault/windows.rs`

- workspace lint `unsafe_code` 由 `"forbid"` 改為 `"deny"`。`forbid` 無法在局部以 `allow` 覆寫，所以必須降一級。
- 只有 `core/src/vault/windows.rs` 在檔案開頭寫 `#![allow(unsafe_code)]`。每個 `unsafe` 區塊都要有 `// SAFETY:` 註解，並保持最小範圍。
- CI 新增 `unsafe-scope` job（純 shell，不新增 action），以下任一條成立就失敗：
  - 根目錄 `Cargo.toml` 不再是 `unsafe_code = "deny"`。
  - 任何 git 追蹤的 `*.rs`、`*Cargo.toml`、`.cargo/*` 中，除了根目錄 `Cargo.toml` 與 `core/src/vault/windows.rs` 以外，出現 `unsafe_code` 字樣。這同時擋下 `allow`、`expect`、`cfg_attr` 與成員 crate 的 `[lints]` 覆寫。
  - `windows.rs` 內的 `allow(unsafe_code)` 不是剛好一次。
- 範圍只到 vault。M3 的 SendInput、剪貼簿格式、IME 等 Win32 FFI 不在本次批准內，屆時另開變更紀錄。

### 2. WAV 與 manifest 不新增依賴

- CLI 自行實作 PCM WAV 讀取：16-bit 整數與 32-bit 浮點，立體聲（或多聲道）平均混成單聲道。其他格式回 `CoreError::AudioFormat`。重取樣到 16 kHz 也由 CLI 自行處理，或要求輸入就是 16 kHz（由工作流 E 決定並寫入測試）。
- `models/manifest.toml` 改為 `models/manifest.json`，以白名單內的 serde_json 解析，結構體加 `deny_unknown_fields`。JSON 不能寫註解，schema 說明移到 `core/src/downloader.rs` 的模組文件與 PROPOSAL §8。manifest 以 `include_str!` 編譯進 binary。
- D-3 的第三項「App Support 目錄定位」不需要依賴：core 的 `Config::load` / `Budget::load` 一律由呼叫端傳入路徑，目錄由 Swift / Tauri / CLI 決定。

### 3. CredMan target 改為 `dev.rightson.keepsay/anthropic`

與 Keychain service `dev.rightson.keepsay.apikey` 同一個前綴。目前沒有任何使用者資料，不需要遷移。0004 中「`keepsay/anthropic` 不變」的說法由本紀錄取代。

### 4. M1 依賴（§14 白名單內）

| crate | 版本 | 位置 | features |
|-------|------|------|----------|
| reqwest | 0.13.5 | core | `default-features = false`，`rustls`、`json` |
| tokio | 1.54.1 | core、cli | `default-features = false`，`rt`、`macros`、`time` |
| serde_json | 1.0.154 | core（由 dev 移到 normal） | 預設 |
| sha2 | 0.11.1 | core | `default-features = false` |
| tracing | 0.1.44 | core、cli | `default-features = false`，`std` |
| zeroize | 1.9.1 | core | 預設 |
| security-framework | 3.7.0 | core，僅 `cfg(target_vendor = "apple")` | `default-features = false`，`OSX_10_15`（`use_protected_keychain`，ThisDeviceOnly 需要） |
| windows | 0.62.2 | core，僅 `cfg(windows)` | `default-features = false`，`Win32_Foundation`、`Win32_Security_Credentials` |
| whisper-rs | 0.16.0 | core，optional（feature `whisper`） | 無；Apple target 加 `metal` |
| tracing-subscriber | 0.3.23 | cli；core 的 dev-dependency | `default-features = false`，`fmt`、`std` |
| wiremock | 0.6.5 | core dev | 預設 |
| proptest | 1.12.0 | core dev | `default-features = false`，`std` |
| tempfile | 3.27.0 | core、cli dev | 預設 |

已用 `cargo tree -e features` 確認：

- 沒有 native-tls、openssl、hyper-tls，也沒有 `system-proxy`（`hyper-util/client-proxy-system`），所以不會讀系統 proxy 設定。
- tokio 沒有 `rt-multi-thread`（含 dev 圖）。`net`、`io-util`、`sync` 由 reqwest / hyper 帶入。wiremock 測試請用 `#[tokio::test]`（current-thread）。
- reqwest 0.13 的 `rustls` feature 以 rustls-platform-verifier 為預設驗證器，加密實作是 aws-lc-rs。所以 rustls-platform-verifier 不另列為直接依賴；若列了反而是未使用的依賴。
- 在 Apple target 上，rustls-platform-verifier 會打開 security-framework 的預設 features（TLS 用的 `alpn`、`session-tickets`），這是 feature unification，不影響 Keychain 程式碼。

不加入的項目：

- uniffi（0.32.2）：授權是 MPL-2.0，不在 deny.toml 允許清單；而且 FFI 層依賴所有 M1 模組，等各工作流完成後再接（US-M1-22）。新增 ROADMAP D-10 待決定。`core/src/ffi.rs` 維持只有說明文件。

### 5. deny.toml 逐 crate 授權例外

白名單內的依賴帶進了三個不在一般允許清單的授權，都屬寬鬆授權。以 `[[licenses.exceptions]]` 只對指定 crate 放行，同授權的新 crate 仍會失敗：

- whisper-rs、whisper-rs-sys：Unlicense。
- webpki-root-certs：CDLA-Permissive-2.0（Mozilla 根憑證清單資料，由 rustls-platform-verifier 帶入）。

### 6. 新 feature `test-support`

- `vault::mock::MockVault` 改為 `cfg(any(test, feature = "test-support"))`，讓 `core/tests/` 的整合測試可以用。
- release build 啟用時 `compile_error!`，與 `test-endpoint` 相同；CI release-feature-guard 同時檢查兩者不在一般依賴圖中、release build 會拒絕兩者。
- 用到 `MockVault` 或 `with_test_endpoint` 的整合測試檔案，第一行寫 `#![cfg(all(feature = "test-support", feature = "test-endpoint"))]`（只需要其一就寫其一），預設 `cargo test` 會跳過，CI 的 `--all-features` 與 test-os job 會跑。

### 7. 共用型別與簽章

本次把 §6.1 需要的型別與函式簽章全部定好，非 trivial 的本體是 `todo!("M1: <module> — …")`。workspace 沒有啟用 `clippy::todo`，所以用 `todo!`。未使用的參數以 `let _ = (…);` 消掉警告，實作時一併移除。

- `CoreError`（`api/error.rs`）：`Copy + Eq`，variant 只帶固定描述、HTTP 狀態碼或 `io::ErrorKind`。`From<io::Error>` 只保留 kind；`From<reqwest::Error>` 只保留分類（逾時、狀態碼、解碼、網路），丟掉訊息與 URL。
- `Fingerprint`（`vault`）：只能由 `vault::fingerprint` 建立，是唯一可以離開 core 的 key 衍生值。
- `KeyVault: Send + Sync`；`vault::platform_vault()` 依平台回傳 `KeychainVault` / `CredManVault`，其他平台回 `Unsupported`。
- `AnthropicClient::new(proxy)`、`with_test_endpoint(url)` 回傳 `Result`；`verify_key(&SecretString)`（尚未存入時驗證）；`messages(&dyn KeyVault, &MessagesRequest)`，在使用當下才從 vault 讀 key、用完就 drop。core 內只有 `AnthropicClient` 呼叫 `KeyVault::load`。
- `polish::polish(client, vault, transcript, opts, &BudgetStatus) -> PolishResult`：永遠不失敗；用量由呼叫端記入 `Budget`。
- `Budget`（`budget.json`）、`YearMonth`（注入時間以測試每月歸零）、`Config`（`schema_version`、mode、translate_to、model、stt_model、glossary、hotkeys、prices、budget、proxy）。
- 會碰到逐字稿、詞彙表、模型輸出或 proxy 的型別，`Debug` 改為手寫，只印長度或是否有值：`MessagesRequest`、`Message`、`MessagesResponse`、`ContentBlock`、`PolishOptions`、`PolishResult`、`SttOptions`、`Config`。

## 模組分工（M1 平行實作）

每個工作流只改自己擁有的檔案。下表「共用」欄的檔案凍結：除了在 `[dev-dependencies]` 加入白名單內、本紀錄已列出的 crate，一律不改。需要改共用檔案或其他工作流的公開簽章時，先回報協調，不要自己改。可以在自己的模組裡新增 private 項目或新的 `pub` 項目。

| 工作流 | 擁有的檔案 | 對應故事 |
|--------|-----------|----------|
| A api | `core/src/api/client.rs`、`core/src/api/types.rs`、`core/src/api/mod.rs`、`core/tests/egress.rs`、`core/tests/request_type.rs`、新增的 api wiremock 測試（例如 `core/tests/api_errors.rs`） | US-M1-01、07、08 |
| B polish | `core/src/polish/*`（含 `modes.rs` 的本體；公開型別形狀不改）、新增的 `core/tests/polish_*.rs` | US-M1-09、10、11、12、13、16（polish 端） |
| C vault | `core/src/vault/*`、`core/tests/canary.rs` 中 vault 相關部分、新增的 `core/tests/vault_*.rs` | US-M1-02、04、05、06、21（vault 端） |
| D downloader + budget + config | `core/src/downloader.rs`、`core/src/budget.rs`、`core/src/config.rs`、`models/manifest.json`、`core/tests/downloader.rs`、新增的 `core/tests/budget_*.rs`、`config_*.rs` | US-M1-14、15、18、19 |
| E stt + cli | `core/src/stt/*`、`cli/*`（Cargo.toml 除外） | US-M1-16（STT 端）、17、23、25 |

共用（凍結）：根目錄 `Cargo.toml`、`core/Cargo.toml`、`cli/Cargo.toml`、`Cargo.lock`、`core/src/lib.rs`、`core/src/api/error.rs`（`CoreError`）、`core/src/ffi.rs`、`deny.toml`、`supply-chain/*`、`.github/workflows/ci.yml`、`docs/specs/*`（各工作流只更新 ROADMAP 中自己的故事狀態）。

給各工作流的注意事項：

- A：`From<reqwest::Error> for CoreError` 已在 error.rs，401/403、429/529、400 額度不足要在 client 內先依狀態碼對應，不要依賴這個 From。`validate_base_url` 不改。
- B：`FallbackReason::from_error` 已定義；`PolishResult::raw` 是退回 raw 的建構子。硬上限時不送請求。
- C：`#![allow(unsafe_code)]` 只能在 `windows.rs`，不要搬到 `mod.rs`。Apple 端的 ThisDeviceOnly 取決於 V6；若需要調整 security-framework features，先回報。
- D：`downloader.rs` 的非註解程式碼不得出現 `vault`、`secrecy`、`SecretString`、`AnthropicClient`、`api::`（文字掃描測試）。`Config` / `Budget` 的路徑由呼叫端傳入。
- E：載入模型前呼叫 `whisper_rs::install_logging_hooks()`（未開 log/tracing backend 時等於丟棄 whisper.cpp 的輸出）。CLI 的 stdout 輸出需要局部 `#[allow(clippy::print_stdout)]`。key 一律經由 vault 取得。
- 所有人：不得在測試、錯誤訊息、`Debug` 或註解中印出或斷言 key、逐字稿、模型輸出、詞彙表；只斷言長度與結構。

## Consequences

- `cargo vet --locked` 目前失敗：213 個依賴未稽核（新增依賴所致）。依 CLAUDE.md，agent 不新增 exemption、不 certify、不 trust；由人處理（US-M1-24、D-4）。CI 的 vet job 在那之前會是紅的。
- CLI 預設開 `whisper`，所以 `cargo build --workspace` / `cargo test --workspace` 會編 whisper.cpp（需要 cmake、clang；ubuntu-latest 有）。只測 core 時用 `-p keepsay-core`。
- macOS 與 Windows runner 會編譯並測試 keepsay-core（`--features test-endpoint,test-support`，不含 whisper）。開發環境是 Linux，Windows 的 `windows` crate features 只核對過原始碼（CredWriteW / CredReadW / CredDeleteW / CredFree 在 `Win32_Security_Credentials`，`CREDENTIALW` 需要 `Win32_Foundation` 的 `FILETIME`），尚未實際編譯。
- D-2 只解決 vault 的部分；D-3、D-6 解決。新增 D-10（uniffi 的 MPL-2.0 授權）。
- 授權例外（Unlicense、CDLA-Permissive-2.0）屬於依賴批准的延伸，列在這裡供人工複核。
