# keepsay 威脅模型

> 對應 PROPOSAL §2 與 §3。本檔是威脅模型的詳細版；兩者衝突時以本檔為準，並同步修正 PROPOSAL。
> 每條對策都標出它由哪個 INV、哪個測試或哪個 CI 檢查保證。沒有自動化保證的對策標「人工」。

## 1. 範圍

- 涵蓋：keepsay-core、keepsay-cli、macOS / iOS App、Windows App、CI 與發佈管道。
- 不涵蓋：Anthropic API 本身、使用者的 Anthropic 帳號安全（2FA、Console 權限）、OS 本身被攻破（kernel / root）。

## 2. 資產

| ID | 資產 | 失去它的後果 | 存放位置 |
|----|------|--------------|----------|
| A1 | Anthropic API key | 財務損失（被盜刷）；帳號被濫用 | OS 安全儲存區；使用時短暫存在 Rust core 記憶體 |
| A2 | 音訊、逐字稿、潤稿/翻譯結果、詞彙表 | 隱私外洩 | 記憶體；結果寫入目標 App 或剪貼簿；詞彙表存在非機密設定 |
| A3 | 使用者當下聚焦的 App | 被插入錯誤、惡意或被扭曲的內容 | — |
| A4 | 發佈管道的完整性（v0.2 新增） | 所有使用者同時被植入惡意版本 | GitHub、CI、簽章憑證 |

## 3. 信任邊界

    ┌ 使用者裝置 ────────────────────────────────────────────────────────┐
    │  UI 層（Swift / WebView）  ── FFI / IPC ──▶  keepsay-core（Rust）  │
    │     只在輸入當下經手 key 一次          唯一持有 key 的地方          │
    │                                              │                      │
    │  OS 安全儲存區（Keychain / CredMan） ◀───────┘                      │
    └──────────────────────────────────────────────┼──────────────────────┘
                                                   │ TLS（平台驗證）
                            api.anthropic.com ◀────┘   （唯一帶 key 的出口）
                            模型下載來源 ◀──────────── ModelDownloader（無 key）

- B1：UI ↔ core。key 只能單向流入 core（store_key），不得流出（INV-2）。
- B2：core ↔ 網路。key 只能出現在 AnthropicClient 送往 BASE_URL 的請求中（INV-3）。
- B3：core ↔ 模型（LLM）。模型的輸出只是資料，它無法觸發任何動作（INV-4），並且要先經過守門（§6.6）才會插入。
- B4：repo ↔ 使用者手上的 binary。只有 CI 的產出可信（INV-8）。

## 4. 威脅與對策

### T1 供應鏈被入侵
- 情境：惡意依賴版本、被接管的 GitHub Action、維護者帳號被盜、發佈物被替換。
- 對策：
  - 依賴白名單與人工批准（INV-9）。由 `deny.toml` 的 bans/sources 保證，`cargo vet` 稽核每個依賴版本。
  - Actions 釘 commit SHA，workflow `permissions: contents: read`，PR workflow 不使用 secret（CI 設定，人工 review）。
  - 只由 CI 發佈，附簽章與 provenance（INV-8，M5）。
  - 沒有自動更新（ADR-5），惡意版本無法自動推送到所有使用者。
- 殘餘風險：維護者帳號被接管；人工 review 漏看惡意變更。

### T2 同機、同權限的惡意程式
- 情境：使用者執行了惡意程式，它嘗試讀取 Keychain、記憶體、設定檔、剪貼簿。
- 對策：
  - key 只存在 OS 安全儲存區，設定為 ThisDeviceOnly 與 LOCAL_MACHINE（INV-1，金絲雀測試）。
  - 預設不在記憶體快取 key，每次請求都重新讀取，用完就 zeroize（INV-2）。
  - key 不經過剪貼簿、不寫檔（金絲雀測試）。
- 殘餘風險：根本無法完全防禦。同權限程式可以 attach debugger、注入 dylib/DLL，或在使用者解鎖時存取 Keychain 項目（macOS 會跳出存取提示，Windows CredMan 不會）。所以損失上限必須由帳號端保證（PROPOSAL §12）。

