# 信任邊界與負向驗收

在正式實作前定義；外部網頁與控制面板使用不同 WKWebView。

| 嘗試／故障 | 預期 | 驗證 |
| --- | --- | --- |
| 網址輸入 javascript、file、data、自訂 scheme | 拒絕，不啟動系統程式 | normalize_address 負向測試 |
| 網址夾帶帳密、控制字元 | 拒絕 | normalize_address 負向測試 |
| 外部網頁呼叫控制 IPC | 沒有註冊該 IPC handler | 建構路徑獨立審查 |
| 控制面板導向遠端或開新視窗 | 拒絕導覽與新視窗 | 建構路徑獨立審查 |
| 網站標題／網址注入 HTML | textContent、JSON 序列化，不使用 innerHTML | UI 原碼審查 |
| 設定格式損毀／未來版本 | 回報錯誤，不覆寫原檔 | 真實暫存目錄整合測試 |
| 保存失敗 | 記憶體狀態不提交、顯示錯誤 | 狀態提交路徑＋I/O 測試 |
| 兩個程序同時執行 | 第二個拒絕寫入 | 檔案鎖整合測試 |
| 螢幕有負座標、縮放不同 | 以 AppKit 邏輯座標計算 | geometry 單元測試 |
| 持續停在邊緣／手動收合 | 必須移開才可再次觸發 | panel 狀態測試 |

不執行使用者登入、購買或外部提交；完整 UI E2E 需另行要求。


## 0.2 行為與故障回歸

- 改名、移動、移除再復原仍保留識別碼；順序與名稱可在重啟後讀回。
- 控制字元／空白名稱、無效 ID／位置、復原重複 ID 與超過容量均拒絕。
- 真實唯讀目錄使保存於 rename 前失敗，原設定 bytes 不變。
- 注入 rename 後的 directory sync 失敗，回傳「已提交但有警告」，重開讀到新設定。
- 讀取設定以 256 KiB 上限保護記憶體；測試上限與超限邊界。
- 原生選單 smoke 從遠端 responder 切至本機網址／新增輸入框，核對 native 與 DOM focus，並驗證收合。


## 0.3 快捷鍵設定驗證

| 狀態／輸入 | 預期 | 證據範圍 |
| --- | --- | --- |
| 舊 schema 1 缺快捷鍵欄位 | 沿用 ⌘⇧Space | model 實際反序列化 |
| 無修飾鍵、shift-only、未知鍵、App 保留鍵 | 拒絕，不註冊／保存 | shortcut 模型測試 |
| 新鍵註冊衝突 | 原快捷鍵與磁碟不變 | 可注入 registry 交易測試 |
| 保存前失敗 | 解除新鍵、原鍵維持 | 交易測試＋真實檔案保存測試 |
| rename 後同步警告 | 新鍵與新設定一致、顯示警告 | 交易測試＋store fault injection |
| 舊鍵解除失敗 | 不反轉已保存設定，提示重啟 | 交易測試＋desktop source trace |
| 舊 ID 事件遲到 | 忽略，只接受目前 ID | 原生工程 smoke |
| 新表單套用／來回更換 | 原生註冊、store.load、UI 提示一致 | WebKit IPC＋Carbon 工程 smoke |
| 已保存預設值但表單有草稿，再恢復預設 | 控制項與送出值皆回預設 | Node DOM regression |

工程 smoke 不送出系統按鍵，不代表所有 macOS 保留組合或键盤配置已實測；註冊錯誤測試不承諾作業系統可偵測所有快捷鍵冲突。


## 0.4 滑鼠尺寸調整

- 舊 schema1 無 height/top_offset 時維持全高與頂端8pt；新尺寸與位置可跨store重啟。
- 模型拒絕非法尺寸、非finite及越界位置，不覆寫原檔。
- 幾何涵蓋左／右、負座標、頂底clamp、小螢幕與最少1pt可用區。
- 原生工程smoke檢查resizable style、NSWindow frame、child WebView尺寸、拖曳狀態禁止hide、保存讀回。
- 對獨立smoke目錄暫設唯讀，確認保存失敗恢復frame/settings，再恢復原權限。
- 本機grip只接受左鍵pointerdown IPC；Rust另查真實左鍵、游標位於把手與可見視窗，遠端仍無IPC。
- 未執行實際系統滑鼠拖曳E2E、各種外接顯示器與縮放設定的實機測試。
