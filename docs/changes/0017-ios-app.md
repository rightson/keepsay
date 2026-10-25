# 0017 iOS 主 App（M4a）

## Context

M4a 要做 iOS 主 App：錄音 → 本機 STT → 潤稿 → 剪貼簿，加上兩個 App Intent（PROPOSAL §7.2、§13）。開發環境是 Linux，沒有 Xcode，只能靠 GitHub Actions 的 macOS runner 編譯。與 M2（macOS）平行開發：共用的 KeepsayKit（UniFFI core bridge、錄音器、設定模型）與 XCFramework 建置腳本由 M2 負責，uniffi FFI 由整合者完成，M4a 開工時兩者都還沒有。

## Decision

1. **專案形式**：手寫 `apple/KeepsayiOS/KeepsayiOS.xcodeproj`（objectVersion 77，使用 `PBXFileSystemSynchronizedRootGroup`），兩個 target：`KeepsayiOS`（App）與 `KeepsayiOSTests`（XCTest，host 為 App）。資料夾同步式群組不需要逐檔列入 pbxproj，新增 Swift 檔不用改專案檔。不用 XcodeGen、Tuist 等工具，也不用任何第三方 Swift 套件（§9）。iOS 17 起（`@Observable`），只支援 iPhone，Swift 5 語言模式，`SWIFT_TREAT_WARNINGS_AS_ERRORS = YES`。
2. **與 core 的邊界**：`CoreService` protocol 對應 §6.1，刻意不提供任何回傳 key 的方法（INV-2）。KeepsayKit 合併前，用 `UnlinkedCore` 佔位，它不存任何東西：沒有 key、模型都標成未安裝、潤稿一律退回 raw（`coreUnavailable`）。合併後再寫一個轉接 KeepsayKit 的實作，只需要替換 `AppModel.shared` 的建構參數。App 內鏡像 core 的 `Mode`、`TargetLang`、`FallbackReason`、`PolishResult`、`BudgetStatus`（`Logic/Domain.swift`），讓純邏輯與測試不依賴產生出來的 bindings。
3. **STT**：依 V1（尚未驗證），預設走 core 的 WhisperEngine。Apple Speech 留待之後選用。依實體記憶體推薦模型：< 3.5 GiB 用 base、< 7.5 GiB 用 small、其餘用 large-v3-turbo（US-M4a-04，門檻是初始值，實測峰值之後記錄）。
4. **狀態機**：`DictationMachine.reduce` 是純函式（idle → recording → transcribing → polishing → done / failed），副作用由 `AppModel` 依轉移執行。`DictationPlan.make` 在開始錄音時決定本次的 mode 與翻譯目標（FR-2）：
   - 「潤飾」使用首頁選的模式。沒有 key 或達到硬上限時改用 raw，並提示原因。
   - 「潤飾並翻譯」一律走 polish + 目標語言。raw 模式、沒有 key、達到硬上限時輸出原文，提示「翻譯需要 API，已輸出原文」（FR-8）。
   - core 回報 fallback，或模型輸出是空白時，輸出未翻譯的逐字稿，不輸出半成品。
   - phase 與 output 的 `description` / `debugDescription` 都經過遮蔽，只顯示長度，不含內容。
