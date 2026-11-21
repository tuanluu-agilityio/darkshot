//! Luật menu khay và hàng đợi thông báo dùng chung mọi nền tảng: bố cục mục menu, hàng đợi thông
//! báo khi biểu tượng chưa có, chính sách thử lại.

use crate::actions::NoticeKind;
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuCommand {
    OpenUpdate,
    Capture,
    /// Chụp lại vùng trước (mờ khi chưa có lần chụp nào để lặp).
    RepeatLast,
    History,
    /// Bắt đầu phiên chụp từng bước (chỉ khi chưa có phiên).
    StepsStart,
    /// Mở cửa sổ Xem lại các bước (chỉ khi có phiên).
    StepsReview,
    /// Đóng mọi ảnh ghim (chỉ có khi đang có ảnh ghim).
    ClosePins,
    Options,
    About,
    Help,
    Quit,
}

/// Nội dung một thông báo (lớp Windows gửi qua `PostMessageW`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub kind: NoticeKind,
    pub title: String,
    pub body: String,
}

/// Một dòng của menu khay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuEntry {
    Item(MenuCommand),
    /// Mục hiện mờ, không chọn được (`MF_GRAYED`).
    Grayed(MenuCommand),
    Separator,
}

/// Các dòng của menu khay theo thứ tự; có bản mới thì mục cập nhật và vạch ngăn đứng đầu; có ảnh
/// ghim (`pins` > 0) thì "Đóng mọi ảnh ghim" ngay sau Lịch sử. "Chụp lại vùng trước" luôn ngay sau
/// "Chụp", mờ khi `can_repeat` là `false`. Ngay sau Lịch sử: "Chụp từng bước" khi không có phiên
/// (`steps` là `None`), "Xem lại các bước (N)" khi có phiên.
pub fn menu_layout(
    has_update: bool,
    pins: usize,
    can_repeat: bool,
    steps: Option<usize>,
) -> Vec<MenuEntry> {
    let mut v = Vec::new();
    if has_update {
        v.push(MenuEntry::Item(MenuCommand::OpenUpdate));
        v.push(MenuEntry::Separator);
    }
    v.push(MenuEntry::Item(MenuCommand::Capture));
    v.push(if can_repeat {
        MenuEntry::Item(MenuCommand::RepeatLast)
    } else {
        MenuEntry::Grayed(MenuCommand::RepeatLast)
    });
    v.push(MenuEntry::Item(MenuCommand::History));
    v.push(MenuEntry::Item(match steps {
        None => MenuCommand::StepsStart,
        Some(_) => MenuCommand::StepsReview,
    }));
    if pins > 0 {
        v.push(MenuEntry::Item(MenuCommand::ClosePins));
    }
    v.extend(
        [
            MenuCommand::Options,
            MenuCommand::About,
            MenuCommand::Help,
            MenuCommand::Quit,
        ]
        .map(MenuEntry::Item),
    );
    v
}

/// Số thông báo giữ lại tối đa trong lúc biểu tượng khay chưa có.
pub const MAX_QUEUED_NOTICES: usize = 8;

/// Xếp một thông báo vào hàng đợi; đầy thì bỏ cái cũ nhất.
pub fn enqueue_notice(q: &mut VecDeque<Notice>, n: Notice) {
    while q.len() >= MAX_QUEUED_NOTICES {
        q.pop_front();
    }
    q.push_back(n);
}

/// Chọn thông báo để hiện khi biểu tượng vừa có: cái mới nhất mức Error nếu có, không thì cái
/// mới nhất. Hàng đợi được làm rỗng.
pub fn take_notice_to_show(q: &mut VecDeque<Notice>) -> Option<Notice> {
    let pick = match q.iter().rposition(|n| n.kind == NoticeKind::Error) {
        Some(i) => q.remove(i),
        None => q.pop_back(),
    };
    q.clear();
    pick
}

/// Số lần thử lại tối đa khi chưa thêm được biểu tượng.
pub const MAX_ADD_RETRIES: u32 = 30;

