# Open Slide Pad 視覺改版

初版 2026-09-20；互動整理 2026-10-22；易用性調整 2026-11-05。定位是隨手開啟的 macOS 側邊工具，首頁優先處理「回到常用網站」。

## 介面決策

- 暖灰底、墨色文字與陶土色；強調色只用於新增、選取與開啟狀態。外觀跟隨系統明暗模式。
- 首頁顯示已加入網站、網域與對應快捷鍵，提供名稱／網址搜尋與直接進入網站管理的入口；無符合結果時顯示提示。空狀態提供新增引導；已有網站時預設收起服務建議。已加入的推薦服務改為開啟既有網站；達 20 個上限時停用新增，並提供管理引導。
- 移除大幅宣傳標語、漸層、裝飾疊卡與全大寫英文標籤。使用 macOS 內建 Avenir Next，中文以 PingFang TC 接續，再以 Helvetica Neue／sans-serif 備援；以細分隔線建立層級。
- 設定分為「一般、網站、快捷鍵」三區，標題與分區標籤固定於上方。一般先顯示語言與常用顯示偏好，尺寸與休眠置於預設收合的「更多設定」，展開與焦點在設定回覆後保留。一般分區說明變更會自動儲存。網站集中新增、匯入、排序與移除，快捷鍵集中錄製與說明。分區支援左右方向鍵與 Home／End；二元偏好使用有可及性名稱的開關。設定更新保留目前分區、焦點、捲動位置與未套用的快捷鍵草稿。
- 最窄 360 pt 仍保留「在預設瀏覽器開啟」。工具列 48 pt、狀態列 26 pt、左側圖示列 64 pt；這組數字以 `src/desktop/browser.rs` 的常數為準，`ui/style.css` 的 `:root` 自訂屬性必須相同，由單元測試綁定（此前為 70／36 pt）。
- 網頁是原生子視圖，永遠蓋在控制面板的 HTML 上面。提示訊息、載入進度只能放在工具列那條帶子裡；設定與載入失敗畫面顯示時，先把原生網頁視圖藏起來。
- 內容主要使用 12–14 px，側欄短標籤與輔助資訊保留 11 px。窄側欄的管理列將名稱與操作分成兩列，操作至少 32 × 34 px；短視窗縮減面板標題與間距，維持內容可捲動。
- 新增、設定與匯入使用 modal dialog；開啟時移入焦點並停用背景操作，Tab／Shift+Tab 在面板內循環，關閉／Esc 返回原入口。匯入返回原本的首頁或網站設定，篩選不取消已選項目，並提示零結果與被隱藏的選取數；結果區獨立捲動，保留篩選、提交與返回操作。
- 支援鍵盤焦點、減少動態效果偏好（控制面板的動畫與轉場，以及原生視窗的滑出：改為只淡入淡出）；網站名稱與網域透過文字節點呈現。
- 選單列圖示與介面左上角是同一個符號，由 `src/desktop/status_icon.rs` 直接畫成 template image，隨選單列明暗變色。

## 降低理解負擔

- 側欄顯示「首頁／新增／設定／收合」，讓主要入口不依賴圖示或滑鼠停留提示。
- 空首頁以「加入你的第一個網站」和主要新增按鈕開始；有網站時提示「點一下網站就能開啟」。空搜尋可直接清除；20 個上限提供可見的管理指引。
- 新增操作明示「加入並開啟」，並交代輸入關鍵字會開啟搜尋結果；有取消或返回管理網站入口。新增與改名回饋留在表單旁，直到再次輸入或離開。
- 首頁的開啟方向、邊緣開啟與自動收合說明由目前設定產生，提供帶文字的固定顯示操作。管理列以「重新命名／移除」說明動作。
- 失敗頁提供重試、修改網址與回首頁，原頁面可恢復時仍保留返回原頁。
- 以上是降低理解負擔的設計與實作；程式回歸及本機控制頁預覽不能證明特定使用族群的成功率。原生 VoiceOver 與實際新手任務測試尚未執行。

## 圖示

以「邊緣滑出的面板」作為符號：暖白圓角方形底、深色框、陶土色面板與短直把手。介面左上角使用對應的簡化幾何符號。

- 原稿：`resources/AppIcon.png`，imagegen 內建工具生成，1254 × 1254，保留透明背景。
- macOS 圖示：`resources/AppIcon.icns`，包含 16–1024 px 表示。
- 封裝：`swift scripts/make-icon.swift`。可指定 `來源.png 輸出.icns`；只做尺寸轉換與封裝，不重新生成設計。
- App 打包：`./scripts/bundle.sh`。

以下為本次生成使用的提示詞；影像生成具隨機性，原稿是交付來源。

> Create one finished macOS app icon for 'Open Slide Pad', a small sidebar web browser that slides in from the screen edge. This is a production app icon asset, not a presentation or mockup. Square 1024x1024 image, transparent background outside the icon. A meticulously composed rounded-square tile in warm porcelain ivory (#f2eee6), with a single bold abstract 'sliding panel' symbol centered within it: a dark charcoal vertical rounded window frame slightly left of center, and one solid muted burnt-terracotta (#b8583c) rectangular panel slid across the right half of that frame, leaving a clear tall narrow dark recess on the left. The red panel has gently rounded corners and one tiny ivory vertical grab-handle cutout near its left edge. It should read as an opening at the edge of a window, with an architectural, quiet, useful character. Large simple geometry, excellent silhouette readable at 32px, optically centered. Subtle crafted depth only from a fine edge and a very soft shallow shadow under the red panel. Calm independent Mac utility, warm restrained industrial design. The icon tile fills 88 percent of the square with generous transparent margin, macOS rounded-square proportions. Front-facing orthographic view. No perspective, no letters, no typography, no badges, no sparkles, no AI imagery, no green, no purple, no gradients, no glossy reflections, no stacked floating cards, no heavy 3D, no scene or surrounding objects. Exactly one icon.

## 預覽

`preview.png` 為使用獨立暫存設定的原生 WKWebView 視窗（測試快捷鍵 F17），擷取自 0.4.1，當時介面預設為繁體中文；`preview-light.png` 與 `preview-dark.png` 為同一份 UI 配合示範網站資料的 Chromium 畫面，僅展示本機控制介面，不代表遠端網站整合驗證。三張圖都早於工具列／狀態列縮小、首頁搜尋與設定分區，尚未重拍；不作為目前介面或本輪原生驗證的證據。
