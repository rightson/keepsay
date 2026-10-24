# keepsay 路線圖與進度

> 本檔是「下一步要做什麼」與「目前做到哪裡」的唯一依據。規格本文在 [PROPOSAL.md](PROPOSAL.md)（下稱 §N）與 [THREAT_MODEL.md](THREAT_MODEL.md)，這裡只放故事、驗收條件與狀態，不重複規格內容。
>
> 更新方式：
> - 故事狀態與實作放在同一個 commit / PR 更新；PR 標題或內文寫出故事 ID（例如 `US-M1-07`）。
> - 涉及非小型決策時，在故事的「相依」或「狀態」補上 `docs/changes/NNNN-…` 連結。
> - 新故事接在該里程碑最後一個編號之後，ID 一經使用不重用、不改號；作廢的故事標 ~~刪除線~~ 並註明原因。
> - 一個故事應能在一個 PR 內完成；太大就拆。
>
> 狀態圖例：✅ 完成 ／ 🚧 進行中 ／ ⬜ 未開始 ／ ⏸ 卡住（註明等什麼）／ 🧑 需人工
>
> 最後更新：2026-10-24（[0014](../changes/0014-m1-integration.md) M1 整合：Keychain 原子取代、FFI 匯出、端到端與金絲雀）

## 進度總覽

| 里程碑 | 目標 | 狀態 | 故事完成 | 卡點 |
|--------|------|------|----------|------|
| M0 專案骨架 | repo、CI、INV 測試骨架 | ✅ | 10 / 10（100%） | — |
| M1 core 與 CLI | §13 M1：vault、client、polish、guard、budget、STT、downloader、CLI | 🚧 | 17 / 26（65%） | D-7、D-9、D-11；§15 V4 / V6 回填；M1-04/05 實機；M1-18 維護者複核 |
| M2 macOS | menu bar app 端到端 | ⬜ | 0 / 17（0%） | M1；§16-1 Apple 帳號 |
| M3 Windows | Tauri tray app 端到端 | ⬜ | 0 / 13（0%） | M1；D-2；§16-2 簽章 |
| M4a iOS 主 App | 錄音 → 潤稿 → 剪貼簿、App Intent | ⬜ | 0 / 6（0%） | M2（共用 KeepsayKit）；§16-1 |
| M4b iOS 鍵盤 | session 模式 | ⬜ | 0 / 4（0%） | M4a；V3 |
| M5 發佈加固 | provenance、簽章、套件管道 | ⬜ | 0 / 7（0%） | §16-1/2/3/6 |
| v1.1+ / Backlog | v1 之後 | — | 0 / 8 | 依各項 |
| **v1 合計（M0–M5）** | | | **27 / 83（約 33%）** | |

## 需求對照（FR / INV → 故事）

| 項目 | 故事 |
|------|------|
| FR-1 key 管理 | M1-06、M2-05、M3-03、M4a-01 |
| FR-2 觸發方式 | M2-02、M2-06、M2-07、M3-04、M4a-03 |
| FR-3 本機 STT | M1-17、M1-18、M2-01、M2-09、M3-06 |
| FR-4 潤稿模式 | M1-09、M1-13 |
| FR-5 插入 | M2-10、M2-11、M3-07、M3-08、M4a-02、M4b-03 |
| FR-6 預算 | M1-14、M2-14、M3-10、M4a-05 |
| FR-7 詞彙表 | M1-16 |
| FR-8 潤飾並翻譯 | M1-10、M1-12、M1-13、M2-13 |
| INV-1 / INV-2 | M0-08、M1-04、M1-05、M1-06、M1-21、M1-22、M3-11 |
| INV-3 / INV-4 | M0-05、M0-08、M1-07、M1-08 |
| INV-5 / INV-6 | M1-20、M1-21、M1-25 |
| INV-7 | M0-08、M1-19 |
| INV-8 | M5-01、M5-02、M5-03、M5-04 |
| INV-9 | M0-03、M0-04、M1-24 |
| INV-10 | M4b-04 |

---

## M0 專案骨架（§13 M0）

**US-M0-01** 身為維護者，我想要 §5.2 的 workspace 結構（core、cli、apple/、windows/、models/、docs/），以便各平台在固定位置開工。
- 驗收：① `cargo build --workspace` 通過 ② `rust-toolchain.toml` 釘版本 ③ workspace `unsafe_code = "forbid"`（0005 改為 `"deny"`，只有 `core/src/vault/windows.rs` 例外）
- 狀態：✅ ｜ 相依：— ｜ 見 [0003](../changes/0003-m0-skeleton.md)

**US-M0-02** 身為維護者，我想要 CI 跑 fmt、clippy `-D warnings`、test（含 `--all-features`），以便每個 PR 自動把關。
- 驗收：① `.github/workflows/ci.yml` 有 fmt / clippy / test job ② PR #1 合併時全綠
- 狀態：✅ ｜ 相依：—

**US-M0-03** 身為安全稽核者，我想要 `cargo deny` 擋掉非白名單的 TLS、HTTP stack、遙測套件與非 crates.io 來源，以便 INV-6、INV-9 有機器把關。
- 驗收：① `deny.toml` bans openssl / native-tls / sentry / tauri http·shell·fs·updater ② sources 只允許 crates.io ③ `.claude/settings.json` 拒絕 `cargo add`
- 狀態：✅ ｜ 相依：—

**US-M0-04** 身為安全稽核者，我想要 `cargo vet` 在 CI 執行，以便每個依賴版本都有稽核紀錄。
- 驗收：① `supply-chain/` 已 init ② CI `vet` job 執行 `cargo vet --locked`（M0 時 15 個 exemptions；匯入外部稽核見 US-M1-24 / 0011）
- 狀態：✅ ｜ 相依：—

**US-M0-05** 身為安全稽核者，我想要 release 建置無法啟用 `test-endpoint`，以便 INV-3 的測試後門不會出貨。
- 驗收：① `core/src/lib.rs` 的 `compile_error!` ② CI `release-feature-guard` 檢查依賴圖並驗證 release 建置拒絕該 feature
- 狀態：✅ ｜ 相依：—

**US-M0-06** 身為安全稽核者，我想要 Actions 釘 commit SHA、`permissions: contents: read`、`persist-credentials: false`，以便降低 T1 風險。
- 驗收：① ci.yml 只用第一方 action 且釘 SHA ② 工具以 `cargo install --locked` 釘版本
- 狀態：✅ ｜ 相依：—

**US-M0-07** 身為貢獻者，我想要 README、SECURITY.md、THREAT_MODEL.md、CLAUDE.md、docs/changes/README.md，以便知道規則與回報管道。
- 驗收：① SECURITY.md 含根本限制與私下回報管道 ② THREAT_MODEL 每條對策標出保證來源
- 狀態：✅ ｜ 相依：—

