# Slidepad 研究與 Open Slide Pad 範圍

查閱日期：2026-09-20。只根據公開網站研究，未反編譯或複製原產品程式與資產。

## 觀察

[Slidepad 官網](https://slidepad.app/) 的主軸為把瀏覽器放在工作邊緣；游標碰到指定邊緣或角落時出現，移開便收合。官網另外列出 iCloud 同步、Safari Web Extensions、MCP server 及 profile switching。同步介紹特別區分設定與登入資料：登入與瀏覽資料留在裝置端。

## 第一版決策

| 能力 | Open Slide Pad |
| --- | --- |
| Hot edge／浮動視窗 | 左右邊緣、停留門檻、輕量滑出動畫 |
| 常用網路工具 | 個別 WebView、懶載入、多頁常駐切換 |
| 偏好設定 | 本機原子 JSON、單一程序鎖 |
| 快速呼叫 | 全域 ⌘⇧Space、選單列 |
| 多螢幕 | AppKit 的邏輯座標與 visibleFrame；游標所在螢幕 |
| 登入與 cookies | 原生 WebKit 預設 store，共用單一 profile |
| iCloud／Safari 擴充／MCP | 未實作；不以 WKWebView 等同具備 Safari 擴充支援 |

## 技術選型

採 [Tao](https://docs.rs/tao/0.37.0/tao/) 管理原生 NSWindow 與 event loop，[Wry](https://docs.rs/wry/0.57.0/wry/) 建立 WKWebView 子視圖。Wry 的 `build_as_child` 可直接把 WebView 放在 macOS 視窗內容區指定矩形，網站不受 iframe 嵌入限制。這也避免打包 Chromium，但網站相容性依系統 WebKit。

[global-hotkey](https://docs.rs/global-hotkey/0.7.0/global_hotkey/) 提供原生全域快捷鍵，[tray-icon](https://docs.rs/tray-icon/0.21.3/tray_icon/) 提供選單列。版本由 Cargo.lock 固定。

控制面板與網站是不同 WebView；只有本機面板可送出嚴格列舉的命令。桌面操作全在 Tao 主執行緒，設定以成功寫入作為提交點。WebView 先於 Window 釋放。

## 已知取捨

- 輪詢滑鼠目前約 31 Hz；無動畫時仍會定時喚醒，後續可改成原生 tracking area 降低待機喚醒。
- 只監看游標位置，不需要讀取使用者鍵盤輸入或螢幕內容。
- 釘選、設定 overlay 與移入／移出狀態共同控制收合；快捷鍵開啟後，需先移入再移出才自動收合，避免剛開啟就消失。
- 分享給其他 Mac 前需正式簽章及公證；本次只交付本機可執行版本。
