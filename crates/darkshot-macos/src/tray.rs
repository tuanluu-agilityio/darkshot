//! Biểu tượng trên thanh menu (`NSStatusItem`) và menu của nó, dựng từ
//! `darkshot_core::tray_logic::menu_layout`. Lệnh chưa làm trên Mac bị loại khỏi menu bằng
//! `implemented` (Task sau chỉ việc bật).

use darkshot_core::i18n::Msg;
use darkshot_core::tray_logic::{MenuCommand, MenuEntry};

/// Lệnh đã làm trên bản Mac. Lệnh trả `false` không xuất hiện trong menu.
pub fn implemented(cmd: MenuCommand) -> bool {
    let _ = cmd;
    true // STUB
}

/// Nhãn cố định của một lệnh menu; `None` cho nhãn có tham số (bản mới, số bước, số ảnh ghim),
/// bên gọi dựng bằng hàm của `i18n`.
pub fn menu_msg(cmd: MenuCommand) -> Option<Msg> {
    let _ = cmd;
    None // STUB
}

/// Các dòng menu sẽ hiện: bỏ lệnh chưa làm; vạch ngăn không đứng đầu, cuối hay liền nhau.
pub fn visible_menu(layout: Vec<MenuEntry>) -> Vec<MenuEntry> {
    layout // STUB
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
}
