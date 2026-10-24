# 0010 — STT 與 CLI（M1 工作流 E）

- Status：Accepted
- Date：2026-10-24
- 影響：`core/src/stt/*`、`cli/*`、`core/tests/no_disk_writes.rs`、`core/tests/stt_whisper.rs`、`cli/tests/cli.rs`、ROADMAP（US-M1-16、17、23、25）。PROPOSAL 不變。

## Context

0005 把 STT 與 CLI 分給工作流 E，並留下幾件事由 E 決定：非 16 kHz 輸入要重取樣還是拒絕、WAV 讀取的範圍、CLI 怎麼在沒有 clap 的情況下解析參數、key 怎麼輸入、未簽章的 CLI 在 macOS 寫不進 data-protection keychain 時怎麼開發。

## Decision

### 1. 重取樣：core 內的線性內插

- `stt::resample_linear` 接受 8–192 kHz，轉成 16 kHz。`WhisperEngine::transcribe` 一律先呼叫它，所以 `SttEngine` 的 `sample_rate` 參數真的有作用。
- 不做抗混疊低通濾波：44.1 / 48 kHz 輸入中 8 kHz 以上的能量會折回頻帶內。對語音辨識影響小，而且各平台錄音本來就直接錄 16 kHz，這條路徑主要給 CLI 讀 WAV 檔用。品質不足時再換成多相濾波，不需要新依賴。
- 上限 30 分鐘（16 kHz 下 28.8M 樣本），遠低於 whisper.cpp 的 `c_int` 樣本數上限。非有限值（NaN、∞）轉成 0。

### 2. whisper 參數

- 語言：`SttOptions.language` 取 BCP-47 主標籤（`zh-TW` → `zh`），未設定時用 `zh`，`auto` 交給 whisper 自動偵測，其他格式回 `CoreError::Config`。
- initial prompt：固定的台灣繁中範例句（含中英夾雜），後接「詞彙：A、B。」。詞彙表去掉控制字元（whisper-rs 遇到 NUL 會 panic）、跳過空字串，總長上限 200 字元，因為 whisper 只看 prompt 最後約 224 個 token，太長會把範例句擠掉。
- 不用 translate task（§6.7、ADR-9）。關閉所有 `print_*`，否則 whisper.cpp 會把逐字稿直接印到 stdout。載入模型前呼叫 `install_logging_hooks()`，丟棄 whisper.cpp / GGML 的輸出。
- tracing 只記樣本數、取樣率、prompt 與輸出的字元數、耗時。模型路徑不記錄，也不放進錯誤。

### 3. WAV 讀取（CLI，無新依賴）

- RIFF/WAVE；PCM 16-bit 與 IEEE float 32-bit，包含 WAVE_FORMAT_EXTENSIBLE；1–8 聲道平均成單聲道。
- 不信任 RIFF 長度欄位；略過未知 chunk 與奇數長度的 pad byte；`fmt ` 可在 `data` 之後；`data` 長度超出檔案時（串流錄音常寫 0 或 0xFFFFFFFF）截到實際長度，其他 chunk 截斷則回錯；丟掉最後不完整的 frame。
- 錯誤是 `WavError`（不帶檔案內容或路徑）。單元測試用程式產生的 byte buffer，proptest 確認任意輸入不會 panic、解出的樣本都在 [-1, 1]。
- 檔案大於 1 GiB 直接拒絕。

### 4. CLI 介面

手寫參數解析（clap 不在 §14 白名單）。錯誤訊息只寫旗標名稱或位置，不回顯參數內容，避免使用者誤把 key 打在命令列時又被印到終端機或 CI log。

    keepsay-cli transcribe <file.wav> --model <ggml.bin> [--lang <bcp47>]
    keepsay-cli polish (<file.wav> --model <ggml.bin> | --text-stdin) [--mode M] [--translate en|zh-TW|ja] [--lang]
    keepsay-cli <file.wav> --model … [--mode] [--translate]      # US-M1-23 的簡寫
    keepsay-cli key set | status | delete | verify
    keepsay-cli models list | download <id> | import <id> <file>
    keepsay-cli budget
    全域：--data-dir <dir>、--dev-mock-vault、-v、--help、--version

