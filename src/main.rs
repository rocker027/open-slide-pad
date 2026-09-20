#[cfg(target_os = "macos")]
mod desktop;

fn main() {
    #[cfg(target_os = "macos")]
    if let Err(error) = desktop::run() {
        eprintln!("Open Slide Pad 啟動失敗：{error:#}");
        if !std::env::args().any(|arg| arg == "--smoke-test") {
            desktop::report_error(&format!("{error:#}"));
        }
        std::process::exit(1);
    }
    #[cfg(not(target_os = "macos"))]
    eprintln!("Open Slide Pad 的桌面介面僅支援 macOS。");
}
