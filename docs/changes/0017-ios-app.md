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

## Status

Accepted（M4a 進行中；core 連結待 KeepsayKit 與 m1-integration）