**US-M0-08** 身為安全稽核者，我想要 INV 測試骨架就位，能做的先啟用，以便 M1 只需補實作。
- 驗收：① `core/tests/egress.rs`（BASE_URL 完全比對）、`request_type.rs`（只有五個 key）、`downloader.rs`（文字掃描）啟用中 ② `canary.rs`、redirect、checksum 測試以 `#[ignore = "M1: …"]` 保留（轉入 M1-07、M1-19、M1-21）
- 狀態：✅ ｜ 相依：—

**US-M0-09** 身為維護者，我想要 iOS 與桌面 UX mockup，以便 M2–M4 的畫面與流程先經過審視。
- 驗收：① `docs/mockups/mobile.html`、`desktop.html` 可離線開啟 ② 涵蓋 key 設定、模型下載、錄音 / 處理 / 完成 / 退回 raw、設定頁
- 狀態：✅ ｜ 相依：—

**US-M0-10** 身為維護者，我想要授權、網域與快捷鍵決策寫入 repo，以便後續識別字與 LICENSE 不再變動。
- 驗收：① `docs/changes/0004-license-domain-hotkeys.md` 存在 ② LICENSE 為 Apache-2.0 全文、Cargo metadata 一致 ③ 識別字為 `dev.rightson.keepsay.*`，§15 V8 / V9 回填結論 ④ README、CLAUDE.md、mockup 快捷鍵同步
- 狀態：✅ 完成 ｜ 相依：D-1

---

## M1 keepsay-core 與 CLI（§13 M1；DoD：§11 自動化測試全過、金絲雀通過）

> 平行實作：共用型別與簽章已在 [0005](../changes/0005-m1-foundation.md) 定好，分成五個工作流，各自只改自己的檔案（檔案清單見 0005「模組分工」）。A api：M1-01、07、08 ／ B polish：M1-09–13、16 ／ C vault：M1-02、04–06、21 ／ D downloader + budget + config：M1-14、15、18、19 ／ E stt + cli：M1-16、17、23、25。五個工作流已合併，整合見 [0014](../changes/0014-m1-integration.md)。
>
> M1 DoD（§13）：§11 自動化測試在 Linux 上全部通過（`cargo test --workspace --all-features`：194 passed、1 ignored，ignored 的是手動執行的校正輸出），金絲雀涵蓋 vault / API / polish / 設定 / 預算 / 下載器 / FFI 路徑並通過。尚未達成：Apple / Windows vault 的實機屬性驗證（macOS 需簽章 binary；Windows 待 CI test-os job 實跑）、§15 V4 / V6 回填、D-7 / D-9 確認。
>
> M1 剩餘人工事項（🧑）：
> - D-7 翻譯守門門檻、D-9 light / formal prompt 的確認（US-M1-09、12）。
> - D-11 `uniffi/tokio`、D-12 Swift binding 產生器與 crate-type（US-M1-22、M2-04）。
> - 維護者在自己的機器重算三個模型的 SHA-256（US-M1-18，方法見 [0009](../changes/0009-models-budget-config.md)）。
> - §16-4：建立開發用 Anthropic workspace 與 key，順便確認撤銷 key、關閉自動儲值的 Console 位置（US-M1-01）。
> - §16-1：簽章 binary 與 `keychain-access-groups` entitlement，才能實測 Keychain 屬性與取代流程在 userPresence 下的提示（US-M1-04、M2）。
> - §16-5：商標與 GitHub 同名確認須在 M1 結束前完成（vault 識別字一旦有使用者資料就要遷移）。
> - 是否改寫 `core/tests/api_errors.rs` 以值比對假 key 的 header matcher（見 0014 Consequences）。

**US-M1-01** 身為維護者，我想要完成 V4 spike（模型 ID、`anthropic-version`、`/v1/models`、workspace 與 spend limit），以便 client 與 onboarding 用的是現行事實。
- 驗收：① §15 V4 回填結論與官方文件連結 ② `MODEL_ALLOWLIST` / `DEFAULT_MODEL` 依結論更新 ③ §12 onboarding 文字依 Console 現況修正
- 狀態：🚧 ｜ 相依：§16-4 ｜ ② 完成；結論在 [0006](../changes/0006-v4-api-facts.md)（`claude-haiku-5-5`、`anthropic-version: 2026-10-21`）；① §15 回填與 ③ §12 修正由 docs 整合者套用中；撤銷 key 與關閉自動儲值的 Console 位置待 🧑 §16-4 確認

**US-M1-02** 身為維護者，我想要完成 V6 spike（security-framework 能否設 ThisDeviceOnly 與 SecAccessControl），以便決定 Apple vault 由 Rust 或 Swift 寫入。
- 驗收：① §15 V6 回填 ② 若不可行，在 0005+ 記錄改由 Swift 寫入、Rust 讀取的方案
- 狀態：🚧 ｜ 相依：— ｜ 結論見 [0008](../changes/0008-vault.md)（可由 Rust 寫入；禁用 `set_access_control_options`）與 [0014](../changes/0014-m1-integration.md)（取代 key 三步驟）；§15 V6 回填由 docs 整合者套用中；實機驗證屬 M2

**US-M1-03** 身為安全稽核者，我想要 Win32 FFI 需要的 `unsafe` 有明確範圍，以便 Windows vault 能實作又不放寬整個 workspace。
- 驗收：① docs/changes 記錄決策（例如只在 `core/src/vault/windows.rs` 以 `#[allow(unsafe_code)]` 開放，或改用安全封裝）② CLAUDE.md Code style 同步
- 狀態：✅ ｜ 相依：D-2 ｜ 見 [0005](../changes/0005-m1-foundation.md)（只開放 `core/src/vault/windows.rs`，CI `unsafe-scope` 檢查）

**US-M1-04** 身為使用者，我想要 key 存進 Keychain（ThisDeviceOnly、不同步、service `dev.rightson.keepsay.apikey`），以便 key 不落地明文、不進 iCloud（INV-1）。
- 驗收：① `vault/apple.rs` 實作 `KeyVault` ② 屬性由 macOS 上的整合測試驗證 ③ 不設共享 access group（INV-10 前置）
- 狀態：🚧 🧑 ｜ 相依：US-M1-02、§16-1 ｜ ① ③ 完成（[0008](../changes/0008-vault.md)；取代 key 改為 pending → 刪舊 → 改名，失敗不丟舊 key，[0014](../changes/0014-m1-integration.md)）。② `core/tests/vault_platform.rs` 在未簽章的 macOS CI 只能驗證乾淨回 `VaultNotEntitled`、無殘留；屬性實測需簽章 binary（M2）

