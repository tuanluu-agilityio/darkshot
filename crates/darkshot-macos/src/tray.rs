//! Biểu tượng trên thanh menu (`NSStatusItem`) và menu của nó, dựng từ
//! `darkshot_core::tray_logic::menu_layout`. Lệnh chưa làm trên Mac bị loại khỏi menu bằng
//! `implemented` (Task sau chỉ việc bật).

use crate::lang;
use darkshot_core::i18n::Msg;
use darkshot_core::tray_logic::{menu_layout, MenuCommand, MenuEntry};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSImage, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::{NSObject, NSObjectProtocol, NSString};

/// Lệnh đã làm trên bản Mac. Lệnh trả `false` không xuất hiện trong menu.
pub fn implemented(cmd: MenuCommand) -> bool {
    matches!(cmd, MenuCommand::Capture | MenuCommand::Quit)
}

/// Nhãn cố định của một lệnh menu; `None` cho nhãn có tham số (bản mới, số bước, số ảnh ghim),
/// bên gọi dựng bằng hàm của `i18n`.
pub fn menu_msg(cmd: MenuCommand) -> Option<Msg> {
    match cmd {
        MenuCommand::Capture => Some(Msg::TrayCapture),
        MenuCommand::RepeatLast => Some(Msg::MenuRepeatLast),
        MenuCommand::History => Some(Msg::TrayHistory),
        MenuCommand::StepsStart => Some(Msg::TrayStepsStart),
        MenuCommand::Options => Some(Msg::TrayOptions),
        MenuCommand::About => Some(Msg::TrayAbout),
        MenuCommand::Help => Some(Msg::TrayHelp),
        MenuCommand::Quit => Some(Msg::MacMenuQuit),
        MenuCommand::OpenUpdate | MenuCommand::StepsReview | MenuCommand::ClosePins => None,
    }
}

/// Các dòng menu sẽ hiện: bỏ lệnh chưa làm; vạch ngăn không đứng đầu, cuối hay liền nhau.
pub fn visible_menu(layout: Vec<MenuEntry>) -> Vec<MenuEntry> {
    let mut out: Vec<MenuEntry> = Vec::with_capacity(layout.len());
    for e in layout {
        match e {
            MenuEntry::Item(c) | MenuEntry::Grayed(c) if !implemented(c) => {}
            MenuEntry::Separator if matches!(out.last(), None | Some(MenuEntry::Separator)) => {}
            e => out.push(e),
        }
    }
    if out.last() == Some(&MenuEntry::Separator) {
        out.pop();
    }
    out
}

/// Mọi lệnh menu theo thứ tự tag (tag = chỉ số + 1; tag 0 là "không có lệnh").
const COMMANDS: [MenuCommand; 11] = [
    MenuCommand::OpenUpdate,
    MenuCommand::Capture,
    MenuCommand::RepeatLast,
    MenuCommand::History,
    MenuCommand::StepsStart,
    MenuCommand::StepsReview,
    MenuCommand::ClosePins,
    MenuCommand::Options,
    MenuCommand::About,
    MenuCommand::Help,
    MenuCommand::Quit,
];

/// Tag của mục menu cho một lệnh.
pub fn tag_of(cmd: MenuCommand) -> isize {
    COMMANDS
        .iter()
        .position(|c| *c == cmd)
        .map_or(0, |i| i as isize + 1)
}

/// Lệnh của một tag; tag lạ thì `None`.
pub fn command_from_tag(tag: isize) -> Option<MenuCommand> {
    usize::try_from(tag)
        .ok()
        .and_then(|t| t.checked_sub(1))
        .and_then(|i| COMMANDS.get(i).copied())
}

/// Tên biểu tượng SF Symbols trên thanh menu.
const STATUS_SYMBOL: &str = "camera.viewfinder";

define_class!(
    // SAFETY: NSObject không có yêu cầu riêng khi kế thừa; lớp không cài `Drop`. Chỉ dùng trên
    // luồng chính (đích của mục menu).
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "DarkshotTrayTarget"]
    #[ivars = fn(MenuCommand)]
    struct MenuTarget;

    unsafe impl NSObjectProtocol for MenuTarget {}

    impl MenuTarget {
        #[unsafe(method(menuItemSelected:))]
        fn menu_item_selected(&self, sender: Option<&NSMenuItem>) {
            if let Some(cmd) = sender.and_then(|m| command_from_tag(m.tag())) {
                (self.ivars())(cmd);
            }
        }
    }
);

impl MenuTarget {
    fn new(mtm: MainThreadMarker, on_command: fn(MenuCommand)) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(on_command);
        // SAFETY: `init` của NSObject trên đối tượng vừa cấp phát.
        unsafe { msg_send![super(this), init] }
    }
}

/// Biểu tượng trên thanh menu và menu của nó. Chỉ sống trên luồng chính.
pub struct Tray {
    item: Retained<NSStatusItem>,
    menu: Retained<NSMenu>,
    target: Retained<MenuTarget>,
}

impl Tray {
    /// Thêm biểu tượng vào thanh menu; `on_command` nhận lệnh người dùng chọn (trên luồng chính).
    pub fn new(mtm: MainThreadMarker, on_command: fn(MenuCommand)) -> Tray {
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        if let Some(button) = item.button(mtm) {
            let name = NSString::from_str("darkshot");
            match NSImage::imageWithSystemSymbolName_accessibilityDescription(
                &NSString::from_str(STATUS_SYMBOL),
                Some(&name),
            ) {
                Some(img) => {
                    // Ảnh mẫu: hệ thống tô theo màu thanh menu (sáng/tối).
                    img.setTemplate(true);
                    button.setImage(Some(&img));
                }
                None => button.setTitle(&name),
            }
            button.setToolTip(Some(&name));
        }
        let menu = NSMenu::new(mtm);
        // Mục mờ phải giữ trạng thái `setEnabled(false)`.
        menu.setAutoenablesItems(false);
        item.setMenu(Some(&menu));
        let tray = Tray {
            item,
            menu,
            target: MenuTarget::new(mtm, on_command),
        };
        tray.rebuild(mtm);
        tray
    }