- stdout 只輸出成品（逐字稿或潤稿結果），由單一函式 `emit_result` 負責；其他訊息一律走 stderr。
- `--translate` 未指定 `--mode` 時用 polish（FR-8）。
- `models import` 需要 id，因為 `downloader::import` 要用 manifest 的那一筆做 SHA-256 驗證。
- 資料目錄（config.json、budget.json、models/）：`--data-dir`，否則 macOS 為 `~/Library/Application Support/dev.rightson.keepsay`，Windows 為 `%APPDATA%\dev.rightson.keepsay`，其他平台為 `$XDG_DATA_HOME/keepsay` 或 `~/.local/share/keepsay`。設定檔不存在時用預設值，不建立檔案。只有 API 呼叫後的 budget.json 與 `models download / import` 會寫檔。
- `--model` 直接接受路徑，不比對 manifest 雜湊：這是開發工具，manifest 目前也還沒有任何模型（US-M1-18）。App 端只載入通過 SHA-256 驗證的檔案（INV-7）。
- 潤稿前的任何錯誤（沒有 vault、建立 client 失敗）都輸出 raw 逐字稿並在 stderr 提示（FR-4）。
- 自訂 panic hook 只印位置，不印 payload（§6.9）。

### 5. key 輸入

- 只從 stdin 讀：stdin 是終端機時以隱藏輸入提示，否則讀管線的第一行。永遠不接受 argv 或環境變數。`key set` 後面多帶任何參數都會被拒，並提示若剛剛打的是真的 key 就去 Console 撤銷。
- 隱藏輸入不用 `unsafe` 也不加依賴：Unix 上對繼承的終端機執行系統的 `stty -echo`，結束時（包含錯誤路徑，用 drop guard）執行 `stty echo`。其他平台無法不靠 FFI 關閉回顯，會先警告輸入可見，建議改用管線。
- 讀到的那一行在轉成 `SecretString` 後立即 zeroize。
- `key set`：格式檢查 → `verify_key` → 存入 vault → 顯示 fingerprint。驗證失敗不存（FR-1）。
- `key status` 與 `key verify` 需要 fingerprint，CLI 呼叫 `KeyVault::load` 後立即 drop，只有 fingerprint 離開。

### 6. `--dev-mock-vault`

- macOS 上未簽章的 CLI 寫不進 data-protection keychain，Linux 沒有 vault 後端。core 的 `MockVault` 只在 `test-support` feature 下公開，而 `cli/Cargo.toml` 凍結、也不該讓 CLI 依賴 `test-support`，所以 CLI 自己實作公開的 `KeyVault` trait（`cli/src/session_vault.rs`）。
- 只編進 debug build（`cfg(debug_assertions)`）；release 的 CLI 收到這個旗標會直接拒絕。
- key 只在這次執行的記憶體中，以 `SecretString` 保存，結束即消失，不寫入任何地方；仍然只有 `AnthropicClient` 能把它送出去。每次使用都在 stderr 印出醒目警告。
- 需要 key 的指令（API 潤稿、`key verify`）會先讀 key：終端機用隱藏提示，管線則是第一行，之後的內容才是 `--text-stdin` 的文字。

### 7. 測試

- `core/src/stt/mod.rs`：重取樣長度與內插、語言對應、initial prompt 的結構與長度上限（不斷言詞彙內容）。
- `core/tests/no_disk_writes.rs`（US-M1-25 ②）：文字掃描 `stt/` 與 `polish/`，不得出現 `std::fs`、`fs::`、`File::`、`OpenOptions`、`tempfile`、`temp_dir`。
- `core/tests/stt_whisper.rs`、`cli/tests/cli.rs`：設定 `KEEPSAY_TEST_WHISPER_MODEL` 指向本機 ggml 模型才跑端到端辨識，否則印出原因並略過；測試不下載模型。只斷言長度與結束碼。
- `cli/tests/cli.rs` 另外確認：key 放在 argv 會被拒且不回顯；raw 模式與錯誤路徑不在資料目錄產生檔案；`--dev-mock-vault` 拒絕格式錯誤的 key 且不回顯。
- 在 Linux 上以 ggml-tiny 對合成的 48 kHz 立體聲正弦波 WAV 跑過 `transcribe`：成功結束，stderr 只有長度與耗時，whisper.cpp 沒有輸出，資料目錄沒有新增檔案。

## Consequences

- `cli/Cargo.toml` 的 `[dev-dependencies]` 加入 proptest（0005 已列的白名單 crate，Cargo.lock 只多一條依賴邊，沒有新 crate）。
- polish、key、budget、models 指令依 0005 的簽章撰寫，要等工作流 A–D 合併後才會真的執行；目前會停在對方模組的 `todo!`（panic hook 只印位置）。
- 建議工作流 C 在 vault 提供 `stored_fingerprint(&dyn KeyVault) -> Result<Fingerprint>`，讓 CLI、FFI 都不必自己呼叫 `load`。採用後 CLI 改呼叫它即可。
- 線性重取樣沒有抗混疊。若 M1-26（V7）的樣本顯示 44.1 / 48 kHz 檔案的辨識品質明顯較差，再改進。