5. **剪貼簿（T6）**：`ClipboardPolicy` 是唯一寫入 pasteboard 的地方。它用 `UIPasteboard.setItems(_:options:)` 寫入，選項為 `[.localOnly: true, .expirationDate: now + 60 s]`，內容是單一個 `public.utf8-plain-text` item。CI 會用文字掃描確認其他檔案沒有存取 pasteboard。
6. **Key 輸入**：`SecureField` 送出時先把欄位清空，再把值交給 `storeKey`。刻意不設 `.textContentType(.password)`，避免系統提示把 key 存進會同步的 iCloud 鑰匙圈（T3）。沒有顯示 key 或複製 key 的功能。Swift 的 `String` 無法 zeroize，這是 INV-2「Swift 只在輸入那一刻經手一次」本來就接受的範圍。
7. **App 本身不碰 Keychain**：只有 Rust core 存取 Keychain。App 沒有 entitlements 檔，也沒有 `keychain-access-groups`，所以不存在共享的 access group（INV-10 的前置條件，M4b 的鍵盤讀不到）。
8. **App Intents**：`PolishDictationIntent`、`PolishAndTranslateDictationIntent`，都設 `openAppWhenRun = true`，因為錄音需要 App 在前景。行為是 toggle：第一次執行開始錄音，第二次停止並處理。Intent 不把逐字稿或結果回傳給捷徑，避免內容進入捷徑變數、被其他動作轉送。結果只寫入本機、會過期的剪貼簿。
9. **離開即丟棄**：App 進入背景時，錄音中就取消（音訊丟棄），完成畫面的結果從記憶體清掉（mockup 的「離開後不保留」）。v1 不申請 background audio（那是 M4b session 模式的事）。
10. **CI**：`.github/workflows/ios-app.yml` 有三個 job：
    - `guards`（ubuntu）：用文字掃描確認 Swift 裡沒有 URLSession / NWConnection（INV-3、INV-7）、沒有 SecItem / kSecAttr / keychain-access-groups（INV-1、INV-10）、沒有 print / NSLog / os_log / Logger、沒有 AVAudioFile / AVAudioRecorder（INV-5）、pasteboard 只在 ClipboardPolicy 裡使用、沒有 remote Swift package。
    - `build-test`（macos-15）：在 iOS Simulator 上跑 XCTest，接著建置 Release 版的 simulator `.app`，壓成 artifact 上傳。
    - `testflight`：`if: false` 的佔位 job。
    - 不簽章，也不使用任何 secret。

## Consequences

- KeepsayKit 合併後要做的事：
  - 新增 `CoreService` 的 KeepsayKit 實作。
  - 把 KeepsayKit 加為 `XCLocalSwiftPackageReference`，必要時也加 XCFramework。
  - 若 KeepsayKit 提供共用的錄音器，用它取代 `MemoryRecorder`。AVAudioSession 與麥克風權限仍由 iOS 端負責。
  - 設定（`SettingsStore`，JSON 存在 Application Support，套用 complete file protection，帶 schema version）改為走 core 的 `config`。
  - 預算的上限與價目表改由 core 的 config 提供。
- 需要實機或 Apple 帳號才能做的事：
  - TestFlight（§16 第 1 項，US-M4a-06）。
  - App Store 隱私標示「不收集資料」。
  - 實測模型記憶體峰值（US-M4a-04）。
  - Action Button 綁定、從捷徑實際執行 Intent。
  - 確認 `.localOnly` 的資料不會出現在 Universal Clipboard。
- 已知限制：沒有 App icon（asset catalog 留到發佈前再加）。Shortcuts 片語只有繁中。

## 補充：接上 KeepsayKit（2026-10-25）

KeepsayKit（M2）與 core 的 UniFFI 介面（0014）合併後，iOS App 改為透過 KeepsayKit 使用 core：

