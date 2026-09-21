use super::{Event, chrome::Command, status_icon};
use anyhow::Result;
use sliderust::i18n::Language;
use tao::event_loop::EventLoopProxy;
use tray_icon::{
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{
        Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
        accelerator::{Accelerator, Code, Modifiers},
    },
};

#[derive(Default)]
pub struct Labels {
    items: Vec<(MenuItem, String)>,
    submenus: Vec<(Submenu, String)>,
    predefined: Vec<(PredefinedMenuItem, String)>,
}

impl Labels {
    fn item(
        &mut self,
        id: impl AsRef<str>,
        title: impl AsRef<str>,
        accelerator: Option<Accelerator>,
    ) -> MenuItem {
        let item = MenuItem::with_id(id.as_ref(), title.as_ref(), true, accelerator);
        self.items.push((item.clone(), title.as_ref().to_owned()));
        item
    }

    fn submenu(&mut self, title: &str) -> Submenu {
        let menu = Submenu::new(title, true);
        self.submenus.push((menu.clone(), title.to_owned()));
        menu
    }

    fn predefined(&mut self, item: PredefinedMenuItem, title: &str) -> PredefinedMenuItem {
        self.predefined.push((item.clone(), title.to_owned()));
        item
    }

    pub fn set_language(&self, language: Language) {
        // 只更新標題，保留原生選單物件、快捷鍵與事件處理器。
        for (item, source) in &self.items {
            item.set_text(language.text(source));
        }
        for (menu, source) in &self.submenus {
            menu.set_text(language.text(source));
        }
        for (item, source) in &self.predefined {
            item.set_text(language.text(source));
        }
    }
}

pub fn build(proxy: EventLoopProxy<Event>, language: Language) -> Result<(TrayIcon, Menu, Labels)> {
    let mut labels = Labels::default();
    let shortcut = |key| Some(Accelerator::new(Some(Modifiers::SUPER), key));
    let toggle = labels.item("toggle", "顯示／收合 Open Slide Pad", None);
    let quit = labels.item("quit", "結束 Open Slide Pad", None);
    let tray_menu = Menu::with_items(&[&toggle, &PredefinedMenuItem::separator(), &quit])?;
    // 左鍵直接顯示／收合，右鍵才開選單；最常做的動作不必先經過選單。
    let tray = TrayIconBuilder::new()
        .with_icon(Icon::from_rgba(
            status_icon::rgba(),
            status_icon::SIZE,
            status_icon::SIZE,
        )?)
        .with_icon_as_template(true)
        .with_menu_on_left_click(false)
        .with_tooltip("Open Slide Pad")
        .with_menu(Box::new(tray_menu))
        .build()?;
    let tray_proxy = proxy.clone();
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        // 按下與放開各回報一次，只在放開時切換。
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            let _ = tray_proxy.send_event(Event::Toggle);
        }
    }));

    let app_menu = Menu::new();
    let app_submenu = Submenu::new("Open Slide Pad", true);
    app_submenu.append_items(&[
        &labels.predefined(
            PredefinedMenuItem::about(Some("關於 Open Slide Pad"), None),
            "關於 Open Slide Pad",
        ),
        &labels.item("settings", "設定⋯", shortcut(Code::Comma)),
        &PredefinedMenuItem::separator(),
        &labels.predefined(
            PredefinedMenuItem::quit(Some("結束 Open Slide Pad")),
            "結束 Open Slide Pad",
        ),
    ])?;

    let file = labels.submenu("檔案");
    file.append_items(&[
        &labels.item("new_pad", "新增網站⋯", shortcut(Code::KeyT)),
        &labels.item("hide", "收合側欄", shortcut(Code::KeyW)),
    ])?;

    let edit = labels.submenu("編輯");
    edit.append_items(&[
        &labels.predefined(PredefinedMenuItem::undo(Some("復原")), "復原"),
        &labels.predefined(PredefinedMenuItem::redo(Some("重做")), "重做"),
        &PredefinedMenuItem::separator(),
        &labels.predefined(PredefinedMenuItem::cut(Some("剪下")), "剪下"),
        &labels.predefined(PredefinedMenuItem::copy(Some("複製")), "複製"),
        &labels.predefined(PredefinedMenuItem::paste(Some("貼上")), "貼上"),
        &labels.predefined(PredefinedMenuItem::select_all(Some("全選")), "全選"),
    ])?;

    let browse = labels.submenu("瀏覽");
    browse.append_items(&[
        &labels.item("focus_address", "輸入網址", shortcut(Code::KeyL)),
        &PredefinedMenuItem::separator(),
        &labels.item("back", "上一頁", shortcut(Code::BracketLeft)),
        &labels.item("forward", "下一頁", shortcut(Code::BracketRight)),
        &labels.item("reload", "重新載入", shortcut(Code::KeyR)),
        &labels.item("stop", "停止載入", shortcut(Code::Period)),
    ])?;

    let sites = labels.submenu("網站");
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
        sites.append(&labels.item(
            format!("select_pad_{index}"),
            format!("第 {} 個網站", index + 1),
            shortcut(key),
        ))?;
    }

    // 主選單讓遠端 child WebView 聚焦時仍能收到應用程式快捷鍵。
    app_menu.append_items(&[&app_submenu, &file, &edit, &browse, &sites])?;
    labels.set_language(language);
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
            "stop" => Some(Event::Command(Command::Stop)),
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
    Ok((tray, app_menu, labels))
}
