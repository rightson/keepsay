# 0011 — cargo-vet：匯入可信組織的稽核，其餘一次性 exemption

- Status：Accepted
- Date：2026-10-24
- 影響：supply-chain/config.toml、supply-chain/imports.lock、CLAUDE.md（vet 規則）、ROADMAP（US-M1-24、D-4）

## Context

0005 新增 M1 依賴後，`cargo vet --locked` 有 213 個未稽核依賴，CI 的 vet job 是紅的（US-M1-24、D-4）。CLAUDE.md 禁止 agent 執行 `cargo vet certify`、`cargo vet trust`，也禁止 agent 自行新增 exemption。D-4（要信任哪些組織的稽核）是人工決定。

## Decision

使用者選擇 D-4 的選項 (a)：

1. 匯入 cargo-vet 內建 registry 中知名組織公開的稽核（`cargo vet import <name>`，使用 registry 的正式 URL）。這只是讀取他人的 `audits.toml`，不是 `certify`（自己宣稱稽核過）也不是 `trust`（信任某個發佈者的所有版本）。

   | registry 名稱 | URL | imports.lock 內實際用到的稽核數 |
   |---------------|-----|----------------------------------|
   | mozilla | https://raw.githubusercontent.com/mozilla/supply-chain/main/audits.toml | 102 audits + 2 wildcard |
   | google | https://raw.githubusercontent.com/google/supply-chain/main/audits.toml（Google 的彙整檔，涵蓋 chromium/supply-chain 與 google/rust-crate-audits） | 24 |
   | bytecode-alliance | https://raw.githubusercontent.com/bytecodealliance/wasmtime/main/supply-chain/audits.toml | 29 audits + 1 wildcard |
   | isrg | https://raw.githubusercontent.com/divviup/libprio-rs/main/supply-chain/audits.toml | 20 |
   | zcash | https://raw.githubusercontent.com/zcash/rust-ecosystem/main/supply-chain/audits.toml | 12 |
   | embark-studios | https://raw.githubusercontent.com/EmbarkStudios/rust-ecosystem/main/audits.toml | 3 |

2. 匯入後仍未涵蓋的依賴，由使用者事先批准一次性 exemption。以 `cargo vet regenerate exemptions` 產生：它只為仍未涵蓋的版本產生 exemption，同時移除已被匯入稽核涵蓋的舊 exemption（prune）。下表列出每一筆。
3. 往後新增 exemption 只能經由人工批准的 `docs/changes/` 紀錄；agent 不得自行新增，也不得 certify / trust（CLAUDE.md 同步修改）。

### 數量

- `Cargo.lock` 的第三方 crate 版本：224 個。
- 完全由匯入稽核涵蓋：47 個。
- 部分涵蓋（匯入的 delta 稽核 + 舊版本的 exemption 當基準）：18 個（17 筆 delta 基準 exemption；getrandom 0.2.17 與 0.4.3 共用 0.2.2 基準）。
- 直接以 exemption 涵蓋：159 個。
- `config.toml` 的 exemption 條目：176 筆（159 + 17 筆 delta 基準）；safe-to-deploy 164 筆、safe-to-run 12 筆。
- 本次前後：exemption 15 筆 → 176 筆；未稽核 213 → 0；`cargo vet --locked`：`Vetting Succeeded (47 fully audited, 18 partially audited, 159 exempted)`。

### 舊 exemption 的變化（prune）

- 移除（已被匯入稽核涵蓋）：`quote 1.0.47`、`zeroize 1.9.1`。
- 準則由 safe-to-run 升為 safe-to-deploy（M1 後這些 crate 進入正式建置）：`itoa 1.0.18`、`memchr 2.8.3`、`serde_json 1.0.154`、`zmij 1.0.23`。
- 保留：`proc-macro2`、`secrecy`、`serde`、`serde_core`、`serde_derive`、`syn 3.0.9`、`thiserror`、`thiserror-impl`、`unicode-ident`。

