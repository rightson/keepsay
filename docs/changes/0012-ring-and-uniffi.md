# 0012 — TLS 加密實作改用 ring；加入 uniffi（MPL-2.0）

- Status：Accepted
- Date：2026-10-24
- 影響：core/Cargo.toml、Cargo.lock、deny.toml、supply-chain/config.toml、supply-chain/imports.lock、PROPOSAL §6.3、§14

## Context

1. 0005 採用 reqwest 0.13 的 `rustls` feature，加密實作因此是 aws-lc-rs / aws-lc-sys（C 與組語、需要 cmake / nasm 類工具鏈、建置期較長，也是依賴圖中最大的一塊 C 程式碼）。
2. uniffi 在 §14 白名單內，但授權 MPL-2.0 不在 deny.toml 的允許清單，0005 暫不加入（ROADMAP D-10）。

## Decision（使用者於 2026-10-24 批准）

### 1. rustls 加密實作改用 ring

reqwest 0.13.5 的 features（取自 locked 版本的 Cargo.toml）：

- `rustls` = `__rustls-aws-lc-rs` + `dep:rustls-platform-verifier` + `__rustls`
- `rustls-no-provider` = `dep:rustls-platform-verifier` + `__rustls`

選擇：

    reqwest = { version = "0.13.5", default-features = false, features = ["rustls-no-provider", "json"] }
    rustls  = { version = "0.23.46", default-features = false, features = ["ring", "std", "tls12"] }

- rustls 的直接依賴只用來選 ring provider（它原本就是 reqwest 的間接依賴，ring 0.17.14 也已在 Cargo.lock 中）。rustls 的預設 features 會開 `aws_lc_rs` 與 `prefer-post-quantum`（後者也依賴 aws-lc-rs），所以必須關閉預設 features。
- 驗證器維持 rustls-platform-verifier 0.7.1（OS 信任庫）。reqwest 以目前的 provider 呼叫 `rustls_platform_verifier::Verifier::new(provider)`，語意不變。
- deny.toml 的 bans 加入 `aws-lc-rs`、`aws-lc-sys`，防止日後被重新帶入。

證明 aws-lc 已不在依賴圖：

    cargo tree --workspace --all-features --target <T> -e normal,build,dev | grep -c aws-lc
    # T = all、x86_64-unknown-linux-gnu、aarch64-apple-darwin、aarch64-apple-ios、x86_64-pc-windows-msvc：皆為 0
    grep -c aws-lc Cargo.lock   # 0

同時從 Cargo.lock 消失的 crate：aws-lc-rs、aws-lc-sys、cfg_aliases、dunce、jobserver、lru-slab、pkg-config、quinn、quinn-proto、quinn-udp、rand_pcg、tinyvec、web-time（quinn 系列是 `__rustls-aws-lc-rs` 對 `quinn?/…` 的弱依賴殘留在 lock 中）。

#### 給 api workstream / 整合者：執行期必須安裝 provider（本紀錄不改 .rs）

`rustls-no-provider` 底下，reqwest 在 `ClientBuilder::build()` 時呼叫 `rustls::crypto::CryptoProvider::get_default()`；沒有安裝 process 預設 provider 時會 **panic**（`default_rustls_crypto_provider()`）。因此在建構任何 `reqwest::Client` 之前（`AnthropicClient` 與 `ModelDownloader` 都是），先執行：

    // Idempotent: Err only means a provider is already installed (only ring is linked).
    let _ = rustls::crypto::ring::default_provider().install_default();

建議：