**US-M1-05** 身為使用者，我想要 Windows 上 key 存進 Credential Manager（GENERIC、LOCAL_MACHINE），以便 key 不隨漫遊設定檔同步（INV-1）。
- 驗收：① `vault/windows.rs` 實作 `KeyVault`，target 依 D-6 定名 ② CredFree 前覆寫 buffer ③ Windows CI runner 上跑整合測試
- 狀態：🚧 ｜ 相依：US-M1-03 ✅、D-6 ✅ ｜ ① ② 完成（[0008](../changes/0008-vault.md)；`CredWriteW` 單次覆寫，取代 key 沒有空窗，[0014](../changes/0014-m1-integration.md)），交叉編譯 clippy 通過；③ 待 CI test-os（windows-latest）實跑 `core/tests/vault_platform.rs`

**US-M1-06** 身為使用者，我想要 store / has / fingerprint / delete / verify 的 key 生命週期，以便驗證失敗的 key 不會被存下（FR-1）。
- 驗收：① 格式檢查 → `GET /v1/models` → 存入，失敗不存 ② 401/403 回 `KeyInvalid` 且不自動刪除 ③ 只回 `vault::fingerprint`（INV-2）④ 預設不快取，用完 drop（§6.2）
- 狀態：✅ ｜ 相依：US-M1-04/05、US-M1-07 ｜ `vault::store_key` 經 FFI `store_key` 與 CLI `key set` 以 `AnthropicClient::verify_key` 接上；`stored_fingerprint`、`verify_stored_key` 讓 vault 與 client 以外不必 `load`（`core/tests/no_key_load.rs`、`ffi_flow.rs`，[0014](../changes/0014-m1-integration.md)）

**US-M1-07** 身為安全稽核者，我想要 `AnthropicClient` 以 reqwest + rustls-platform-verifier、`redirect::Policy::none()`、`no_proxy()`、10s/30s 逾時實作，以便 key 只會送到 BASE_URL（INV-3）。
- 驗收：① `egress.rs` 的 redirect 測試解除 ignore 並通過（第二台 mock 收不到 `x-api-key`）② 非白名單 host 建構失敗 ③ proxy 只能由設定明確開啟 ④ 只有此型別能附加 key（code review 清單）
- 狀態：✅ ｜ 相依：US-M1-01 ｜ 依賴與簽章已在 [0005](../changes/0005-m1-foundation.md)；實作見 [0006](../changes/0006-v4-api-facts.md)

**US-M1-08** 身為使用者，我想要 API 錯誤有一致的對應與重試，以便失敗時快速退回 raw 而不是卡住（§6.3）。
- 驗收：① wiremock 覆蓋 401/403、429/529（retry-after 上限 2s、只重試一次）、400 額度不足、逾時 ② 錯誤訊息不含任何使用者內容
- 狀態：✅ ｜ 相依：US-M1-07 ｜ `core/tests/api_errors.rs`；402 與 429 spend cap 也對應 `BudgetExhausted`（[0006](../changes/0006-v4-api-facts.md)）

**US-M1-09** 身為使用者，我想要 raw / light / polish / formal 四種模式的 prompt，以便依情境選擇潤稿強度（FR-4）。
- 驗收：① raw 不呼叫 API ② polish system prompt 依 §6.5 ③ light、formal 的 prompt 寫入 docs/changes 並同步 §6.5（規格目前未定義）④ 單元測試確認標籤清除
- 狀態：🧑 實作與測試完成（[0007](../changes/0007-polish-prompts-and-guard.md)）；light / formal prompt 待人確認（D-9），§6.5 同步由 docs 整合者套用 ｜ 相依：D-9

**US-M1-10** 身為使用者，我想要「潤飾並翻譯」的 prompt 變體（en / zh-TW / ja），以便一次請求得到譯文（FR-8、ADR-9）。
- 驗收：① 依 §6.5 翻譯變體與 `{target_style_rule}` ② `max_tokens` 用 ×3 公式（已有 `request_type.rs` 測試）③ 詞彙表拼寫保留
- 狀態：✅ 工作流 B（[0007](../changes/0007-polish-prompts-and-guard.md)）｜ 相依：US-M1-09

**US-M1-11** 身為使用者，我想要非翻譯模式的輸出守門，以便被注入或失控的輸出不會插入 App（§6.6、T5）。
- 驗收：① 長度比例、標籤殘留、前言剝除規則 ② proptest 覆蓋 ③ 未通過時回傳 `fell_back_to_raw` 與原因
- 狀態：✅ 工作流 B（[0007](../changes/0007-polish-prompts-and-guard.md)）｜ 相依：—

**US-M1-12** 身為使用者，我想要翻譯模式的守門（內容單位、跨語言比例、目標語言比例 ≥ 60%），以便不輸出半翻譯結果（FR-8）。
- 驗收：① 沿用 `guard::content_units` ② 固定語料 + proptest ③ 0.3 / 3 / 60% 以語料校正，結果寫入 docs/changes
- 狀態：🧑 實作與校正完成（[0007](../changes/0007-polish-prompts-and-guard.md)：長度改用加權大小、上限 ×2 + 10，新增 ja 假名檢查）；待 D-7 確認 ｜ 相依：US-M1-11、D-7

**US-M1-13** 身為使用者，我想要 `polish()` 串起 prompt、client、guard、budget，任何錯誤都退回 raw，以便永遠拿得到可用文字（FR-4、FR-8）。
- 驗收：① `PolishResult` 含 `fell_back_to_raw`、`fallback_reason`、`usage` ② 翻譯在 raw / 無 key / 硬上限時輸出原文並給「翻譯需要 API」原因 ③ wiremock 端到端測試
- 狀態：✅ ｜ 相依：US-M1-08、10、12、14 ｜ ③ `core/tests/polish_e2e.rs` 已解除 ignore，經真正的 `AnthropicClient` 對 wiremock；不完整回應（`is_complete()`）退回 raw 並保留 usage；請求不含 temperature、`thinking` 為 disabled（[0014](../changes/0014-m1-integration.md)）

**US-M1-14** 身為使用者，我想要依 API usage 累計 token、可編輯價目表、月軟 / 硬上限，以便費用可控（FR-6、§6.8）。
- 驗收：① budget.json 帶 schema version，非機密 ② 硬上限時 `needs_api` 請求改走 raw ③ 每月 1 日歸零（單元測試注入時間）
- 狀態：✅ ｜ 相依：US-M1-15 ｜ 見 [0009](../changes/0009-models-budget-config.md)；FFI `polish` / `budget_status` 記帳與「ledger 損毀視為硬上限」（[0014](../changes/0014-m1-integration.md)）

