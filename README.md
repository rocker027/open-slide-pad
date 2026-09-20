# Open Slide Pad

以 Rust 製作的 macOS 側邊瀏覽器，參考 [Slidepad](https://slidepad.app/) 的操作概念，採獨立介面與實作。網站使用 macOS 原生 **WKWebView**；視窗、狀態、快捷鍵與保存由 Rust 管理。HTML/CSS/JavaScript 僅用於本機控制面板，不需要 Node.js。

<img src="resources/AppIcon.png" width="80" alt="Open Slide Pad 滑出面板圖示">

<img src="docs/preview-light.png" width="300" alt="淺色首頁，示範網站資料"> <img src="docs/preview-dark.png" width="300" alt="深色首頁，示範網站資料">

暖灰與陶土色介面，跟隨 macOS 明暗外觀。首頁直接開啟已加入的網站；設計決策、圖示原稿與重建方式見 [視覺設計](docs/design.md)，[原生視窗實際畫面](docs/preview.png)。

## 執行

需求：macOS 13+、Rust 1.88+、Xcode Command Line Tools。實測機器為 Apple Silicon／macOS 15.6.1；Intel 尚未實測。

```sh
cargo run --locked
# 建置可直接開啟的 app
./scripts/bundle.sh
open "dist/Open Slide Pad.app"
```

可將 `dist/Open Slide Pad.app` 拖進「應用程式」。產物為本機 ad-hoc 簽章，未做 Apple Developer ID 公證；未發行至 App Store。

## 操作

- 游標停在螢幕右緣約 0.18 秒，側欄淡入滑出。設定可改為左側。
- 游標進入面板後移開約 0.65 秒，自動收合；圖釘可固定顯示。
- 預設 **⌘⇧Space** 或選單列的 **◧** 可顯示／收合。
- 在 **設定 → 顯示／收合快捷鍵** 選擇修飾鍵與主鍵，按「套用快捷鍵」即可立即更換，下次啟動也會保留。「恢復預設」會改回 **⌘⇧Space**。
- 快捷鍵支援 A–Z、0–9、空白鍵、F1–F20，至少包含 ⌘、⌥、⌃ 之一。App／文字編輯的既有快捷鍵會保留；系統回報組合已占用時，保留原快捷鍵並顯示錯誤。若啟動時註冊失敗，可用選單列開啟設定再換一組；部分系統保留組合不一定會回報衝突，請避免使用 macOS 既有快捷鍵。
- 首頁列出已加入的網站與網域，按任一列即可開啟；「加入常用服務」可展開快速新增建議。
- **＋** 新增網站或搜尋；左側切換網站，各網站的 WebView 保持在記憶體中，初次選取才載入，最多 20 個。
- 網址列、上一頁、下一頁、重新整理及「在預設瀏覽器開啟」。
- 設定可調整左右位置、360–960 pt 寬度、hot edge、釘選及移除網站。
- 用滑鼠拖曳視窗邊緣，或拖曳**內側下角的斜線把手**調整寬高（右側側欄在左下角，左側側欄在右下角）。拖曳期間暫停自動收合；放開後保存尺寸與頂端位置，網頁會同步縮放。
- 高度最小 320 pt，尺寸會限制在目前螢幕可用範圍內；設定的「恢復全高」可回到隨螢幕高度展開。保存失敗會回復拖曳前的尺寸。
- 原生快捷鍵在網站聚焦時也可用：**⌘L** 選取網址、**⌘T** 新增、**⌘R** 重新整理、**⌘[／⌘]** 返回／前進、**⌘1–9** 切換前九個網站、**⌘,** 開啟設定、**⌘W** 收合。
- 設定中的「編輯」可重新命名，**↑／↓** 可調整網站順序，重啟後保留。
- 移除網站後，底部的「復原移除」可恢復最近一次移除的捷徑。此紀錄僅保留於本次執行；不恢復原網站尚未提交的輸入內容。
- **Esc** 在本機控制面板聚焦時關閉面板／收合；網站內 Esc 由網站處理。
- 關閉視窗只會收合；選單列「結束 Open Slide Pad」才會退出。

## 資料與信任邊界

更名後沿用既有資料識別碼與 WebKit 設定，讓網站、快捷鍵、視窗偏好及網站資料持續使用。`directories::ProjectDirs` 將設定放在 `~/Library/Application Support/app.sliderust.SlideRust/`。設定是 version 1 的 `settings.json`，以同目錄暫存檔＋原子 rename 保存。檔案鎖阻止同一資料目錄的第二個程序覆寫設定。損毀或未支援版本會停止啟動，保留原檔供修復；最多讀取 256 KiB。原子替換前寫入失敗不變更畫面狀態；替換成功但目錄同步失敗時，畫面採用已寫入的新設定，並顯示耐久性警告。

網站登入與 cookies 交由此 app 的 WebKit 預設資料存放區管理，與 Safari 的登入分開。所有網站共用同一個 profile；移除捷徑不清除網站資料。網址列明確提交的網址會保存；頁面自動導向與 OAuth URL 不寫回設定。

遠端 WebView 沒有控制 IPC。控制面板不允許遠端導覽、iframe 或網路請求，所有網站標題以文字呈現。僅支援 HTTP/HTTPS；不執行 `file:`、`javascript:` 或外部 app scheme。

## 驗證

```sh
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
# 必須使用獨立目錄；smoke 使用無痕 WebKit，最長約 25 秒
cargo run --locked -- --smoke-test --data-dir "$(mktemp -d)"
```

smoke 驗證原生視窗、本機控制 IPC、example.com 載入、同文件 pushState，以及原生選單 ⌘L／⌘T／⌘W 的命令路由、原生 responder、DOM 焦點與收合。測試直接呼叫 app 內的 AppKit 選單契約，不送出系統鍵盤輸入。另驗證快捷鍵設定表單 IPC、原生 F18／F19 組合來回註冊、即時保存、過期快捷鍵 ID 過濾與畫面提示同步。亦驗證原生視窗 resize 事件、網頁尺寸、尺寸保存、唯讀目錄下回復與恢復全高。它不是完整 UI E2E，不送出實際滑鼠拖曳，不會登入帳號或提交外部表單。

快捷鍵負向測試包含註冊衝突、保存失敗回復、提交後同步警告與解除失敗警告。`node scripts/test-shortcut-ui.cjs` 可重跑快捷鍵重設、尺寸控制命令與首頁網站清單的回歸測試。

## 目前邊界

尚無 iCloud、Safari 擴充、MCP、多帳號 profile、下載管理、通知、登入時啟動及自動更新。`target=_blank`／新視窗要求會在原網站分頁開啟；需要 popup opener 的 OAuth 流程可能不相容，請使用「在預設瀏覽器開啟」。攝影機／麥克風流程未支援。所有網站的登入相容性、全螢幕 Spaces、實體多螢幕與 Intel 機器尚未全面驗證。

研究、取捨與來源見 [docs/research.md](docs/research.md)，負向驗收見 [docs/security-tests.md](docs/security-tests.md)。
