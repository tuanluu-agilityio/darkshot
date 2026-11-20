//! Luồng ứng dụng thuần (lý do mở Tùy chọn, so sánh cấu hình, đối số dòng lệnh) dùng chung mọi nền tảng.

use crate::config::Config;
use crate::i18n::Lang;

/// Lý do mở cửa sổ Tùy chọn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenReason {
    /// Người dùng chọn "Tùy chọn..." ở menu.
    User,
    /// Ứng dụng tự mở vì R2 chưa cấu hình, mất khóa hoặc cấu hình lỗi.
    NeedsR2,
}

/// Gộp lý do của hai yêu cầu mở Tùy chọn đang chờ: `NeedsR2` thắng.
pub fn merge_reason(a: OpenReason, b: OpenReason) -> OpenReason {
    if a == OpenReason::NeedsR2 || b == OpenReason::NeedsR2 {
        OpenReason::NeedsR2
    } else {
        OpenReason::User
    }
}

/// Ngôn ngữ sau khi đóng Tùy chọn: chỉ đổi khi đã ghi cấu hình thành công.
pub fn lang_after_save(old: Lang, new: Lang, saved: bool) -> Lang {
    if saved {
        new
    } else {
        old
    }
}

/// Chữ trong cửa sổ Thông tin và Trợ giúp đã cũ sau khi lưu cấu hình mới (ngôn ngữ hoặc phím tắt đổi).
pub fn info_windows_stale(old: &Config, new: &Config) -> bool {
    old.language != new.language || old.hotkeys != new.hotkeys
}

/// Chữ trong cửa sổ Lịch sử đã cũ sau khi lưu cấu hình mới: chỉ khi đổi ngôn ngữ (cửa sổ này không
/// hiện phím tắt nên đổi phím không cần đóng).
pub fn history_window_stale(old: &Config, new: &Config) -> bool {
    old.language != new.language
}

/// Đối số dòng lệnh đã phân tích.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Args {
    /// `--history`: mở cửa sổ Lịch sử ngay khi khởi động.
    pub open_history: bool,
}

/// Phân tích đối số (không gồm tên chương trình); đối số lạ bị bỏ qua.
pub fn parse_args(args: &[String]) -> Args {
    Args {
        open_history: args.iter().any(|a| a == "--history"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_history_flag() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert!(!parse_args(&s(&[])).open_history);
        assert!(parse_args(&s(&["--history"])).open_history);
        assert!(parse_args(&s(&["--foo", "--history"])).open_history);
        assert!(!parse_args(&s(&["--histories", "history", "-history"])).open_history);
    }

    #[test]
    fn open_reason_merge_prefers_r2() {
        use OpenReason::*;
        assert_eq!(merge_reason(User, User), User);
        assert_eq!(merge_reason(User, NeedsR2), NeedsR2);
        assert_eq!(merge_reason(NeedsR2, User), NeedsR2);
    }

    #[test]
    fn language_changes_only_when_saved() {
        assert_eq!(lang_after_save(Lang::Vi, Lang::En, true), Lang::En);
        assert_eq!(lang_after_save(Lang::Vi, Lang::En, false), Lang::Vi);
    }

    #[test]
    fn info_windows_close_when_language_or_hotkey_changes() {
        use std::path::Path;
        let a = Config::defaults(Path::new("C:/p"), Lang::Vi);
        assert!(!info_windows_stale(&a, &a.clone()));
        let mut b = a.clone();
        b.language = Lang::En;
        assert!(info_windows_stale(&a, &b));
        let mut b = a.clone();
        b.hotkeys.region.key.ctrl = true;
        assert!(info_windows_stale(&a, &b));
        let mut b = a.clone();
        b.autostart = true;
        assert!(!info_windows_stale(&a, &b));
    }

    /// Lịch sử chỉ có chữ theo ngôn ngữ (không hiện phím tắt): đổi ngôn ngữ thì đóng, đổi phím thì không.
    #[test]
    fn history_window_closes_only_when_language_changes() {
        use std::path::Path;
        let a = Config::defaults(Path::new("C:/p"), Lang::Vi);
        assert!(!history_window_stale(&a, &a.clone()));
        let mut b = a.clone();
        b.language = Lang::En;
        assert!(history_window_stale(&a, &b));
        let mut b = a.clone();
        b.hotkeys.region.key.ctrl = !b.hotkeys.region.key.ctrl;
        assert!(info_windows_stale(&a, &b));
        assert!(!history_window_stale(&a, &b));
        let mut b = a.clone();
        b.history_enabled = !b.history_enabled;
        assert!(!history_window_stale(&a, &b));
    }

    #[test]
    fn info_windows_close_when_any_hotkey_row_changes() {
        use std::path::Path;
        let a = Config::defaults(Path::new("C:/p"), Lang::Vi);
        // Bật/tắt một dòng: stale.
        for r in crate::config::HotkeyRole::ALL {
            let mut b = a.clone();
            let mut slot = b.hotkeys.get(r);
            slot.enabled = !slot.enabled;
            b.hotkeys.set(r, slot);
            assert!(info_windows_stale(&a, &b), "{r:?}");
        }
        // Đổi tổ hợp của dòng 2 hay 3: stale.
        let mut b = a.clone();
        b.hotkeys.save_full.key.alt = true;
        assert!(info_windows_stale(&a, &b));
        let mut b = a.clone();
        b.hotkeys.upload_full.key.vk = 0x71;
        assert!(info_windows_stale(&a, &b));
        // Đổi show_success_notices: không stale.
        let mut b = a.clone();
        b.show_success_notices = !b.show_success_notices;
        assert!(!info_windows_stale(&a, &b));
    }
}