**US-M1-15** 身為使用者，我想要非機密設定以 JSON 存在 App Support 目錄並帶 schema version，以便設定可攜且不含 key（§10、INV-1）。
- 驗收：① 涵蓋模式、快捷鍵、目標語言、模型、詞彙表、價目表、預算、proxy ② 讀寫測試確認檔案不含金絲雀
- 狀態：✅ ｜ 相依：D-3 ✅（路徑由呼叫端傳入，見 [0005](../changes/0005-m1-foundation.md)）｜ 見 [0009](../changes/0009-models-budget-config.md)；金絲雀掃描設定檔（[0014](../changes/0014-m1-integration.md)）

**US-M1-16** 身為使用者，我想要自訂詞彙表同時注入 STT initial prompt 與潤稿 system prompt，以便專有名詞拼對（FR-7）。
- 驗收：① 兩處注入點有單元測試（只斷言結構與長度，不斷言詞彙表內容）② 詞彙表不出現在 log 或錯誤訊息
- 狀態：✅ 工作流 B（prompt，見 [0007](../changes/0007-polish-prompts-and-guard.md)）、E（STT：`stt::initial_prompt`，見 [0010](../changes/0010-stt-cli.md)）｜ 相依：US-M1-09、US-M1-17

**US-M1-17** 身為使用者，我想要 whisper-rs 在本機辨識台灣繁中與中英夾雜，以便語音不離開裝置（FR-3、§6.7）。
- 驗收：① `stt/whisper.rs` 實作 `SttEngine`，Apple 走 Metal ② 繁中 initial prompt ③ 樣本 wav 的辨識測試（可標 `#[ignore]` 需模型檔，CI 另行處理）
- 狀態：✅ 工作流 E，見 [0010](../changes/0010-stt-cli.md)（③ `core/tests/stt_whisper.rs`，設 `KEEPSAY_TEST_WHISPER_MODEL` 才跑；Metal 尚未在 Apple 實機驗證）｜ 相依：US-M1-18 ｜ feature `whisper`（core 預設關、CLI 預設開）

**US-M1-18** 身為維護者，我想要 `models/manifest.json` 填入 small 與 large-v3-turbo 的固定 revision URL 與親自計算的 SHA-256，以便下載可驗證（INV-7）。
- 驗收：① 每筆含 id、url、revision、sha256、size、license ② 雜湊由維護者下載後計算，PR 註明方法
- 狀態：🚧 🧑 ｜ 相依：— ｜ base、small、large-v3-turbo 已填入（釘 revision `5359861c`），SHA-256 由 agent 下載計算並與 HF LFS oid 比對一致；待維護者在自己的機器複核（[0009](../changes/0009-models-budget-config.md)）

**US-M1-19** 身為安全稽核者，我想要 `ModelDownloader` 只抓 manifest 的 URL、驗證 SHA-256、支援手動匯入，以便下載器拿不到 key 也收不到被換掉的檔（INV-7、§8）。
- 驗收：① `downloader.rs` 的 checksum 測試解除 ignore ② 另一個 reqwest Client，無 redirect 到 manifest 外的 host ③ `downloader.rs` 文字掃描仍通過 ④ 超過 size 中止
- 狀態：✅ ｜ 相依：US-M1-18、D-3 ✅ ｜ redirect 手動跟隨、逐跳檢查 `redirect_hosts`，見 [0009](../changes/0009-models-budget-config.md)；FFI `download_model` / `import_model`

**US-M1-20** 身為安全稽核者，我想要 tracing 只記事件、長度與耗時，panic hook 不記 payload，以便 log 與 crash 不洩漏（§6.9、INV-6）。
- 驗收：① 自訂 panic hook 只記位置 ② 測試以 tracing-subscriber 擷取輸出 ③ 無任何遠端 sink
- 狀態：✅ ｜ 相依：— ｜ `core::panic_hook::install()`（只記位置，`ffi::init` 會安裝）；CLI 自有 stderr 版；`canary.rs` 的 `panic_hook_logs_location_not_payload` 以 tracing 擷取驗證；deny.toml 擋遙測 / crash 上傳套件（[0014](../changes/0014-m1-integration.md)）

**US-M1-21** 身為安全稽核者，我想要金絲雀測試跑完所有正常與錯誤路徑並掃描 log、設定目錄、暫存目錄，以便 INV-1/2/6 有自動化證明（§11）。
- 驗收：① `core/tests/canary.rs` 解除 ignore ② 金絲雀出現次數為 0 ③ 整合測試可用的 mock vault（目前 `mock.rs` 只在 `cfg(test)`，`tests/` 用不到，需改為 feature 或測試 helper）
- 狀態：✅ ｜ 相依：US-M1-06、13、14、15、20 ｜ vault、API 錯誤（401/403/402/400/429/529/500/302/格式錯誤/逾時）、polish 退回、設定 / 預算寫檔、下載器錯誤、FFI 全部跑過，金絲雀 0 次；另斷言金絲雀只出現在送往 API 的 `x-api-key`；`CoreError` 每個 variant 由窮舉 `match` 保證列入（[0008](../changes/0008-vault.md)、[0014](../changes/0014-m1-integration.md)）

**US-M1-22** 身為安全稽核者，我想要 UniFFI 匯出 §6.1 介面，且沒有任何回傳 key 的函式，以便 Swift 只經手 key 一次（INV-2）。
- 驗收：① `ffi.rs` 匯出 §6.1 全部函式（含 async `polish`）② 測試掃描匯出簽章，回傳型別不含 key ③ `store_key` 收下即轉 `SecretString`
- 狀態：🧑 ｜ 相依：US-M1-06、13、17、19、D-10 ✅、D-11 ｜ ② ③ 完成；① §6.1 全部匯出（另加 `init`、`set_utc_offset`、`import_model`），但 `polish` 等為阻塞呼叫（每次一個 current-thread runtime），真正的 async 匯出需要 `uniffi/tokio`（D-11）。`core/tests/ffi_surface.rs`、`ffi_flow.rs`；Swift binding 產生方式留到 M2（D-12）（[0014](../changes/0014-m1-integration.md)）

**US-M1-23** 身為貢獻者，我想要 `keepsay-cli <file.wav> [--mode] [--translate]`，以便不開 App 就能測整條管線（§13 M1）。
- 驗收：① wav → 逐字稿 → 潤稿 / 潤稿並翻譯 ② 輸出只到 stdout，不寫檔、不記 log 內容 ③ key 經由 vault 取得，不接受命令列參數或環境變數
- 狀態：✅ ｜ 相依：US-M1-13、17、D-3 ✅（WAV 自行實作）｜ [0010](../changes/0010-stt-cli.md)；`cli/tests/cli_api.rs`（feature `test-endpoint`）以 wiremock 跑 `polish --text-stdin`、`--translate`、`key verify` / `status`、`budget`；wav 端到端在設 `KEEPSAY_TEST_WHISPER_MODEL` 時跑（[0014](../changes/0014-m1-integration.md)）