### Exemption 清單

「由誰引入」欄是工作區直接依賴中、在全 feature / 全 target 依賴圖上可達該 crate 者（`cargo metadata --all-features` 計算，可能高估；平台限定的 crate 如 `windows-*`、`core-foundation` 實際只在對應 target 編譯）。`direct` 表示它本身就是工作區的直接依賴。「delta 基準」表示這筆 exemption 的版本不在 Cargo.lock 中，它是匯入的 delta 稽核（基準 → lock 版本）的起點。

| crate | 版本 | 準則 | 由誰引入 | 備註 |
|-------|------|------|----------|------|
| aho-corasick | 1.1.5 | safe-to-deploy | via whisper-rs, wiremock |  |
| autocfg | 1.5.1 | safe-to-run | via proptest |  |
| aws-lc-rs | 1.18.2 | safe-to-deploy | via reqwest |  |
| aws-lc-sys | 0.46.0 | safe-to-deploy | via reqwest |  |
| base64 | 0.23.1 | safe-to-deploy | via reqwest, wiremock |  |
| bitflags | 2.13.2 | safe-to-deploy | via proptest, reqwest, security-framework, tempfile +1 |  |
| block-buffer | 0.12.1 | safe-to-deploy | via sha2 |  |
| bytes | 1.12.1 | safe-to-deploy | via reqwest, tokio, wiremock |  |
| cc | 1.10.0 | safe-to-deploy | via reqwest, whisper-rs |  |
| cfg-if | 1.0.5 | safe-to-deploy | via proptest, reqwest, sha2, tempfile +2 |  |
| clang-sys | 1.9.1 | safe-to-deploy | via whisper-rs |  |
| cmake | 0.1.58 | safe-to-deploy | via reqwest, whisper-rs |  |
| combine | 4.6.9 | safe-to-deploy | via reqwest |  |
| core-foundation | 0.10.1 | safe-to-deploy | via reqwest, security-framework |  |
| crypto-common | 0.2.2 | safe-to-deploy | via sha2 |  |
| deadpool | 0.12.3 | safe-to-run | via wiremock |  |
| deadpool-runtime | 0.1.4 | safe-to-run | via wiremock |  |
| digest | 0.11.3 | safe-to-deploy | via sha2 |  |
| displaydoc | 0.2.7 | safe-to-deploy | via reqwest, wiremock |  |
| either | 1.19.0 | safe-to-deploy | via whisper-rs |  |
| equivalent | 1.0.3 | safe-to-deploy | via reqwest, wiremock |  |
| fastrand | 2.5.0 | safe-to-run | via tempfile |  |
| find-msvc-tools | 0.1.0 | safe-to-deploy | via reqwest, whisper-rs | delta base; lock has 0.1.15 |
| fs_extra | 1.3.0 | safe-to-deploy | via reqwest, whisper-rs |  |
| futures | 0.3.34 | safe-to-run | via wiremock |  |
| futures-channel | 0.3.34 | safe-to-deploy | via proptest, reqwest, tempfile, whisper-rs +1 |  |
| futures-executor | 0.3.34 | safe-to-run | via wiremock |  |
| futures-io | 0.3.34 | safe-to-deploy | via proptest, reqwest, tempfile, whisper-rs +1 |  |
| futures-macro | 0.3.34 | safe-to-deploy | via proptest, reqwest, tempfile, whisper-rs +1 |  |
| futures-sink | 0.3.34 | safe-to-deploy | via proptest, reqwest, tempfile, whisper-rs +1 |  |
| futures-task | 0.3.21 | safe-to-deploy | via proptest, reqwest, tempfile, whisper-rs +1 | delta base; lock has 0.3.34 |
| futures-util | 0.3.21 | safe-to-deploy | via proptest, reqwest, tempfile, whisper-rs +1 | delta base; lock has 0.3.34 |
| getrandom | 0.2.2 | safe-to-deploy | via proptest, reqwest, tempfile, whisper-rs | delta base; lock has 0.2.17, 0.4.3 |
| glob | 0.3.4 | safe-to-deploy | via whisper-rs |  |
| h2 | 0.4.20 | safe-to-deploy | via reqwest, wiremock |  |
| hashbrown | 0.14.2 | safe-to-deploy | via reqwest, wiremock | delta base; lock has 0.17.1 |
| hermit-abi | 0.5.3 | safe-to-run | via wiremock |  |
| http | 1.5.0 | safe-to-deploy | via reqwest, wiremock |  |
| http-body | 1.1.0 | safe-to-deploy | via reqwest, wiremock |  |
| http-body-util | 0.1.5 | safe-to-deploy | via reqwest, wiremock |  |
| httparse | 1.10.1 | safe-to-deploy | via reqwest, wiremock |  |
| hybrid-array | 0.4.15 | safe-to-deploy | via sha2 |  |
| hyper | 1.12.0 | safe-to-deploy | via reqwest, wiremock |  |
| hyper-rustls | 0.27.10 | safe-to-deploy | via reqwest |  |
| hyper-util | 0.1.21 | safe-to-deploy | via reqwest, wiremock |  |
| icu_collections | 2.4.0 | safe-to-deploy | via reqwest, wiremock |  |
| icu_locale_core | 2.4.0 | safe-to-deploy | via reqwest, wiremock |  |
| icu_normalizer | 2.4.0 | safe-to-deploy | via reqwest, wiremock |  |
| icu_normalizer_data | 2.4.0 | safe-to-deploy | via reqwest, wiremock |  |
| icu_properties | 2.4.0 | safe-to-deploy | via reqwest, wiremock |  |
| icu_properties_data | 2.4.0 | safe-to-deploy | via reqwest, wiremock |  |
| icu_provider | 2.4.0 | safe-to-deploy | via reqwest, wiremock |  |
| idna_adapter | 1.2.2 | safe-to-deploy | via reqwest, wiremock |  |
| indexmap | 2.14.2 | safe-to-deploy | via reqwest, wiremock |  |
| ipnet | 2.5.0 | safe-to-deploy | via reqwest, wiremock | delta base; lock has 2.12.2 |
| itertools | 0.13.0 | safe-to-deploy | via whisper-rs |  |
| itoa | 1.0.18 | safe-to-deploy | via reqwest, serde_json, wiremock |  |
| jni | 0.22.4 | safe-to-deploy | via reqwest |  |
| jni-macros | 0.22.4 | safe-to-deploy | via reqwest |  |
| jni-sys | 0.4.1 | safe-to-deploy | via reqwest |  |
| jni-sys-macros | 0.4.1 | safe-to-deploy | via reqwest |  |
| jobserver | 0.1.35 | safe-to-deploy | via reqwest, whisper-rs |  |
| js-sys | 0.3.106 | safe-to-deploy | via proptest, reqwest, tempfile, whisper-rs |  |
| lazy_static | 1.5.1 | safe-to-deploy | via tracing-subscriber, wiremock |  |
| libc | 0.2.191 | safe-to-deploy | via proptest, reqwest, security-framework, sha2 +4 |  |
| libloading | 0.8.9 | safe-to-deploy | via whisper-rs |  |
| linux-raw-sys | 0.12.1 | safe-to-run | via tempfile |  |
| litemap | 0.8.3 | safe-to-deploy | via reqwest, wiremock |  |
| log | 0.4.34 | safe-to-deploy | via reqwest, whisper-rs, wiremock |  |
| memchr | 2.8.3 | safe-to-deploy | via proptest, reqwest, serde_json, tempfile +2 |  |
| minimal-lexical | 0.2.1 | safe-to-deploy | via whisper-rs |  |
| mio | 1.2.4 | safe-to-deploy | via reqwest, tokio, wiremock |  |
| once_cell | 1.12.0 | safe-to-deploy | via proptest, reqwest, tempfile, tracing +3 | delta base; lock has 1.21.4 |
| openssl-probe | 0.2.1 | safe-to-deploy | via reqwest |  |
| pin-project-lite | 0.2.17 | safe-to-deploy | via proptest, reqwest, tempfile, tokio +3 |  |
| potential_utf | 0.1.6 | safe-to-deploy | via reqwest, wiremock |  |
| prettyplease | 0.2.37 | safe-to-deploy | via whisper-rs |  |
| proc-macro2 | 1.0.107 | safe-to-deploy | via proptest, reqwest, serde, serde_json +6 |  |
| proptest | 1.12.0 | safe-to-run | direct |  |
| quinn | 0.11.12 | safe-to-deploy | via reqwest |  |
| quinn-proto | 0.11.19 | safe-to-deploy | via reqwest |  |
| r-efi | 5.2.0 | safe-to-deploy | via proptest, reqwest, tempfile, whisper-rs | delta base; lock has 6.0.0 |
| rand_xorshift | 0.5.0 | safe-to-run | via proptest |  |
| regex | 1.13.1 | safe-to-deploy | via whisper-rs, wiremock |  |
| regex-automata | 0.4.18 | safe-to-deploy | via whisper-rs, wiremock |  |
| regex-syntax | 0.8.11 | safe-to-deploy | via proptest, whisper-rs, wiremock |  |
| reqwest | 0.13.5 | safe-to-deploy | direct |  |
| ring | 0.17.11 | safe-to-deploy | via reqwest | delta base; lock has 0.17.14 |
| rustc-hash | 2.1.3 | safe-to-deploy | via reqwest, whisper-rs |  |
| rustix | 1.1.5 | safe-to-run | via tempfile |  |
| rustls | 0.23.37 | safe-to-deploy | via reqwest | delta base; lock has 0.23.46 |
| rustls-native-certs | 0.8.5 | safe-to-deploy | via reqwest |  |
| rustls-pki-types | 1.15.1 | safe-to-deploy | via reqwest |  |
| rustls-platform-verifier | 0.7.1 | safe-to-deploy | via reqwest |  |
| rustls-platform-verifier-android | 0.2.0 | safe-to-deploy | via reqwest |  |
| rustls-webpki | 0.103.10 | safe-to-deploy | via reqwest | delta base; lock has 0.103.16 |
| rustversion | 1.0.23 | safe-to-deploy | via proptest, reqwest, tempfile, whisper-rs |  |
| same-file | 1.0.6 | safe-to-deploy | via reqwest |  |
| schannel | 0.1.30 | safe-to-deploy | via reqwest |  |
| secrecy | 0.10.3 | safe-to-deploy | direct |  |
| security-framework | 3.7.0 | safe-to-deploy | direct; via reqwest |  |
| security-framework-sys | 2.17.0 | safe-to-deploy | via reqwest, security-framework |  |
| serde | 1.0.229 | safe-to-deploy | direct; via reqwest, serde_json, wiremock |  |
| serde_core | 1.0.229 | safe-to-deploy | via reqwest, serde, serde_json, wiremock |  |
| serde_derive | 1.0.229 | safe-to-deploy | via reqwest, serde, serde_json, wiremock |  |
| serde_json | 1.0.154 | safe-to-deploy | direct; via reqwest, wiremock |  |
| sha2 | 0.11.1 | safe-to-deploy | direct |  |
| simd_cesu8 | 1.2.0 | safe-to-deploy | via reqwest |  |
| slab | 0.4.12 | safe-to-deploy | via proptest, reqwest, tempfile, whisper-rs +1 |  |
| smallvec | 1.16.4 | safe-to-deploy | via reqwest, wiremock |  |
| socket2 | 0.6.5 | safe-to-deploy | via reqwest, tokio, wiremock |  |
| stable_deref_trait | 1.2.1 | safe-to-deploy | via reqwest, wiremock |  |
| syn | 2.0.119 | safe-to-deploy | via reqwest, whisper-rs, windows |  |
| syn | 3.0.9 | safe-to-deploy | via proptest, reqwest, serde, serde_json +5 |  |
| synstructure | 0.14.0 | safe-to-deploy | via reqwest, wiremock |  |
| thiserror | 2.0.21 | safe-to-deploy | direct; via reqwest |  |
| thiserror-impl | 2.0.21 | safe-to-deploy | via reqwest, thiserror |  |
| thread_local | 1.1.10 | safe-to-deploy | via tracing-subscriber |  |
| tinystr | 0.8.5 | safe-to-deploy | via reqwest, wiremock |  |
| tokio | 1.54.1 | safe-to-deploy | direct; via reqwest, wiremock |  |
| tokio-macros | 2.7.2 | safe-to-deploy | via reqwest, tokio, wiremock |  |
| tokio-rustls | 0.26.6 | safe-to-deploy | via reqwest |  |
| tokio-util | 0.7.20 | safe-to-deploy | via reqwest, wiremock |  |
| tower | 0.5.3 | safe-to-deploy | via reqwest |  |
| tower-http | 0.6.11 | safe-to-deploy | via reqwest |  |
| tower-layer | 0.1.0 | safe-to-deploy | via reqwest | delta base; lock has 0.3.3 |
| tower-service | 0.3.1 | safe-to-deploy | via reqwest, wiremock | delta base; lock has 0.3.3 |
| tracing | 0.1.44 | safe-to-deploy | direct; via reqwest, wiremock |  |
| tracing-subscriber | 0.3.23 | safe-to-deploy | direct |  |
| typenum | 1.20.1 | safe-to-deploy | via sha2 |  |
| unicode-ident | 1.0.27 | safe-to-deploy | via proptest, reqwest, serde, serde_json +6 |  |
| url | 2.5.8 | safe-to-deploy | via reqwest, wiremock |  |
| walkdir | 2.5.0 | safe-to-deploy | via reqwest |  |
| want | 0.3.2 | safe-to-deploy | via reqwest, wiremock |  |
| wasi | 0.11.0+wasi-snapshot-preview1 | safe-to-deploy | via reqwest, tokio, wiremock | delta base; lock has 0.11.1+wasi-snapshot-preview1 |
| wasm-bindgen | 0.2.129 | safe-to-deploy | via proptest, reqwest, tempfile, whisper-rs |  |
| wasm-bindgen-futures | 0.4.79 | safe-to-deploy | via reqwest |  |
| wasm-bindgen-macro | 0.2.99 | safe-to-deploy | via proptest, reqwest, tempfile, whisper-rs | delta base; lock has 0.2.129 |
| wasm-bindgen-macro-support | 0.2.129 | safe-to-deploy | via proptest, reqwest, tempfile, whisper-rs |  |
| wasm-bindgen-shared | 0.2.100 | safe-to-deploy | via proptest, reqwest, tempfile, whisper-rs | delta base; lock has 0.2.129 |
| web-sys | 0.3.106 | safe-to-deploy | via reqwest |  |
| webpki-root-certs | 1.0.11 | safe-to-deploy | via reqwest |  |
| whisper-rs | 0.16.0 | safe-to-deploy | direct |  |
| whisper-rs-sys | 0.15.0 | safe-to-deploy | via whisper-rs |  |
| winapi-util | 0.1.9 | safe-to-deploy | via reqwest | delta base; lock has 0.1.11 |
| windows | 0.62.2 | safe-to-deploy | direct |  |
| windows-collections | 0.3.2 | safe-to-deploy | via windows |  |
| windows-core | 0.62.2 | safe-to-deploy | via windows |  |
| windows-future | 0.3.2 | safe-to-deploy | via windows |  |
| windows-implement | 0.60.2 | safe-to-deploy | via windows |  |
| windows-interface | 0.59.3 | safe-to-deploy | via windows |  |
| windows-numerics | 0.3.1 | safe-to-deploy | via windows |  |
| windows-result | 0.4.1 | safe-to-deploy | via windows |  |
| windows-strings | 0.5.1 | safe-to-deploy | via windows |  |
| windows-sys | 0.52.0 | safe-to-deploy | via reqwest |  |
| windows-sys | 0.61.2 | safe-to-deploy | via reqwest, tempfile, tokio, wiremock |  |
| windows-targets | 0.52.6 | safe-to-deploy | via reqwest |  |
| windows-threading | 0.2.1 | safe-to-deploy | via windows |  |
| windows_aarch64_gnullvm | 0.52.6 | safe-to-deploy | via reqwest |  |
| windows_aarch64_msvc | 0.52.6 | safe-to-deploy | via reqwest |  |
| windows_i686_gnu | 0.52.6 | safe-to-deploy | via reqwest |  |
| windows_i686_gnullvm | 0.52.6 | safe-to-deploy | via reqwest |  |
| windows_i686_msvc | 0.52.6 | safe-to-deploy | via reqwest |  |
| windows_x86_64_gnu | 0.52.6 | safe-to-deploy | via reqwest |  |
| windows_x86_64_gnullvm | 0.52.6 | safe-to-deploy | via reqwest |  |
| windows_x86_64_msvc | 0.52.6 | safe-to-deploy | via reqwest |  |
| wiremock | 0.6.5 | safe-to-run | direct |  |
| writeable | 0.6.4 | safe-to-deploy | via reqwest, wiremock |  |
| yoke | 0.8.4 | safe-to-deploy | via reqwest, wiremock |  |
| yoke-derive | 0.8.4 | safe-to-deploy | via reqwest, wiremock |  |
| zerofrom | 0.1.8 | safe-to-deploy | via reqwest, wiremock |  |
| zerofrom-derive | 0.1.8 | safe-to-deploy | via reqwest, wiremock |  |
| zerotrie | 0.2.5 | safe-to-deploy | via reqwest, wiremock |  |
| zerovec | 0.11.9 | safe-to-deploy | via reqwest, wiremock |  |
| zerovec-derive | 0.11.7 | safe-to-deploy | via reqwest, wiremock |  |
| zmij | 1.0.23 | safe-to-deploy | via reqwest, serde_json, wiremock |  |

