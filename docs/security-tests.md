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


## 0.4.3 輸入放行規則與設定復原

矩陣先於實作建立，並先對舊實作取得失敗結果。

| 嘗試／故障 | 預期 | 驗證 |
| --- | --- | --- |
| 網址列輸入 `host:port`（`localhost:3000`、`example.com:8080/a`） | 補上 scheme 後開啟；只有 http/https 進入 WebKit | normalize_address 測試＋「接受的輸出必為 web_url」測試 |
| 含空白且開頭像 scheme（`site:x y`、`javascript:alert(1) //x`） | 當成 DuckDuckGo 搜尋字串，經 URL 編碼 | normalize_address 測試 |
| 無空白的非 http(s) scheme、畸形埠、帳密變體（`foo:bar`、`nas:5000`、`evil.com:80@example.com`、`example.com:99999`、`https://example.com:8a/`、全形數字埠） | 拒絕 | normalize_address 負向測試 |
| 百分比編碼後超過 8192 位元組的網址或搜尋 | 拒絕；回傳值一律以實際要載入的字串再過一次 web_url | 「接受的輸出必為 web_url」測試（含 8192 邊界） |
| 頭尾空白的網址（`Url::parse` 會默默剝除） | web_url 在解析前拒絕 | navigation_allowed 負向測試 |
| iframe 導向 `about:blank`（可帶 query／fragment）、`about:srcdoc`（只能帶 fragment）、來源為 http(s) 的 `blob:` | 放行 | navigation_allowed 測試＋原生 smoke |
| 任何形態的位址含空白、控制字元，或整個位址超過 8192 位元組 | 拒絕；前置條件對 http(s)、`blob:`、`about:` 一視同仁 | 「前置條件 × 位址形態」矩陣測試 |
| `about:srcdoc?query`、`about:config#about:blank`、`javascript:…#about:blank`、`about:blank%23x` 等前綴混淆 | 拒絕；比對整份文件位址，不是「包含」或「開頭是」 | navigation_allowed 負向測試 |
| 任何框架導向 `data:`、`file:`、`javascript:`、`blob:null`、`blob:file:`、`blob:blob:`、含帳密的 blob 來源、自訂 scheme（含 `ws:`、`filesystem:`、`view-source:`）、其他 `about:` 頁、`about:`／`blob:` 的大小寫變體、頭尾空白、全形或百分比編碼的 scheme、含控制字元者 | 拒絕 | navigation_allowed 負向測試；`data:` 另由原生 smoke 觀察 |
| 設定損毀／過大／未來版本（取代第一節同名列的「回報錯誤」） | 詢問後才動作：結束則原檔不動；重設則原檔改名備份，位元組相同 | quarantine 整合測試；smoke 模式不顯示對話框、原檔不動 |
| 同名備份已存在／沒有設定檔可備份 | 拒絕，兩個檔案都不變動、不建立新檔 | quarantine 整合測試 |
| 新版寫入本版不認得的頂層欄位 | 原樣保留並寫回；`pads` 與 `toggle_shortcut` 內的未知欄位仍拒絕 | model 往返測試＋真實檔案保存測試 |
| 設定檔頂層出現 `__proto__` 之類的鍵 | 只保留在磁碟，不送進控制面板 | without_unknown_fields 測試 |
| 保存失敗時切換網站 | 照常切換並提示；磁碟設定不變，之後的成功保存一併寫入 | 原生 smoke（唯讀目錄） |

解析後仍是一般 http(s) 網址的寫法（大寫 scheme、空的 userinfo、同形字網域）視為合法；同形字網域在網址列以 punycode 顯示。

矩陣經獨立探測者三輪實跑後補列並重整；最後一輪對定版規則跑了 536 個手工輸入與約 76 萬個相異 fuzz 輸入（含與重構前邏輯的差分比對），零反例。探測程式未收進 repo。「備份並重設」對話框的按鈕行為未經自動化驗證（執行環境沒有輔助使用權限）；已實測對話框會以 modal 層級顯示在前景，第一個（預設）按鈕是結束。