**US-M1-24** 身為安全稽核者，我想要匯入可信組織的 cargo-vet 稽核並逐步消化 exemptions，以便新增的 M1 依賴有人審過（INV-9）。
- 驗收：① 由人決定要信任的來源（mozilla、google、bytecode-alliance…）② exemptions 數量在本 PR 前後列出 ③ 不由 agent 執行 certify / trust
- 狀態：✅ ｜ 相依：D-4 ｜ 見 [0011](../changes/0011-cargo-vet-imports.md)：匯入 mozilla、google、bytecode-alliance、isrg、zcash、embark-studios；exemptions 15 → 176 筆（47 個 crate 由匯入完全涵蓋、18 個部分涵蓋、159 個 exemption）；`cargo vet --locked` 通過。後續逐步以稽核取代 exemption

**US-M1-25** 身為安全稽核者，我想要自動檢查音訊與逐字稿不落地，以便 INV-5 不只靠 review。
- 驗收：① STT 與 polish 路徑跑完後暫存與設定目錄無新檔（併入金絲雀掃描）② `stt/`、`polish/` 不使用 `std::fs` 寫入（文字掃描測試，比照 `downloader.rs`）
- 狀態：✅ ｜ 相依：US-M1-17、US-M1-21 ｜ ① 金絲雀測試斷言 polish / API 路徑後資料目錄為空，CLI 端 `cli.rs` / `cli_api.rs` 同樣檢查；② `core/tests/no_disk_writes.rs` 改為掃描 `stt/`、`polish/` 下所有檔案（[0014](../changes/0014-m1-integration.md)）

**US-M1-26** 身為維護者，我想要完成 V7 spike（OpenCC Rust 實作與依賴成本），以便決定 raw 模式的簡繁問題是否要引入新依賴。
- 驗收：① 以 M1-17 的樣本量化簡體出現率 ② §15 V7 回填，需依賴時提 D-5
- 狀態：⬜ ｜ 相依：US-M1-17

---

## M2 macOS menu bar app（§13 M2；DoD：V2、V9 結論、手動清單、notarized 測試版）

**US-M2-01** 身為維護者，我想要完成 V1 spike（SpeechAnalyzer / SFSpeechRecognizer 的 zh-TW 與中英夾雜品質），以便決定 Apple 端預設 STT 引擎。
- 驗收：① 與 whisper 用同一批樣本比較 ② §15 V1 回填，強制 on-device
- 狀態：⬜ ｜ 相依：US-M1-17

**US-M2-02** 身為維護者，我想要實測 V9 的 active CGEventTap 方案（🌐 鍵行為、AZERTY keycode、Secure Input、與 sandbox 的交互），以便確認 0004 的決定可行。
- 驗收：① §15 V9 列出的四項實測結果回填 ② 若不可行，改以 Carbon 組合為預設並記錄於 docs/changes
- 狀態：⬜ ｜ 相依：US-M0-10

**US-M2-03** 身為維護者，我想要完成 V2 spike（sandbox 下 AX 插入與 CGEvent 貼上），以便決定 entitlements。
- 驗收：① §15 V2 回填 ② 開 sandbox 時只開 network.client、audio-input；關閉時在 SECURITY.md 說明原因 ③ Hardened Runtime 開啟
- 狀態：⬜ ｜ 相依：—

**US-M2-04** 身為貢獻者，我想要 Xcode 專案與 KeepsayKit 透過 UniFFI 連結 core，以便 macOS 與 iOS 共用同一份 Swift 綁定。
- 驗收：① §5.2 的 apple/ 結構 ② 無第三方 Swift 套件（§9）③ bundle id `dev.rightson.keepsay.*`
- 狀態：⬜ ｜ 相依：US-M1-22

**US-M2-05** 身為使用者，我想要在 SecureField 輸入 key、看到末四碼、可更換與刪除，並看到 §12 的安全引導，以便安心設定 key（FR-1）。
- 驗收：① 送出後立即 `store_key` 並清空欄位（INV-2）② 沒有顯示或複製 key 的功能 ③ onboarding 顯示 §12 四步驟
- 狀態：⬜ ｜ 相依：US-M2-04、US-M1-01

**US-M2-06** 身為使用者，我想要用 fn+Z / fn+Shift+Z 開始與結束錄音，以便單手觸發潤飾或翻譯（FR-2）。
- 驗收：① active tap 隔離在單一檔案，回呼只讀 Z keycode 與 fn / Shift 旗標、不保存任何按鍵（T7）② 有自動檢查限制該檔案的事件處理範圍（THREAT_MODEL T7 待補）③ toggle 語意與 Esc 取消
- 狀態：⬜ ｜ 相依：US-M2-02

**US-M2-07** 身為沒有 fn 鍵或不授予輔助使用權限的使用者，我想要 Carbon 備援組合 ⌥Space / ⌥⇧Space 與改鍵 UI，以便仍能使用快捷鍵（FR-2）。
- 驗收：① 註冊失敗時提示改鍵 ② 設定中可錄製新快捷鍵
- 狀態：⬜ ｜ 相依：US-M2-04

**US-M2-08** 身為使用者，我想要 AVAudioEngine 錄音轉 16 kHz mono f32 只存在記憶體，以便音訊不落地（INV-5）。
- 驗收：① 不使用任何檔案型錄音 API ② 錄音結束即釋放 buffer
- 狀態：⬜ ｜ 相依：US-M2-04

**US-M2-09** 身為使用者，我想要在 App 內下載或手動匯入模型並看到進度，以便離線辨識（FR-3、INV-7）。
- 驗收：① 只呼叫 `download_model` ② 匯入檔案同樣驗 SHA-256
- 狀態：⬜ ｜ 相依：US-M1-19、US-M2-04

**US-M2-10** 身為使用者，我想要結果以 AX `kAXSelectedTextAttribute` 插入聚焦欄位，以便不經剪貼簿（FR-5、T6）。
- 驗收：① 插入成功率記入手動清單 ② 失敗時進入 M2-11
- 狀態：⬜ ｜ 相依：US-M2-03

