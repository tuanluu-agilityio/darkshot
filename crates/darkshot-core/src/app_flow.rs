//! Luồng ứng dụng thuần (lý do mở Tùy chọn, so sánh cấu hình, đối số dòng lệnh) dùng chung mọi nền tảng.

use crate::app_state::ConfigNotice;
use crate::color::CustomColors;
use crate::config::Config;
use crate::geometry::{to_screen_coords, Point, Rect};
use crate::i18n::Lang;
use std::time::{Duration, Instant};

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

/// Khung cửa sổ `frames` (tọa độ màn hình) bỏ khung `skip` (nếu có).
pub fn without_frame(mut frames: Vec<Rect>, skip: Option<Rect>) -> Vec<Rect> {
    if let Some(r) = skip {
        frames.retain(|f| *f != r);
    }
    frames
}

/// Tổng thời gian chờ lúc thoát (ghi Lịch sử, ảnh bước, xuất), chia nhau theo một hạn chót chung;
/// Windows cho khoảng 5 giây ở `WM_ENDSESSION`.
pub const EXIT_BUDGET: Duration = Duration::from_secs(4);

/// Thời gian còn lại tới hạn chót `deadline` (0 khi đã qua).
pub fn remaining(deadline: Instant, now: Instant) -> Duration {
    deadline.saturating_duration_since(now)
}

/// Phím chụp tới khi một menu đang mở (menu khay, hoặc menu ngữ cảnh của ảnh ghim/Lịch sử): đóng
/// menu rồi chụp hoãn qua `GateEvent::MenuCapture`, không chạy lớp phủ lồng trong vòng lặp modal.
pub fn hotkey_defers(tray_menu_open: bool, pin_menu_open: bool) -> bool {
    tray_menu_open || pin_menu_open
}

/// Hẹn giờ chụp sau menu khay tới khi menu ngữ cảnh của ảnh ghim hay Lịch sử đang mở: chờ thêm.
pub fn timer_should_wait(pin_menu_open: bool, history_menu_open: bool) -> bool {
    pin_menu_open || history_menu_open
}

/// Góc trên trái của ảnh ghim (tọa độ màn hình) cho vùng `rect` (tọa độ ảnh chụp): góc sau khi kẹp
/// vào ảnh như `picked_image`/`crop`, cộng gốc `origin` của màn hình ảo đúng một lần.
pub fn pin_origin(rect: Rect, origin: Point) -> Point {
    let clamped = Rect {
        x: rect.x.max(0),
        y: rect.y.max(0),
        ..rect
    };
    let r = to_screen_coords(clamped, origin);
    Point { x: r.x, y: r.y }
}

/// Có ghi file cấu hình sau khi lớp phủ đóng không: chỉ khi màu tự chọn đã đổi và cấu hình
/// trong bộ nhớ khớp file (`config_ok`, xem `config_ok_at_start`). Cấu hình hỏng hoặc không đọc
/// được thì không bao giờ ghi đè file (sẽ mất khóa R2 trong đó).
pub fn custom_colors_need_save(
    before: &CustomColors,
    after: &CustomColors,
    config_ok: bool,
) -> bool {
    config_ok && before != after
}