### T3 使用者無意外洩
- 情境：log、crash report、螢幕截圖、Time Machine / iCloud 備份、漫遊設定檔。
- 對策：
  - tracing 只記錄事件、長度與耗時；panic hook 不記錄 payload；SecretString 的 Debug 輸出是 `[REDACTED]`（vault 單元測試、金絲雀測試）。
  - Keychain 項目設定為 ThisDeviceOnly，不進 iCloud Keychain、不進備份還原到其他裝置（INV-1）。
  - UI 只顯示末四碼，沒有「顯示 key」或「複製 key」的功能（mockup 與 code review）。
- 殘餘風險：使用者在輸入時自行截圖。

### T4 網路 MITM 或惡意 proxy
- 對策：
  - 平台 TLS 驗證（rustls-platform-verifier）。
  - 預設 `no_proxy()`；禁止 redirect；BASE_URL 完全比對（`core/tests/egress.rs`）。
- 殘餘風險：裝置被安裝了惡意根憑證（ADR-4 決定不做 pinning）。

### T5 逐字稿內含 prompt injection
- 情境：使用者口述或朗讀的內容中夾帶「忽略以上指示…」等文字。
- 對策：
  - 逐字稿以 `<transcript>` 包裹並移除標籤字串（`polish::prompt` 單元測試）。
  - 請求型別沒有 tools 欄位（INV-4，`core/tests/request_type.rs`）。
  - 輸出守門：長度比例、標籤殘留、前言、翻譯時的目標語言比例（§6.6）。
- 殘餘風險：潤稿或翻譯的語意被扭曲，但只限於文字層，模型無法執行任何動作。

### T6 剪貼簿被其他 App 讀取
- 對策：
  - 優先用 AX / SendInput 插入。
  - 不得不用剪貼簿時，加上 Concealed/Transient 或 Exclude*/CanInclude*=0 格式，並且立即還原原內容。
  - iOS 設定 localOnly 與 60 秒 expiration。
- 殘餘風險：不遵守這些標記的剪貼簿工具。

### T7 全域按鍵監聽能力被濫用（v0.2 新增，取決於 V8 / V9 的結論）
- 情境：若採用 WH_KEYBOARD_LL、Raw Input 或 CGEventTap 實作 Right Alt 或 fn 快捷鍵，App 就具備讀取全域按鍵的能力。一旦 T1 發生（發佈物被植入惡意程式碼），損害就從「偷 key」擴大為「keylogger」。
- 候選對策：
  - 回呼只判讀特定 virtual key / flag，不保存、不轉送其他按鍵。
  - 這段程式碼隔離在單一小模組，並列入 code review 必查項目。
  - 預設使用不需要監聽權限的組合，監聽式快捷鍵由使用者主動開啟。
- 狀態（v0.3 變更，見 docs/changes/0004-license-domain-hotkeys.md）：V8 / V9 已決定。
  - Windows：不使用監聽。預設 Alt + Shift + Z / X 以 RegisterHotKey 註冊，App 只收到自己註冊的組合。Right Alt 延到 v1.1 以後，屆時需重新評估本節。
  - macOS：採監聽。fn + Z / fn + Shift + Z 以 active CGEventTap 實作，App 能看到全域 keyDown / keyUp。對策：
    - 回呼只比對 Z 的 keycode 與 fn、Shift 旗標；不保存、不記 log、不轉送任何按鍵，非命中事件原封不動放行。
    - tap 程式碼隔離在單一 Swift 檔案，列入 code review 必查項目；只訂閱 keyDown / keyUp。
    - 權限沿用 AX 插入已需要的「輔助使用」，不申請「輸入監控」。Secure Input 期間（密碼欄位）系統不會把事件交給 tap。
    - 不授予輔助使用權限時退回 Carbon 備援組合（⌥ Space / ⌥ ⇧ Space），不需任何監聽權限。
  - 待補（M2）：自動化檢查。例如 CI 以文字掃描確認 tap 模組不呼叫檔案、網路、剪貼簿或 logging API，也不把 event 存進集合。

## 5. 根本限制（對外說明，同步放在 README 與 SECURITY.md）

key 存在使用者的裝置上，就不可能完全防住以同一使用者權限執行的惡意程式。keepsay 保證的是：

1. key 靜態存放時不會有明文落地。
2. key 在記憶體中曝露的時間極短。
3. key 只能被送往一個寫死的目的地。
4. 外洩後的損失由帳號端設定封頂：專用 workspace、不綁付款方式、設定 spend limit。

## 6. 維護

- 新增網路出口、權限、依賴、IPC command 或 FFI 函式時，都要更新本檔。
- 每個里程碑結束時重新檢查 §4，對策對應的測試若有變動要同步更新。