**US-M2-11** 身為使用者，我想要 AX 失敗時用剪貼簿貼上，且標記 Concealed / Transient 並在 300 ms 後還原，以便剪貼簿管理器看不到內容（T6）。
- 驗收：① 備份並還原所有 pasteboard items ② 加上 `org.nspasteboard.ConcealedType`、`TransientType`
- 狀態：⬜ ｜ 相依：US-M2-10

**US-M2-12** 身為使用 CJK 輸入法的使用者，我想要貼上前暫時切到 ASCII 輸入來源、完成後切回，以便 Cmd+V 不被輸入法吃掉（競品 shibing624/VoiceInput 經驗）。
- 驗收：① 只在剪貼簿路徑啟用 ② 切回原輸入來源 ③ 注音、倉頡、日文輸入法手動驗證
- 狀態：⬜ ｜ 相依：US-M2-11

**US-M2-13** 身為使用者，我想要錄音、處理、完成、退回 raw 的 HUD 與提示，以便知道結果是否經過潤稿或翻譯（FR-4、FR-8）。
- 驗收：① 對應 `desktop.html` HUD 狀態 ② 顯示 fallback 原因（含「翻譯需要 API，已輸出原文」）
- 狀態：⬜ ｜ 相依：US-M2-06、US-M1-13

**US-M2-14** 身為使用者，我想要預算 UI（本月用量、估算金額、軟 / 硬上限、價目表編輯），以便控制花費（FR-6）。
- 驗收：① 讀 `budget_status()` ② 達軟上限警告、硬上限提示已切到 raw
- 狀態：⬜ ｜ 相依：US-M1-14

**US-M2-15** 身為使用者，我想要設定預設模式、目標語言、詞彙表、模型，以便符合我的使用習慣（FR-4、FR-7、FR-8）。
- 驗收：① 對應 mockup 設定頁 ② 寫入 M1-15 的設定檔
- 狀態：⬜ ｜ 相依：US-M1-15

**US-M2-16** 身為維護者，我想要跑完 §11 macOS 手動清單（瀏覽器、Electron、Office、終端機、斷網、key 失效、無 fn 外接鍵盤），以便達成 M2 DoD。
- 驗收：① 結果記錄於 docs/changes ② 失敗項目開成新故事
- 狀態：⬜ ｜ 相依：US-M2-05 至 15

**US-M2-17** 身為使用者，我想要可安裝的 Developer ID 簽章與 notarized 測試版，以便不用自行建置。
- 驗收：① CI 產出（不在本機簽）② Gatekeeper 驗證通過
- 狀態：⬜ 🧑 ｜ 相依：§16-1、US-M5-03

---

## M3 Windows Tauri app（§13 M3；DoD：V5、V8 結論、capability 稽核、簽章測試版）

**US-M3-01** 身為維護者，我想要完成 V5 spike（CredUIPromptForWindowsCredentialsW 或自製 ES_PASSWORD 對話框），以便 key 永不進 WebView。
- 驗收：① §15 V5 回填 ② 列出所需 `windows` crate features（最小化）
- 狀態：⬜ ｜ 相依：D-2

**US-M3-02** 身為安全稽核者，我想要 Tauri 2 骨架只開必要 capability、CSP `default-src 'self'; connect-src ipc: http://ipc.localhost`、release 關 devtools、不載入遠端 URL，以便 WebView 無法自行連網（§7.3）。
- 驗收：① 無 shell / fs / http / updater plugin（`cargo deny` 已禁）② capability 稽核表寫入 docs/changes
- 狀態：⬜ ｜ 相依：US-M1-22

**US-M3-03** 身為使用者，我想要在原生 Win32 對話框輸入 key 並看到末四碼，以便 key 不經 JavaScript（FR-1、INV-2）。
- 驗收：① 對話框由 Rust 開啟，直接呼叫 core ② 對應 `desktop.html` 的 w32 對話框
- 狀態：⬜ ｜ 相依：US-M3-01、US-M1-06

**US-M3-04** 身為使用者，我想要 RegisterHotKey 註冊的預設快捷鍵（0004：Alt+Shift+Z / Alt+Shift+X，MOD_NOREPEAT），以便不需鍵盤 hook 就能觸發（FR-2、V8 選項 (c)）。
- 驗收：① 透過 tauri-plugin-global-shortcut，不使用 WH_KEYBOARD_LL 或 Raw Input ② 註冊失敗時提示改鍵 ③ AltGr 配置手動驗證不被攔截
- 狀態：⬜ ｜ 相依：D-1

**US-M3-05** 身為使用者，我想要 cpal 錄音只存在記憶體，以便音訊不落地（INV-5）。
- 驗收：① 16 kHz mono f32 ② 不寫暫存檔
- 狀態：⬜ ｜ 相依：US-M3-02

**US-M3-06** 身為使用者，我想要在 Windows 下載或匯入模型並辨識，以便離線使用（FR-3、INV-7）。
- 驗收：① 呼叫 core 的 download / transcribe ② 進度顯示在設定頁
- 狀態：⬜ ｜ 相依：US-M1-19、US-M3-02

**US-M3-07** 身為使用者，我想要結果以 SendInput + KEYEVENTF_UNICODE 逐字插入，以便不經剪貼簿（FR-5）。
- 驗收：① 不使用 MSIX AppContainer（§7.3）② 長文切換到 M3-08
- 狀態：⬜ ｜ 相依：D-2

**US-M3-08** 身為使用者，我想要剪貼簿備援帶 ExcludeClipboardContentFromMonitorProcessing、CanIncludeInClipboardHistory=0、CanUploadToCloudClipboard=0 並還原原內容，以便不進剪貼簿歷史與雲端（T6）。
- 驗收：① 三種格式皆設定 ② 還原原內容
- 狀態：⬜ ｜ 相依：US-M3-07

**US-M3-09** 身為使用注音 / 新注音的使用者，我想要貼上前把 IME 切到英數模式、完成後還原，以便 Ctrl+V 正常（競品經驗）。
- 驗收：① 只在剪貼簿路徑啟用 ② 還原原 IME 狀態
- 狀態：⬜ ｜ 相依：US-M3-08

**US-M3-10** 身為使用者，我想要 tray、錄音 overlay、設定頁（模式、目標語言、詞彙表、預算、快捷鍵），以便完成 M2 同等功能（FR-4、FR-6、FR-7、FR-8）。
- 驗收：① 對應 `desktop.html` Windows 畫面 ② fallback 提示同 M2-13
- 狀態：⬜ ｜ 相依：US-M3-02

**US-M3-11** 身為安全稽核者，我想要自動檢查所有 IPC command 的回傳型別，以便沒有任何 command 回傳 key（INV-2）。
- 驗收：① 測試列舉 command 並比對允許清單 ② 新增 command 時測試會失敗直到更新清單
- 狀態：⬜ ｜ 相依：US-M3-02