/// Cấu hình đọc lúc khởi động có khớp file không: chỉ khi file đã có (`file_existed`) và đọc
/// bình thường, không thông báo nào (hỏng, không đọc được, hay mất khóa bí mật). Chưa có file
/// thì không ghi, để lần chạy đầu không tạo file với R2 rỗng (mất việc tự mở Tùy chọn).
pub fn config_ok_at_start(notice: Option<ConfigNotice>, file_existed: bool) -> bool {
    file_existed && notice.is_none()
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

    #[test]
    fn exit_waits_share_one_deadline() {
        let now = Instant::now();
        let deadline = now + EXIT_BUDGET;
        assert_eq!(remaining(deadline, now), EXIT_BUDGET);
        assert_eq!(
            remaining(deadline, now + Duration::from_secs(1)),
            EXIT_BUDGET - Duration::from_secs(1)
        );
        assert_eq!(
            remaining(deadline, deadline + Duration::from_secs(1)),
            Duration::ZERO
        );
        assert!(EXIT_BUDGET <= Duration::from_secs(4));
    }

    #[test]
    fn without_frame_drops_only_the_session_bar() {
        let r = |x, y, w, h| Rect { x, y, w, h };
        let bar = r(700, 12, 400, 40);
        let wins = vec![r(0, 0, 800, 600), bar, r(10, 10, 50, 50)];
        assert_eq!(
            without_frame(wins.clone(), Some(bar)),
            vec![r(0, 0, 800, 600), r(10, 10, 50, 50)]
        );
        assert_eq!(without_frame(wins.clone(), None), wins);
    }

    #[test]
    fn custom_colors_saved_only_when_changed() {
        use crate::theme::Rgb;
        let empty = CustomColors::default();
        let mut added = CustomColors::default();
        added.add(Rgb(1, 2, 3), None);
        // Đổi + cấu hình ổn -> ghi.
        assert!(custom_colors_need_save(&empty, &added, true));
        let mut other = added;
        other.add(Rgb(4, 5, 6), Some(0));
        assert!(custom_colors_need_save(&added, &other, true));
        // Đổi + cấu hình hỏng / không đọc được / mất khóa -> không ghi.
        for n in [
            ConfigNotice::Corrupt,
            ConfigNotice::Unreadable,
            ConfigNotice::SecretLost,
        ] {
            let ok = config_ok_at_start(Some(n), true);
            assert!(!ok, "{n:?}");
            assert!(!custom_colors_need_save(&empty, &added, ok));
        }
        assert!(config_ok_at_start(None, true));
        // Chưa có file lúc khởi động -> không ghi.
        assert!(!config_ok_at_start(None, false));
        assert!(!custom_colors_need_save(
            &empty,
            &added,
            config_ok_at_start(None, false)
        ));
        // Không đổi -> không ghi.
        assert!(!custom_colors_need_save(
            &empty,
            &CustomColors::default(),
            true
        ));
        assert!(!custom_colors_need_save(&added, &added.clone(), true));
        // Ghi lại đúng màu đang có ở cùng ô: không đổi, không ghi.
        let mut same = added;
        same.add(Rgb(1, 2, 3), Some(0));
        assert!(!custom_colors_need_save(&added, &same, true));
    }

    /// Góc ảnh ghim = góc trên trái vùng (đã kẹp vào ảnh) cộng gốc màn hình ảo, đúng một lần.
    #[test]
    fn pin_origin_is_clamped_corner_in_screen_coords() {
        let r = |x, y| Rect { x, y, w: 50, h: 40 };
        // Gốc 0: tọa độ ảnh trùng tọa độ màn hình.
        assert_eq!(
            pin_origin(r(10, 20), Point { x: 0, y: 0 }),
            Point { x: 10, y: 20 }
        );
        // Màn hình bên trái/trên: gốc âm.
        assert_eq!(
            pin_origin(r(10, 20), Point { x: -1920, y: -200 }),
            Point { x: -1910, y: -180 }
        );
        // Vùng tràn ra ngoài ảnh: góc sau khi kẹp (khớp gốc ảnh cắt của `picked_image`).
        assert_eq!(
            pin_origin(r(-15, -7), Point { x: -1920, y: -200 }),
            Point { x: -1920, y: -200 }
        );
        assert_eq!(
            pin_origin(r(-15, 30), Point { x: 0, y: 0 }),
            Point { x: 0, y: 30 }
        );
    }

    /// Phím chụp khi một menu (khay, ảnh ghim hay Lịch sử) đang mở: đóng menu rồi chụp hoãn.
    #[test]
    fn hotkey_defers_when_any_menu_open() {
        assert!(!hotkey_defers(false, false));
        assert!(hotkey_defers(true, false));
        assert!(hotkey_defers(false, true));
        assert!(hotkey_defers(true, true));
    }

    /// Hẹn giờ chụp sau menu: menu ảnh ghim hay Lịch sử còn mở thì chờ thêm, không chụp.
    #[test]
    fn timer_waits_while_pin_or_history_menu_open() {
        assert!(!timer_should_wait(false, false));
        assert!(timer_should_wait(true, false));
        assert!(timer_should_wait(false, true));
        assert!(timer_should_wait(true, true));
    }
}