1. **連結方式**：`KeepsayiOS.xcodeproj` 以 `XCLocalSwiftPackageReference`（`../KeepsayKit`）連結 KeepsayKit，只連到 App target，測試 target 不另外連結（避免同一個 static library 在 host App 與測試 bundle 各有一份）。KeepsayKit 的 `Package.swift` 在 `Frameworks/KeepsayCoreFFI.xcframework` 與產生的 bindings 都存在時才連結 core，否則 `CoreBridge.make()` 回傳 `UnavailableCore`（只能用原文模式、不存任何東西）。原本 App 內的 `UnlinkedCore` 移除。
2. **`KitCoreService`**：`CoreService` 的唯一實作，也是 App 內唯一 `import KeepsayKit` 的檔案（ios-app.yml 新增文字掃描）。KeepsayKit 與 App 有同名型別（`AppSettings`、`PolishOutcome`、`BudgetSnapshot`…），這個檔案一律寫 `KeepsayKit.` 前綴，其他檔案看不到 KeepsayKit 的型別。
3. **一律離開 main thread**：core 的 FFI 都是阻塞呼叫（`has_key` 也要讀鑰匙圈屬性），所以 `CoreService` 改成全部 async，`KitCoreService` 每個呼叫都包在 `Task.detached`。`AppModel` 的狀態（有沒有 key、fingerprint、預算、已安裝模型）一次讀成 `CoreStatus`，在進入前景、key 變更、每次聽寫後更新。開始錄音時用最後一次的狀態決定計畫；core 每次呼叫都會再檢查 key 與硬上限，所以狀態過期也無法繞過。
4. **錯誤對應**：core 的 `CoreFailure` 轉成不含內容的 `CoreServiceError` / `FallbackReason`；`VaultNotEntitled`（未簽章，-34018）顯示「無法寫入鑰匙圈（未簽章的開發版）」，不退回其他儲存方式。轉寫錯誤不再先檢查「模型已安裝」，改由 core 回報（`ModelUnknown` / `ModelHash` → 「尚未下載語音模型」）。
5. **初始化**：data dir 由 KeepsayKit 的 `UniffiCore` 決定（App 容器內的 Application Support），App 不另外指定。設定變更時把非機密設定交給 core（`saveSettings`）。
6. **其他**：軟上限每個月提醒一次（`SoftLimitNotice`，記在設定的 `softLimitWarnedMonth`，硬上限有自己的提示，不重複提醒）；§12 安全建議改為 v0.4 內容（專用的非預設 workspace、Billing → Spend limits；撤銷與自動儲值的 Console 位置尚未確認，所以不寫選單路徑）；onboarding 加步驟點；未連結 core 時設定頁顯示開發版提示。
7. **測試**：`AppModelTests` 以假的 core 與錄音器測 key 流程（只看 fingerprint）、聽寫各路徑、翻譯沒有 key 時不呼叫 API、硬上限、Intent toggle、設定送到 core、軟上限只提醒一次；`KitCoreServiceUnlinkedTests` 測沒有 core 時的行為。

### 合併 M2 的 bindings（0019）之後

- M2 補上 `get_settings` / `save_settings` / `list_models`：`apply` 送出的設定（模式、目標語言、`stt_model`、詞彙表、價目表、上限）寫入 core 的 `config.json`，`transcribe` 因此用 App 選的模型；已安裝狀態改由 core 回報。data dir 為 App 容器內的 Application Support/`dev.rightson.keepsay`。
- iOS 的 `settings.json` 仍保留 App 專屬欄位（`onboarded`、`softLimitWarnedMonth`），也保留一份同樣的非機密設定；啟動時與每次修改時都推給 core，iOS 上 App 是 config.json 唯一的寫入者。之後可改為啟動時從 core 讀回，只在 App 端保留 App 專屬欄位。
- ios-app.yml 新增 `build-test-core` job：`KEEPSAY_CORE_PLATFORMS="ios-sim ios" apple/scripts/build-core.sh` → 連結 core 跑 XCTest（`KEEPSAY_EXPECT_CORE=1`，`LiveCoreTests` 不准略過）→ Release 建置 simulator 與 device，以 `nm` 確認有連結 core 的 `store_key`、沒有任何讀 key 的匯出，Info.plist 沒有 keychain 項目 → 上傳連結 core 的 simulator `.app`。原本不連結 core 的 `build-test` 保留。
- `LiveCoreTests` 只測不需網路的路徑：狀態來自 core（fingerprint 為「…」加四碼）、格式不符的 key 被 core 擋下且已存的 key 不變、raw 不呼叫 API、manifest 以外的模型被拒。

## Status

Accepted（M4a 進行中；App 已連結 keepsay-core，TestFlight 與實機驗證待 §16 第 1 項）