**US-M3-12** 身為維護者，我想要跑完 §11 Windows 手動清單（含 AltGr 配置、Office、終端機、斷網），以便達成 M3 DoD。
- 驗收：① 結果記錄於 docs/changes
- 狀態：⬜ ｜ 相依：US-M3-03 至 11

**US-M3-13** 身為使用者，我想要 Authenticode 簽章的測試安裝檔，以便 SmartScreen 不阻擋。
- 驗收：① 由 CI 簽章 ② MSI 或 NSIS 擇一
- 狀態：⬜ 🧑 ｜ 相依：§16-2、US-M5-04

---

## M4a iOS 主 App（§13 M4a；DoD：TestFlight 內測版）

**US-M4a-01** 身為 iOS 使用者，我想要 onboarding 設定或略過 key、下載模型，以便第一次就能用（FR-1、`mobile.html` ob-key / ob-model）。
- 驗收：① SecureField 送出即清空 ② 略過時只能用 raw
- 狀態：⬜ ｜ 相依：US-M2-04、US-M2-05

**US-M4a-02** 身為 iOS 使用者，我想要錄音 → STT → 潤稿 → 剪貼簿（localOnly、60 秒 expiration），以便貼到任何 App（FR-5、T6）。
- 驗收：① `UIPasteboard` 設 localOnly 與 expirationDate ② 音訊只在記憶體（INV-5）
- 狀態：⬜ ｜ 相依：US-M4a-01

**US-M4a-03** 身為 iOS 使用者，我想要「潤飾」與「潤飾並翻譯」兩個 App 內動作與兩個 App Intent，以便綁 Action Button 或捷徑（FR-2、FR-8）。
- 驗收：① 兩個 Intent 皆可從捷徑執行 ② 失敗時退回 raw 並提示
- 狀態：⬜ ｜ 相依：US-M4a-02

**US-M4a-04** 身為 iOS 使用者，我想要在記憶體受限下仍能載入模型，以便舊機型可用。
- 驗收：① 依裝置推薦模型 ② 實測記憶體峰值記入 docs/changes
- 狀態：⬜ ｜ 相依：US-M1-17、US-M2-01

**US-M4a-05** 身為 iOS 使用者，我想要設定頁（模式、目標語言、詞彙表、預算），以便與桌面一致（FR-6、FR-7）。
- 驗收：① 對應 `mobile.html` settings
- 狀態：⬜ ｜ 相依：US-M4a-01

**US-M4a-06** 身為使用者，我想要 TestFlight 內測版，以便實機試用。
- 驗收：① CI 上傳 ② 隱私標示為「不收集資料」
- 狀態：⬜ 🧑 ｜ 相依：§16-1、US-M0-10

---

## M4b iOS 鍵盤（§13 M4b；DoD：V3 結論、INV-10 檢查）

**US-M4b-01** 身為維護者，我想要完成 V3 spike（background audio + Darwin notification + App Group 是否符合 App Review），以便決定鍵盤是否可行。
- 驗收：① §15 V3 回填，引用 App Review Guidelines 條文
- 狀態：⬜ ｜ 相依：US-M4a-06

**US-M4b-02** 身為 iOS 使用者，我想要在主 App 開啟 session，由鍵盤發送開始 / 停止訊號，以便不離開目前 App 就能聽寫。
- 驗收：① Darwin notification 只傳訊號不傳內容 ② 結果經 App Group 傳遞後立即刪除
- 狀態：⬜ ｜ 相依：US-M4b-01

**US-M4b-03** 身為 iOS 使用者，我想要鍵盤以 `textDocumentProxy.insertText` 插入結果，以便不經剪貼簿（FR-5）。
- 驗收：① 鍵盤在記憶體上限內運作
- 狀態：⬜ ｜ 相依：US-M4b-02

**US-M4b-04** 身為安全稽核者，我想要 CI 檢查鍵盤 target 的連結依賴不含 keepsay-core 與網路框架、沒有共享 Keychain access group，以便 INV-10 有機器把關。
- 驗收：① 解析 build 產物的連結清單 ② entitlements 檢查
- 狀態：⬜ ｜ 相依：US-M4b-03

---

## M5 發佈加固（§13 M5；DoD：任何人能從 tag 驗證 binary 來源）

**US-M5-01** 身為安全稽核者，我想要 release workflow 只在 tag 觸發，並需 release environment 人工核准，以便只有 CI 能發佈（INV-8）。
- 驗收：① PR workflow 無 secret ② environment 核准者已設定
- 狀態：⬜ ｜ 相依：§16-3

**US-M5-02** 身為使用者，我想要每個 release 附 SHA256SUMS 與 `actions/attest-build-provenance`，以便驗證下載物（INV-8）。
- 驗收：① `gh attestation verify` 可通過 ② action 釘 SHA
- 狀態：⬜ ｜ 相依：US-M5-01

**US-M5-03** 身為維護者，我想要 CI 產生簽章並 notarized 的 macOS DMG，以便 M2 測試版與正式版同一管道。
- 驗收：① 使用 App Store Connect API key（secret 只在 release environment）
- 狀態：⬜ 🧑 ｜ 相依：§16-1、US-M5-01

**US-M5-04** 身為維護者，我想要 CI 以雲端 HSM 型服務做 Authenticode 簽章，以便私鑰不在 runner 上。
- 驗收：① 簽章只發生在 release job
- 狀態：⬜ 🧑 ｜ 相依：§16-2、US-M5-01

**US-M5-05** 身為安全稽核者，我想要記錄 toolchain 與 Xcode 版本並嘗試可重現建置，以便第三方能重建比對。
- 驗收：① 建置說明文件 ② 兩次建置的雜湊比對結果
- 狀態：⬜ ｜ 相依：US-M5-02

**US-M5-06** 身為使用者，我想要用 Homebrew cask 與 winget 安裝，以便不需 in-app 更新（ADR-5）。
- 驗收：① manifest 指向 release 資產與雜湊
- 狀態：⬜ 🧑 ｜ 相依：§16-6、US-M5-02

**US-M5-07** 身為使用者，我想要 README / SECURITY.md 寫出驗證步驟、根本限制與 sandbox 決定，以便自行確認安裝物來源。
- 驗收：① 從 tag 到 binary 的驗證指令可照做 ② 狀態從 pre-alpha 更新
- 狀態：⬜ ｜ 相依：US-M5-02、US-M2-03

---

## v1.1+ / Backlog

