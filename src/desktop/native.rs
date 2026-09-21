use objc2::MainThreadMarker;
use objc2_app_kit::{NSEvent, NSScreen, NSWindow};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use sliderust::panel::Frame;
use tao::{platform::macos::WindowExtMacOS, window::Window};

pub fn mouse() -> (f64, f64) {
    let point = NSEvent::mouseLocation();
    (point.x, point.y)
}
pub fn left_mouse_down() -> bool {
    NSEvent::pressedMouseButtons() & 1 != 0
}
pub fn window_frame(host: &Window) -> Frame {
    frame(window(host).frame())
}
pub fn is_live_resize(host: &Window) -> bool {
    window(host).inLiveResize()
}
fn frame(rect: NSRect) -> Frame {
    Frame {
        left: rect.origin.x,
        bottom: rect.origin.y,
        width: rect.size.width,
        height: rect.size.height,
    }
}
pub fn screens() -> Vec<(Frame, Frame)> {
    let mtm = MainThreadMarker::new().expect("AppKit 必須在主執行緒");
    NSScreen::screens(mtm)
        .iter()
        .map(|screen| (frame(screen.frame()), frame(screen.visibleFrame())))
        .collect()
}
fn window(window: &Window) -> &NSWindow {
    // Tao 擁有此 NSWindow；借用期間 Window 尚在且所有操作都在主執行緒。
    unsafe { &*window.ns_window().cast::<NSWindow>() }
}
pub fn set_frame(host: &Window, frame: Frame) {
    window(host).setFrame_display(
        NSRect::new(
            NSPoint::new(frame.left, frame.bottom),
            NSSize::new(frame.width, frame.height),
        ),
        true,
    );
}
pub fn animate_frame(host: &Window, target: Frame, progress: f64, right: bool) {
    let eased = 1.0 - (1.0 - progress).powi(3);
    let offset = (1.0 - eased) * if right { 44.0 } else { -44.0 };
    window(host).setFrameOrigin(NSPoint::new(target.left + offset, target.bottom));
    window(host).setAlphaValue(eased);
}
pub fn opacity(host: &Window, opacity: f64) {
    window(host).setAlphaValue(opacity);
}

/// 啟動期的對話框出現在事件迴圈開始之前，這時 tao 還沒套用啟用政策，App 也尚未啟用。
/// 對話框會開在一般視窗層級、可能被其他 App 的視窗蓋住，看起來像啟動卡住；
/// 先設定政策並帶到前景，對話框才會以 modal 層級顯示在最上層。
fn bring_to_front(application: &objc2_app_kit::NSApplication) {
    application.setActivationPolicy(objc2_app_kit::NSApplicationActivationPolicy::Accessory);
    // macOS 14 的 activate() 在最低支援的 macOS 13 上不存在。
    #[allow(deprecated)]
    application.activateIgnoringOtherApps(true);
}

/// 設定無法載入時，詢問是否備份原檔並以預設值啟動；回傳 true 代表使用者選擇重設。
/// 第一個按鈕（Return 預設）是結束，避免誤按就丟掉網站清單。
pub fn confirm_reset(message: &str) -> bool {
    use objc2_app_kit::{NSAlert, NSAlertSecondButtonReturn, NSApplication};
    use objc2_foundation::NSString;
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    bring_to_front(&NSApplication::sharedApplication(mtm));
    let alert = NSAlert::new(mtm);
    // 偏好尚未載入，與啟動錯誤一樣使用產品預設語言。
    let language = sliderust::i18n::Language::default();
    alert.setMessageText(&NSString::from_str(
        &language.text("Open Slide Pad 無法載入設定"),
    ));
    alert.setInformativeText(&NSString::from_str(&format!(
        "{}\n\n{}",
        language.text(message),
        language.text(
            "可以先結束再自行修復。也可以把目前的設定檔改名備份，然後以預設設定啟動。網站登入資料不受影響。"
        )
    )));
    alert.addButtonWithTitle(&NSString::from_str(&language.text("結束")));
    alert.addButtonWithTitle(&NSString::from_str(&language.text("備份並重設")));
    alert.runModal() == NSAlertSecondButtonReturn
}

pub fn alert(message: &str) {
    use objc2_app_kit::{NSAlert, NSApplication};
    use objc2_foundation::NSString;
    if let Some(mtm) = MainThreadMarker::new() {
        bring_to_front(&NSApplication::sharedApplication(mtm));
        let alert = NSAlert::new(mtm);
        // 尚未成功載入偏好時，啟動錯誤使用產品預設語言。
        let language = sliderust::i18n::Language::default();
        alert.setMessageText(&NSString::from_str(
            &language.text("Open Slide Pad 無法啟動"),
        ));
        alert.setInformativeText(&NSString::from_str(&language.text(message)));
        alert.addButtonWithTitle(&NSString::from_str(&language.text("好")));
        alert.runModal();
    }
}
