use super::{Event, chrome::Command};
use anyhow::Result;
use tao::event_loop::EventLoopProxy;
use tray_icon::{
    TrayIcon, TrayIconBuilder,
    menu::{
        Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
        accelerator::{Accelerator, Code, Modifiers},
    },
};

pub fn build(proxy: EventLoopProxy<Event>) -> Result<(TrayIcon, Menu)> {
    let shortcut = |key| Some(Accelerator::new(Some(Modifiers::SUPER), key));
    let toggle = MenuItem::with_id("toggle", "顯示／收合 Open Slide Pad", true, None);
    let quit = MenuItem::with_id("quit", "結束 Open Slide Pad", true, None);
    let tray_menu = Menu::with_items(&[&toggle, &PredefinedMenuItem::separator(), &quit])?;
    let tray = TrayIconBuilder::new()
        .with_title("◧")
        .with_tooltip("Open Slide Pad")
        .with_menu(Box::new(tray_menu))
        .build()?;

    let app_menu = Menu::new();
    let app_submenu = Submenu::new("Open Slide Pad", true);
    app_submenu.append_items(&[
        &PredefinedMenuItem::about(Some("關於 Open Slide Pad"), None),
        &MenuItem::with_id("settings", "設定⋯", true, shortcut(Code::Comma)),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::quit(Some("結束 Open Slide Pad")),
    ])?;

    let file = Submenu::new("檔案", true);
    file.append_items(&[
        &MenuItem::with_id("new_pad", "新增網站⋯", true, shortcut(Code::KeyT)),
        &MenuItem::with_id("hide", "收合側欄", true, shortcut(Code::KeyW)),
    ])?;

    let edit = Submenu::new("編輯", true);
    edit.append_items(&[
        &PredefinedMenuItem::undo(None),
        &PredefinedMenuItem::redo(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::cut(None),
        &PredefinedMenuItem::copy(None),
        &PredefinedMenuItem::paste(None),
        &PredefinedMenuItem::select_all(None),
    ])?;

    let browse = Submenu::new("瀏覽", true);
    browse.append_items(&[
        &MenuItem::with_id("focus_address", "輸入網址", true, shortcut(Code::KeyL)),
        &PredefinedMenuItem::separator(),
        &MenuItem::with_id("back", "上一頁", true, shortcut(Code::BracketLeft)),
        &MenuItem::with_id("forward", "下一頁", true, shortcut(Code::BracketRight)),
        &MenuItem::with_id("reload", "重新載入", true, shortcut(Code::KeyR)),
    ])?;

    let sites = Submenu::new("網站", true);
    for (index, key) in [
        Code::Digit1,
        Code::Digit2,
        Code::Digit3,
        Code::Digit4,
        Code::Digit5,
        Code::Digit6,
        Code::Digit7,
        Code::Digit8,
        Code::Digit9,
    ]
    .into_iter()
    .enumerate()
    {
        sites.append(&MenuItem::with_id(
            format!("select_pad_{index}"),
            format!("第 {} 個網站", index + 1),
            true,
            shortcut(key),
        ))?;
    }

    // 主選單讓遠端 child WebView 聚焦時仍能收到應用程式快捷鍵。
    app_menu.append_items(&[&app_submenu, &file, &edit, &browse, &sites])?;
    app_menu.init_for_nsapp();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let message = match event.id.0.as_str() {
            "toggle" => Some(Event::Toggle),
            "quit" => Some(Event::Quit),
            "new_pad" => Some(Event::Command(Command::NewPad)),
            "hide" => Some(Event::Command(Command::Hide)),
            "settings" => Some(Event::Command(Command::ShowSettings)),
            "focus_address" => Some(Event::Command(Command::FocusAddress)),
            "back" => Some(Event::Command(Command::Back)),
            "forward" => Some(Event::Command(Command::Forward)),
            "reload" => Some(Event::Command(Command::Reload)),
            id => id
                .strip_prefix("select_pad_")
                .and_then(|index| index.parse::<usize>().ok())
                .filter(|index| *index < 9)
                .map(|index| Event::Command(Command::SelectIndex { index })),
        };
        if let Some(message) = message {
            let _ = proxy.send_event(message);
        }
    }));
    Ok((tray, app_menu))
}