/// Có nên thử thêm lại biểu tượng khay không (`attempts` là số lần đã thử lại).
pub fn should_retry(added: bool, attempts: u32) -> bool {
    !added && attempts < MAX_ADD_RETRIES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_layout_puts_update_first_with_separator() {
        use MenuCommand::*;
        use MenuEntry::*;
        assert_eq!(
            menu_layout(true, 0, true, None),
            vec![
                Item(OpenUpdate),
                Separator,
                Item(Capture),
                Item(RepeatLast),
                Item(History),
                Item(StepsStart),
                Item(Options),
                Item(About),
                Item(Help),
                Item(Quit)
            ]
        );
        assert_eq!(
            menu_layout(false, 0, true, None),
            vec![
                Item(Capture),
                Item(RepeatLast),
                Item(History),
                Item(StepsStart),
                Item(Options),
                Item(About),
                Item(Help),
                Item(Quit)
            ]
        );
    }

    /// Có ảnh ghim: "Đóng mọi ảnh ghim (n)" ngay sau Lịch sử, có hay không có bản mới.
    #[test]
    fn menu_layout_adds_close_pins_after_history() {
        use MenuCommand::*;
        use MenuEntry::*;
        assert_eq!(
            menu_layout(false, 3, true, None),
            vec![
                Item(Capture),
                Item(RepeatLast),
                Item(History),
                Item(StepsStart),
                Item(ClosePins),
                Item(Options),
                Item(About),
                Item(Help),
                Item(Quit)
            ]
        );
        assert_eq!(
            menu_layout(true, 3, true, None),
            vec![
                Item(OpenUpdate),
                Separator,
                Item(Capture),
                Item(RepeatLast),
                Item(History),
                Item(StepsStart),
                Item(ClosePins),
                Item(Options),
                Item(About),
                Item(Help),
                Item(Quit)
            ]
        );
        assert_eq!(menu_layout(true, 1, true, None).len(), 11);
        assert!(!menu_layout(true, 0, true, None).contains(&Item(ClosePins)));
    }

    /// Chưa có lần chụp để lặp: mục "Chụp lại vùng trước" vẫn ở ngay sau "Chụp" nhưng mờ.
    #[test]
    fn menu_layout_grays_repeat_last_without_capture() {
        use MenuCommand::*;
        use MenuEntry::*;
        assert_eq!(
            menu_layout(false, 0, false, None),
            vec![
                Item(Capture),
                Grayed(RepeatLast),
                Item(History),
                Item(StepsStart),
                Item(Options),
                Item(About),
                Item(Help),
                Item(Quit)
            ]
        );
        let with_update = menu_layout(true, 2, false, None);
        assert_eq!(&with_update[2..4], &[Item(Capture), Grayed(RepeatLast)]);
        assert!(!with_update.contains(&Item(RepeatLast)));
        assert!(menu_layout(true, 2, true, None).contains(&Item(RepeatLast)));
        assert!(!menu_layout(true, 2, true, None).contains(&Grayed(RepeatLast)));
    }

    /// Không phiên: "Chụp từng bước" ngay sau Lịch sử; có phiên: "Xem lại các bước (N)" ở đúng chỗ đó,
    /// trước "Đóng mọi ảnh ghim".
    #[test]
    fn menu_layout_steps_entry() {
        use MenuCommand::*;
        use MenuEntry::*;
        let none = menu_layout(false, 0, true, None);
        assert_eq!(&none[2..4], &[Item(History), Item(StepsStart)]);
        assert!(!none.contains(&Item(StepsReview)));
        let some = menu_layout(false, 2, true, Some(3));
        assert_eq!(
            some,
            vec![
                Item(Capture),
                Item(RepeatLast),
                Item(History),
                Item(StepsReview),
                Item(ClosePins),
                Item(Options),
                Item(About),
                Item(Help),
                Item(Quit)
            ]
        );
        assert!(!some.contains(&Item(StepsStart)));
        // Phiên rỗng vẫn là phiên.
        assert!(menu_layout(true, 0, false, Some(0)).contains(&Item(StepsReview)));
    }

    #[test]
    fn tray_add_retry_policy() {
        assert!(!should_retry(true, 0));
        assert!(should_retry(false, 0));
        assert!(should_retry(false, 29));
        assert!(!should_retry(false, 30));
    }

    fn note(kind: NoticeKind, title: &str) -> Notice {
        Notice {
            kind,
            title: title.into(),
            body: String::new(),
        }
    }

    #[test]
    fn notice_queue_keeps_newest_eight() {
        let mut q = VecDeque::new();
        for i in 0..10 {
            enqueue_notice(&mut q, note(NoticeKind::Info, &i.to_string()));
        }
        assert_eq!(q.len(), MAX_QUEUED_NOTICES);
        let titles: Vec<_> = q.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(titles, ["2", "3", "4", "5", "6", "7", "8", "9"]);
    }

    #[test]
    fn notice_to_show_prefers_newest_error() {
        let mut q = VecDeque::new();
        enqueue_notice(&mut q, note(NoticeKind::Error, "e1"));
        enqueue_notice(&mut q, note(NoticeKind::Error, "e2"));
        enqueue_notice(&mut q, note(NoticeKind::Info, "i3"));
        assert_eq!(
            take_notice_to_show(&mut q),
            Some(note(NoticeKind::Error, "e2"))
        );
        assert!(q.is_empty());

        enqueue_notice(&mut q, note(NoticeKind::Info, "i1"));
        enqueue_notice(&mut q, note(NoticeKind::Info, "i2"));
        assert_eq!(
            take_notice_to_show(&mut q),
            Some(note(NoticeKind::Info, "i2"))
        );
        assert!(q.is_empty());
        assert_eq!(take_notice_to_show(&mut q), None);
    }
}
