# 0003 — M0 專案骨架

- Status：Accepted
- Date：2026-10-24

## Context

PROPOSAL §13 M0 的範圍：repo 結構、CI（fmt、clippy、test、deny、vet）、deny.toml、CLAUDE.md、SECURITY.md、THREAT_MODEL.md。DoD 是 CI 全綠，且 INV 測試的骨架已存在。

## Decision

1. 文件位置：
   - 長期規格放 `docs/specs/`：PROPOSAL.md v0.2、THREAT_MODEL.md。
   - 決策紀錄放 `docs/changes/`。
   - `SECURITY.md` 放 repo 根目錄，GitHub 才會在 Security 分頁辨識它。
2. Rust workspace：
   - 成員是 `core`（keepsay-core）與 `cli`（keepsay-cli），edition 2024。
   - toolchain 釘在 1.97.0（`rust-toolchain.toml`）。
   - workspace 層級設定 `unsafe_code = "forbid"`。
3. M0 只引入目前用得到的白名單依賴：serde、thiserror、secrecy（會連帶引入 zeroize）。serde_json 只放在 dev-dependency。其餘白名單依賴等到 M1 用到時才加。
4. 不需要額外依賴就能真正實作的 INV 檢查，現在就實作並啟用：
   - INV-3：`validate_base_url` 只接受與 BASE_URL 完全相同的字串（`core/tests/egress.rs`）。
   - INV-4：序列化後的請求只允許五個 key（`core/tests/request_type.rs`）。
   - INV-7：用文字掃描確認 `downloader.rs` 沒有引用 vault、secrecy 或 api（`core/tests/downloader.rs`）。
   - INV-2：fingerprint 只回傳末四碼；SecretString 的 Debug 輸出會遮蔽內容（vault 單元測試）。
   - transcript 標籤字串的清除，以及翻譯守門用的內容單位計算。
   需要 M1 元件的測試（canary、redirect、hash mismatch）先以 `#[ignore = "M1: …"]` 保留。
5. `test-endpoint` 的雙重防線：
   - crate 內的 `compile_error!` 讓 release 建置一旦開啟此 feature 就無法編譯。
   - CI 的 `release-feature-guard` job 檢查依賴圖中沒有任何成員開啟它，並驗證 release 建置確實會拒絕它。
6. CI：
   - 只使用第一方 action `actions/checkout`，釘在 v7.0.1 的 commit SHA，並設定 `persist-credentials: false`。
   - cargo-deny 0.20.2 與 cargo-vet 0.10.3 以 `cargo install --locked` 安裝並釘版本，不使用第三方 action。
   - workflow 的 `permissions` 為 `contents: read`。
7. cargo-vet：
   - 執行 `cargo vet init`，目前的依賴全部列為 exemptions（15 個）。
   - 尚未匯入其他組織的稽核紀錄（mozilla、google、bytecode-alliance 等），因為匯入需要人工決定信任哪些來源。列為 M1 待辦。
8. deny.toml：
   - licenses 只允許寬鬆授權，不論專案最終選 GPL-3.0 或 Apache-2.0 都相容。
   - bans 直接禁止 openssl / native-tls、遙測與 crash 上傳套件、第二套 HTTP stack，以及 Tauri 的 http / shell / fs / updater plugin。
   - sources 只允許 crates.io。
9. `.claude/settings.json`：
   - 允許 cargo 的 fmt / clippy / test / build / tree / deny / vet。
   - 明確拒絕 `cargo add`（INV-9），以及 `cargo vet certify` / `trust`，因為稽核屬於人工決定。

## Consequences

- 發現授權不一致：repo 的 LICENSE 是 GPL-3.0，但 ADR-8 寫的是 Apache-2.0；GPL 與 App Store 的相容性也有爭議。已列入 PROPOSAL §16 第 7 項，待使用者決定。Cargo metadata 暫依 LICENSE 檔案標為 `GPL-3.0-only`。
- `cargo test --release --all-features` 在設計上就會失敗。CLAUDE.md 的 Gotchas 已註明。
- CI 每次都要從原始碼編譯 cargo-deny 與 cargo-vet，會比較慢。之後可以評估加上快取，或改用官方發佈的 binary 並驗證 checksum（屬於供應鏈決策，需人工批准）。