**US-BL-01** 身為桌面使用者，我想要 push-to-talk（按住說話、放開處理），以便短句更快。競品經驗顯示這是主要使用方式，考慮提前到 M2 / M3（見 D-8）。
- 驗收：① macOS 需要 keyUp 事件（active tap 已訂閱）② Windows RegisterHotKey 沒有放開事件，需另行設計 ③ 與 toggle 並存
- 狀態：⬜ ｜ 相依：US-M2-06、US-M3-04、D-8

**US-BL-02** 身為使用者，我想要錄音時看到即時波形與本機 STT 的部分逐字稿 HUD，以便確認有收到音。
- 驗收：① 部分逐字稿只在記憶體、不送 API ② 不影響端到端延遲目標（§1.3）
- 狀態：⬜ ｜ 相依：US-M1-17、US-M2-13

**US-BL-03** 身為 Windows 使用者，我想要主動開啟 Right Alt 快捷鍵，以便沿用原始偏好（V8 選項 (a)，需 WH_KEYBOARD_LL 並更新 THREAT_MODEL T7）。
- 驗收：① 預設關閉 ② 偵測到 AltGr 配置時自動停用
- 狀態：⬜ ｜ 相依：D-2、US-M3-04

**US-BL-04** 身為使用者，我想要開啟生物辨識後在記憶體快取 key（閒置 10 分鐘或鎖定即清除），以便少跳提示（§6.2）。
- 驗收：① 快取清除時機有測試 ② 預設關閉
- 狀態：⬜ ｜ 相依：US-M1-04

**US-BL-05** 身為使用者，我想要 OpenCC 簡繁轉換，以便 raw 模式不出現簡體。
- 驗收：① 僅在 V7 結論建議且依賴獲批後實作
- 狀態：⬜ ｜ 相依：US-M1-26、D-5

**US-BL-06** 身為使用者，我想要更多翻譯目標語言，以便涵蓋更多場景（FR-8 延後項）。
- 驗收：① 每新增一種語言補守門語料
- 狀態：⬜ ｜ 相依：US-M1-12

**US-BL-07** 身為維護者，我想要 CI 快取或改用驗證過 checksum 的 cargo-deny / cargo-vet binary，以便縮短 CI 時間（0003 Consequences）。
- 驗收：① 供應鏈影響寫入 docs/changes 並經人工批准
- 狀態：⬜ 🧑 ｜ 相依：—

**US-BL-08** 身為使用者，我想要 Apple 未來把 fn+Z 收為系統快捷鍵時自動改用 Carbon 組合，以便升級 macOS 後不失效（V9 風險）。
- 驗收：① 啟動時偵測 tap 建立失敗或事件不到達 ② 提示並切換
- 狀態：⬜ ｜ 相依：US-M2-06

---

## 決策待辦

| ID | 待決定事項 | 影響 | 狀態 |
|----|------------|------|------|
| D-1 | Windows 預設快捷鍵確切組合（0004 定為 Alt+Shift+Z / Alt+Shift+X） | US-M3-04、README、mockup | ✅ 已決定；M3 手動測試需確認 Alt+Shift 不觸發輸入語言切換 |
| D-2 | 放寬 `unsafe_code`：Windows vault（CredReadW）、SendInput、剪貼簿格式、IME 都需要 Win32 FFI | US-M1-03/05、M3 | 🚧 vault 已批准（0005，只限 `core/src/vault/windows.rs`）；M3 的 SendInput / 剪貼簿 / IME 仍需 🧑 |
| D-3 | §14 白名單缺口：manifest 的 TOML 解析、CLI 的 wav 解碼、App Support 目錄定位（或自行實作） | US-M1-15/19/23 | ✅ 0005：manifest 改 JSON、WAV 自行實作、路徑由呼叫端傳入 |
| D-4 | cargo-vet 要信任哪些組織的稽核 | US-M1-24 | ✅ 0011：選項 (a)，匯入 registry 的知名組織稽核，其餘一次性 exemption |
| D-5 | V7 結論後是否引入 OpenCC | US-BL-05 | 🧑（待 M1-26） |
| D-6 | Windows CredMan target 是否也改為 `dev.rightson.keepsay.*` 風格（§6.2 仍為 `keepsay/anthropic`） | US-M1-05 | ✅ 0005：`dev.rightson.keepsay/anthropic` |
| D-7 | 翻譯守門門檻校正結果是否接受 | US-M1-12 | 🧑 待確認 [0007](../changes/0007-polish-prompts-and-guard.md) |
| D-8 | push-to-talk 是否從 v1.1 提前到 M2 / M3 | US-BL-01、FR-2 | 🧑 |
| D-9 | light / formal 模式的 prompt 內容（§6.5 未定義） | US-M1-09 | 🧑 草稿見 [0007](../changes/0007-polish-prompts-and-guard.md) |
| D-10 | uniffi 授權為 MPL-2.0，不在 deny.toml 允許清單：是否加逐 crate 例外，或改用其他 FFI 方式 | US-M1-22、M2、M4a | ✅ [0012](../changes/0012-ring-and-uniffi.md)：6 個 uniffi crate 逐 crate 例外 |
| D-11 | 是否批准 `uniffi/tokio` feature，讓 `polish`、`store_key`、`verify_key`、`download_model` 成為真正的 async 匯出（目前為阻塞 + 每次呼叫一個 current-thread runtime） | US-M1-22、M2-04 | 🧑 見 [0014](../changes/0014-m1-integration.md) |
| D-12 | M2 的 Swift binding 產生器：`uniffi` 的 `cli` feature（workspace 內 bin crate，或 workspace 外 `cargo install`），以及 core 加 `staticlib` / `cdylib` crate-type 或改由薄 FFI crate 承擔 | US-M2-04 | 🧑 見 [0014](../changes/0014-m1-integration.md) |

§16 人工事項：

| § 16 | 事項 | 擋住 | 狀態 |
|------|------|------|------|
| 1 | Apple Developer Program、Developer ID、App Store Connect API key | M2-17、M4a-06、M5-03 | 🧑 ⬜ |
| 2 | Windows 程式碼簽章服務與憑證 | M3-13、M5-04 | 🧑 ⬜ |
| 3 | GitHub 2FA、branch protection、release environment、secrets | M5-01 | 🧑 ⬜ |
| 4 | 開發用 Anthropic workspace 與 key（只放額度、不綁卡） | M1-01、M1-06 | 🧑 ⬜ |
| 5 | 商標（USPTO、TIPO、EUIPO）與 GitHub 同名確認；名稱與網域 `keepsay.rightson.dev` 已決定 | M1 結束前（避免 vault 識別字遷移） | 🧑 🚧 |
| 6 | 提交 Homebrew cask 與 winget manifest | M5-06 | 🧑 ⬜ |
| 7 | 授權：已決定 Apache-2.0 | US-M0-10 | ✅（套用中） |