    /// Dựng lại menu theo `menu_layout` và ngôn ngữ hiện hành.
    pub fn rebuild(&self, mtm: MainThreadMarker) {
        self.menu.removeAllItems();
        for entry in visible_menu(menu_layout(false, 0, false, None)) {
            let (cmd, enabled) = match entry {
                MenuEntry::Separator => {
                    self.menu.addItem(&NSMenuItem::separatorItem(mtm));
                    continue;
                }
                MenuEntry::Item(c) => (c, true),
                MenuEntry::Grayed(c) => (c, false),
            };
            // Nhãn có tham số chưa có trên Mac (lệnh đó chưa làm nên đã bị loại).
            let Some(msg) = menu_msg(cmd) else { continue };
            // SAFETY: `alloc` mới trên luồng chính; bộ chọn `menuItemSelected:` có trên `MenuTarget`.
            let mi = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &NSString::from_str(lang::tr(msg)),
                    Some(sel!(menuItemSelected:)),
                    &NSString::from_str(""),
                )
            };
            mi.setTag(tag_of(cmd));
            mi.setEnabled(enabled);
            let target: &AnyObject = &self.target;
            // SAFETY: đích là `MenuTarget` sống cùng `Tray` (menu chỉ giữ tham chiếu yếu tới đích).
            unsafe { mi.setTarget(Some(target)) };
            self.menu.addItem(&mi);
        }
    }

    /// Bỏ biểu tượng khỏi thanh menu.
    pub fn remove(&self) {
        NSStatusBar::systemStatusBar().removeStatusItem(&self.item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkshot_core::i18n::{t, Lang};
    use darkshot_core::tray_logic::menu_layout;
    use MenuCommand::*;
    use MenuEntry::*;

    const ALL: [MenuCommand; 11] = [
        OpenUpdate,
        Capture,
        RepeatLast,
        History,
        StepsStart,
        StepsReview,
        ClosePins,
        Options,
        About,
        Help,
        Quit,
    ];

    #[test]
    fn only_capture_and_quit_are_implemented() {
        for c in ALL {
            assert_eq!(implemented(c), matches!(c, Capture | Quit), "{c:?}");
        }
    }

    #[test]
    fn menu_shows_only_implemented_commands() {
        let want = vec![Item(Capture), Item(Quit)];
        assert_eq!(visible_menu(menu_layout(false, 0, false, None)), want);
        assert_eq!(visible_menu(menu_layout(false, 0, true, None)), want);
        // Bản mới, ảnh ghim, phiên chụp từng bước: chưa làm trên Mac, vạch ngăn cũng bị bỏ.
        assert_eq!(visible_menu(menu_layout(true, 3, true, Some(2))), want);
    }

    #[test]
    fn separators_are_trimmed_and_collapsed() {
        assert_eq!(
            visible_menu(vec![
                Separator,
                Item(Capture),
                Separator,
                Item(History),
                Separator,
                Item(Quit),
                Separator
            ]),
            vec![Item(Capture), Separator, Item(Quit)]
        );
        assert_eq!(visible_menu(vec![Separator, Separator]), vec![]);
        assert_eq!(
            visible_menu(vec![Grayed(RepeatLast), Item(Quit)]),
            vec![Item(Quit)]
        );
    }

    #[test]
    fn menu_labels_come_from_core_messages() {
        assert_eq!(menu_msg(Capture), Some(Msg::TrayCapture));
        assert_eq!(menu_msg(Quit), Some(Msg::MacMenuQuit));
        assert_eq!(menu_msg(RepeatLast), Some(Msg::MenuRepeatLast));
        assert_eq!(menu_msg(History), Some(Msg::TrayHistory));
        assert_eq!(menu_msg(StepsStart), Some(Msg::TrayStepsStart));
        assert_eq!(menu_msg(Options), Some(Msg::TrayOptions));
        assert_eq!(menu_msg(About), Some(Msg::TrayAbout));
        assert_eq!(menu_msg(Help), Some(Msg::TrayHelp));
        for c in [OpenUpdate, StepsReview, ClosePins] {
            assert_eq!(menu_msg(c), None, "{c:?}");
        }
        // Mọi lệnh đã làm có nhãn cố định, đủ hai ngôn ngữ.
        for c in ALL.into_iter().filter(|c| implemented(*c)) {
            let m = menu_msg(c).expect("label");
            for l in Lang::ALL {
                assert!(!t(l, m).is_empty(), "{c:?} {l:?}");
            }
        }
        assert_eq!(t(Lang::Vi, Msg::TrayCapture), "Chụp màn hình");
        assert_eq!(t(Lang::En, Msg::MacMenuQuit), "Quit darkshot");
    }

    #[test]
    fn tags_round_trip_and_unknown_tags_are_ignored() {
        for c in ALL {
            let tag = tag_of(c);
            assert!(tag > 0, "{c:?}");
            assert_eq!(command_from_tag(tag), Some(c), "{c:?}");
        }
        let tags: std::collections::HashSet<isize> = ALL.iter().map(|c| tag_of(*c)).collect();
        assert_eq!(tags.len(), ALL.len());
        for bad in [0, -1, 12, isize::MAX, isize::MIN] {
            assert_eq!(command_from_tag(bad), None, "{bad}");
        }
    }
}
