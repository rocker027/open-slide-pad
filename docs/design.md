# Open Slide Pad 視覺改版

2026-09-20。定位是隨手開啟的 macOS 側邊工具，首頁優先處理「回到常用網站」。

## 介面決策

- 暖灰底、墨色文字與陶土色；強調色只用於新增、選取與開啟狀態。外觀跟隨系統明暗模式。
- 首頁顯示已加入網站、網域與對應快捷鍵。空狀態提供新增引導；已有網站時預設收起服務建議。
- 移除大幅宣傳標語、漸層、裝飾疊卡與全大寫英文標籤。沿用系統字體，以細分隔線建立層級。
- 設定分為視窗與顯示、快捷鍵、管理網站。二元偏好使用有可及性名稱的開關。
- 最窄 360 pt 仍保留「在預設瀏覽器開啟」。工具列 48 pt、狀態列 26 pt、左側圖示列 64 pt；這組數字以 `src/desktop/browser.rs` 的常數為準，`ui/style.css` 的 `:root` 自訂屬性必須相同，由單元測試綁定（此前為 70／36 pt）。
- 網頁是原生子視圖，永遠蓋在控制面板的 HTML 上面。提示訊息、載入進度只能放在工具列那條帶子裡；設定與載入失敗畫面顯示時，先把原生網頁視圖藏起來。
- 最小字級 11 px。首頁不重複產品名、網站數與快捷鍵。
- 支援鍵盤焦點、減少動態效果偏好（控制面板的動畫與轉場，以及原生視窗的滑出：改為只淡入淡出）；網站名稱與網域透過文字節點呈現。
- 選單列圖示與介面左上角是同一個符號，由 `src/desktop/status_icon.rs` 直接畫成 template image，隨選單列明暗變色。

## 圖示

以「邊緣滑出的面板」作為符號：暖白圓角方形底、深色框、陶土色面板與短直把手。介面左上角使用對應的簡化幾何符號。

- 原稿：`resources/AppIcon.png`，imagegen 內建工具生成，1254 × 1254，保留透明背景。
- macOS 圖示：`resources/AppIcon.icns`，包含 16–1024 px 表示。
- 封裝：`swift scripts/make-icon.swift`。可指定 `來源.png 輸出.icns`；只做尺寸轉換與封裝，不重新生成設計。
- App 打包：`./scripts/bundle.sh`。

以下為本次生成使用的提示詞；影像生成具隨機性，原稿是交付來源。

> Create one finished macOS app icon for 'Open Slide Pad', a small sidebar web browser that slides in from the screen edge. This is a production app icon asset, not a presentation or mockup. Square 1024x1024 image, transparent background outside the icon. A meticulously composed rounded-square tile in warm porcelain ivory (#f2eee6), with a single bold abstract 'sliding panel' symbol centered within it: a dark charcoal vertical rounded window frame slightly left of center, and one solid muted burnt-terracotta (#b8583c) rectangular panel slid across the right half of that frame, leaving a clear tall narrow dark recess on the left. The red panel has gently rounded corners and one tiny ivory vertical grab-handle cutout near its left edge. It should read as an opening at the edge of a window, with an architectural, quiet, useful character. Large simple geometry, excellent silhouette readable at 32px, optically centered. Subtle crafted depth only from a fine edge and a very soft shallow shadow under the red panel. Calm independent Mac utility, warm restrained industrial design. The icon tile fills 88 percent of the square with generous transparent margin, macOS rounded-square proportions. Front-facing orthographic view. No perspective, no letters, no typography, no badges, no sparkles, no AI imagery, no green, no purple, no gradients, no glossy reflections, no stacked floating cards, no heavy 3D, no scene or surrounding objects. Exactly one icon.

## 預覽

`preview.png` 為使用獨立暫存設定的原生 WKWebView 視窗（測試快捷鍵 F17），擷取自 0.4.1，當時介面預設為繁體中文；`preview-light.png` 與 `preview-dark.png` 為同一份 UI 配合示範網站資料的 Chromium 畫面，僅展示本機控制介面，不代表遠端網站整合驗證。三張圖都早於工具列／狀態列縮小與首頁去重，尚未重拍。
