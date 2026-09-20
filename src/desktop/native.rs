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

pub fn alert(message: &str) {
    use objc2_app_kit::{NSAlert, NSApplication};
    use objc2_foundation::NSString;
    if let Some(mtm) = MainThreadMarker::new() {
        let _application = NSApplication::sharedApplication(mtm);
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