- 放在 `AnthropicClient` 與 `ModelDownloader` 各自的建構函式中、`reqwest::Client::builder()` 之前。兩處都呼叫是安全的（第二次回傳 `Err`，忽略即可）。依賴圖中只有 ring 一個 provider，不會裝到別的 provider。
- 也可以抽成一個 `pub(crate) fn ensure_tls_provider()`，但不要放在 api 模組下讓 downloader 去 import：`core/tests/downloader.rs` 會掃描 `src/downloader.rs` 的 `api::` 字串。放在獨立的小模組（例如 `core/src/tls.rs`）或在兩處各寫一行。
- 不要改用 `use_preconfigured_tls` 自建 `ClientConfig`：那需要把 rustls-platform-verifier 列為直接依賴，且要自己重做 reqwest 的 TLS 版本、ALPN 設定。
- 已用 scratch crate 驗證（相同版本與 features）：第一次 `install_default()` 為 Ok、第二次為 Err，之後 `Client::builder().no_proxy().redirect(Policy::none()).build()` 成功。
- 目前的 stub 程式碼沒有建構 Client，所以現有測試不受影響；實作 client 時要有一個 wiremock 測試真的走過 `build()`（測試 endpoint 是 http，不經 TLS，但 `build()` 仍會初始化 rustls）。

### 2. 加入 uniffi 0.32.2（MPL-2.0）

    uniffi = { version = "0.32.2", default-features = false }

- 0.32.2 是 crates.io 目前最新的穩定版。proc-macro 寫法（`#[uniffi::export]`、`uniffi::setup_scaffolding!()`）不需要任何 feature，也不需要 UDL 與 build script。
- 關閉的 features：`cargo-metadata`（預設，只給 bindgen 用）、`cli`、`bindgen`、`build`。Swift binding 由獨立的 bindgen 執行檔產生（M2 時再決定放在哪個 crate 或工具，屆時另寫紀錄），不進 core library 的依賴。
- 未開 `tokio` feature。§6.1 的 `polish` 是 async，若 export 時需要 `#[uniffi::export(async_runtime = "tokio")]`，要開 `uniffi/tokio`，屬於新增 feature，需再經批准。另一種做法是 FFI 函式內部用 core 自己的 tokio runtime `block_on`，不需要該 feature。由整合者決定後回報。
- `crate-type` 不變（只有預設的 `lib`）。proc-macro 寫法不需要改 crate-type，`cargo test` / Tauri 直接以 Rust 依賴使用也不需要。Apple 端要打包 xcframework 時才需要 `staticlib`（以及 bindgen library mode 讀 metadata 用的 `cdylib`），屆時（M2）再加，並評估是否改由一個薄的 FFI crate 承擔，避免每次 `cargo test` 都多連結 staticlib / cdylib。
- 整合者要做的事（本紀錄不寫 ffi.rs）：`lib.rs` 加 `uniffi::setup_scaffolding!();`，`ffi.rs` 以 `#[uniffi::export]` 匯出 §6.1 的函式，仍然不得有回傳 key 的函式。
- 執行期新增的依賴：anyhow、uniffi_core（bytes、once_cell、static_assertions）。其餘（uniffi_macros、uniffi_meta、uniffi_internal_macros、uniffi_pipeline、toml、winnow、fs-err、camino、siphasher、heck、tempfile）只在編譯期的 proc-macro 中執行。toml 是 uniffi_macros 讀 `uniffi.toml` 用的，與 0005「manifest 用 JSON、不新增 TOML 依賴」不衝突：core 的程式碼不使用 toml。

授權：deny.toml 只對以下 6 個 crate 加 `MPL-2.0` 的 `[[licenses.exceptions]]`，一般 `allow` 清單不變，其他 MPL crate 仍會失敗：
uniffi、uniffi_core、uniffi_macros、uniffi_meta、uniffi_internal_macros、uniffi_pipeline。
MPL-2.0 是檔案層級的 copyleft，只要不修改 uniffi 本身的檔案，與 Apache-2.0 的 keepsay 相容。

### 3. cargo-vet

依 0011 的流程：`cargo vet regenerate imports`（重新抓取已設定的 6 個匯入來源，未新增來源）→ `cargo vet regenerate exemptions`。

