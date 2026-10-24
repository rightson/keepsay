# 0001 — 預設快捷鍵與「潤飾並翻譯」

- Status：Accepted（功能與預設組合）／Proposed（實作方式：V8、V9 待決定）
- Date：2026-10-24

## Context

使用者指定了桌面預設快捷鍵，並新增一個 v0.1 沒有的功能：潤飾後翻譯到目標語言。

| 平台 | 潤飾 | 潤飾並翻譯 |
|------|------|------------|
| Windows | Right Alt | Right Shift + Right Alt |
| macOS | fn + Z | fn + Shift + Z |

v0.1 的快捷鍵方案是 Carbon RegisterEventHotKey（macOS）與 RegisterHotKey（Windows），兩者刻意選擇「不需要額外權限、不讀全域按鍵」。新的預設組合都無法用這兩個 API 註冊：

- Windows：RegisterHotKey 不能註冊單獨修飾鍵、不分左右。
- macOS：Carbon 熱鍵沒有 fn 修飾鍵。

## Decision

1. 採納預設組合與翻譯功能，寫入 PROPOSAL v0.2：FR-2、FR-8、§6.1、§6.4、§6.5、§6.6、§7.1、§7.2、§7.3、§11、§13、ADR-9。
2. 翻譯在潤飾的同一次 API 請求內完成（ADR-9），不新增網路出口，INV-3 / INV-4 不受影響。
3. 翻譯守門改用「內容單位」（CJK 字元數 + 非 CJK 詞數），比例門檻為 0.3× ~ 3× + 30，並加上目標語言文字比例 ≥ 60% 的檢查。門檻在 M1 以語料校正。
4. 實作路徑不在此決定。兩者都要在「讀取全域按鍵」與「預設組合」之間取捨，屬於安全面的設計變更，列為 V8（Windows）、V9（macOS）交由使用者決定。在決定前：
   - 程式中保留 v0.1 的 RegisterHotKey / Carbon 方案作為備援組合。
   - 快捷鍵一律可在設定中修改。

## Consequences

- 若 V8 選 (a) 或 V9 選 (a)/(b)：
  - App 會具備讀取全域按鍵的能力，THREAT_MODEL 要新增「按鍵監聽被濫用」的情境（T1 被利用時損害擴大），§7.3「不使用低階鍵盤 hook」要改寫。
  - macOS 需要「輸入監控」或「輔助使用」權限的 onboarding 說明。
- AltGr 鍵盤配置與無 fn 鍵的外接鍵盤需要自動 fallback，已加入 §11 手動測試清單。
- 翻譯讓 output token 增加，max_tokens 公式在翻譯時改為 ×3。預算估算不變（同一張價目表）。
- 失敗時一律輸出「未翻譯的原文」，不輸出半翻譯結果，與 FR-4「任何錯誤都退回 raw」一致。
- mockup（docs/mockups/）的快捷鍵顯示需要在 M2 / M3 實作時更新。