## Consequences

- CI 的 vet job 轉綠。`cargo vet --locked` 只讀 `imports.lock`，不在 CI 連網抓匯入檔；匯入來源更新時要有人在本機跑 `cargo vet`（非 locked）更新 `imports.lock`，並在 PR 中檢視差異。
- 159 個 crate 版本只有 exemption，等於「沒人稽核過、由人工批准暫時放行」。之後的工作（US-M1-24 後續）是逐步以真正的稽核或新的可信匯入取代，exemption 只減不增；每次依賴升級時 `cargo vet` 會指出哪些需要重新處理。
- 值得優先人工審視的高風險項目：
  - `aws-lc-rs` / `aws-lc-sys`（由 reqwest 的 rustls 預設 crypto provider 引入，含大量 C / 組語）與 `ring`（同時存在）。是否要調整 reqwest / rustls 的 crypto provider 屬於依賴 feature 變更，需另案批准，不在本紀錄範圍。
  - `whisper-rs-sys`（whisper.cpp 原始碼與 build script）、`bindgen`、`cmake`、`cc` 等 build 期依賴（safe-to-deploy 也涵蓋 build script 的執行）。
  - `security-framework`、`windows*`（FFI 面，直接碰觸 Keychain / CredMan）。
- 匯入是「信任這些組織的稽核判斷」，若某組織的 audits.toml 被竄改，影響會透過下一次 `cargo vet` 更新進入 `imports.lock`；因此 `imports.lock` 的變更必須經 PR review。
- CLAUDE.md 的 vet 規則改為：agent 不執行 certify / trust，也不自行新增 exemption；exemption 只能經由人工批准的 `docs/changes/` 紀錄加入。