- uniffi、uniffi_core、uniffi_macros、uniffi_meta、uniffi_internal_macros、uniffi_pipeline 由 mozilla 的 wildcard audit（safe-to-deploy）涵蓋，不需 exemption。camino、heck、static_assertions 也由匯入稽核涵蓋。
- 移除（crate 已不在依賴圖）：`aws-lc-rs 1.18.2`、`aws-lc-sys 0.46.0`、`jobserver 0.1.35`、`quinn 0.11.12`、`quinn-proto 0.11.19`。

新增 exemption（本次人工批准的一次性 exemption）：

| crate | 版本 | 準則 | 由誰引入 | 備註 |
|-------|------|------|----------|------|
| anyhow | 1.0.104 | safe-to-deploy | uniffi、uniffi_core（執行期） | 匯入稽核只到 1.0.103 |
| fs-err | 3.3.2 | safe-to-deploy | uniffi_macros（編譯期） |  |
| serde_spanned | 1.1.2 | safe-to-deploy | toml ← uniffi_macros | 匯入稽核只到 1.1.1 |
| siphasher | 1.0.4 | safe-to-deploy | uniffi_meta |  |
| tempfile | 3.3.0 | safe-to-deploy | uniffi_pipeline | delta 基準；lock 為 3.27.0，bytecode-alliance / mozilla 的 delta 稽核 3.3.0 → 3.27.0 |
| toml | 1.1.8+spec-1.1.0 | safe-to-deploy | uniffi_macros |  |
| toml_datetime | 1.1.2+spec-1.1.0 | safe-to-deploy | toml | 匯入稽核只到 1.1.1 |
| toml_parser | 1.1.5+spec-1.1.0 | safe-to-deploy | toml |  |
| toml_writer | 1.1.3+spec-1.1.0 | safe-to-deploy | toml | 匯入稽核只到 1.1.1 |
| winnow | 1.0.4 | safe-to-deploy | toml、toml_parser |  |

準則由 safe-to-run 升為 safe-to-deploy（原本只經 dev-dependency 使用，現在經 uniffi 的編譯期依賴進入正式建置）：

| crate | 版本 | 由誰引入 |
|-------|------|----------|
| autocfg | 1.5.1 | fs-err（build-dependency） |
| fastrand | 2.5.0 | tempfile |
| rustix | 1.1.5 | tempfile |
| linux-raw-sys | 0.12.1 | rustix |

tempfile 3.27.0 原本以 google 的 safe-to-run 稽核（3.10.1）加 mozilla delta 涵蓋，現在需要 safe-to-deploy，所以多一筆 3.3.0 基準。

### 數量

| | 0011 之後 | 本次之後 |
|---|---|---|
| Cargo.lock 第三方 crate 版本 | 224 | 229 |
| 完全由匯入稽核涵蓋 | 47 | 47 |
| 部分涵蓋（匯入 delta + exemption 基準） | 18 | 19 |
| 直接以 exemption 涵蓋 | 159 | 163 |
| config.toml 的 exemption 條目 | 176 | 181（−5，+10） |
| 其中 safe-to-run | 12 | 8 |

`cargo vet --locked`：`Vetting Succeeded (47 fully audited, 19 partially audited, 163 exempted)`。

## 驗證

- `cargo deny check`：advisories ok, bans ok, licenses ok, sources ok。
- `cargo fmt --all --check`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`：通過。
- `cargo test --workspace`、`cargo test -p keepsay-core --features test-endpoint,test-support`、`cargo test --workspace --all-features`：通過（目前為 stub 程式碼）。

## Consequences

- 建置不再需要 aws-lc-sys 的 C / 組語工具鏈；ring 仍含 C 與組語，但體積小得多，且已是既有依賴。
- 失去 aws-lc-rs 的 post-quantum 金鑰交換偏好（`prefer-post-quantum`）。ring provider 支援 X25519、P-256、P-384，TLS 1.2 / 1.3 皆可連 api.anthropic.com。
- 忘記安裝 provider 會在建構 client 時 panic，不會靜默改用其他實作；實作 client 時以測試覆蓋。
- uniffi 的編譯期依賴（toml、winnow 等）多了約 10 筆 exemption，待人工稽核。
