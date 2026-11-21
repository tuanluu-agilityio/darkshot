//! Biểu tượng khay, menu và thông báo.

use crate::lang;
use darkshot_core::actions::{NoticeKind, Notifier};
use darkshot_core::i18n::{self, Msg};
// Bố cục menu và hàng đợi thông báo nằm ở core (dùng chung với macOS); giữ tên cũ cho chỗ gọi.
use darkshot_core::tray_logic::{enqueue_notice, menu_layout, take_notice_to_show, MenuEntry};
pub use darkshot_core::tray_logic::{should_retry, MenuCommand, Notice};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ffi::c_void;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_ERROR, NIIF_INFO, NIM_ADD,
    NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, GetSystemMetrics, LoadIconW,
    LoadImageW, PostMessageW, SetForegroundWindow, TrackPopupMenu, HICON, IDI_APPLICATION,
    IMAGE_ICON, LR_SHARED, MF_GRAYED, MF_SEPARATOR, MF_STRING, SM_CXSMICON, SM_CYSMICON,
    TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_NULL,
};

/// Thông điệp riêng của biểu tượng khay.
pub const WM_TRAY: u32 = WM_APP + 1;
/// Thông báo từ luồng khác: `lparam` là `Box<Notice>` đã chuyển quyền sở hữu.
pub const WM_NOTICE: u32 = WM_APP + 2;

const TRAY_UID: u32 = 1;
const ID_CAPTURE: usize = 1;
const ID_OPTIONS: usize = 2;
const ID_ABOUT: usize = 3;
const ID_HELP: usize = 4;
const ID_QUIT: usize = 5;
const ID_UPDATE: usize = 6;
const ID_HISTORY: usize = 7;
const ID_CLOSE_PINS: usize = 8;
const ID_REPEAT_LAST: usize = 9;
const ID_STEPS_START: usize = 10;
const ID_STEPS_REVIEW: usize = 11;

pub struct Tray {
    hwnd: HWND,
    icon: HICON,
    added: Cell<bool>,
    /// Thông báo đến lúc biểu tượng chưa có.
    pending: RefCell<VecDeque<Notice>>,
    /// Bong bóng gần nhất là thông báo có bản mới (bấm vào thì mở trang tải về).
    update_notice: Cell<bool>,
}

fn hwnd_from(v: isize) -> HWND {
    HWND(v as *mut c_void)
}

/// Chép chuỗi vào mảng UTF-16 cố định, cắt bớt và luôn kết thúc bằng 0.
pub fn fill_wide<const N: usize>(dst: &mut [u16; N], s: &str) {
    *dst = [0; N];
    for (slot, u) in dst.iter_mut().take(N - 1).zip(s.encode_utf16()) {
        *slot = u;
    }
}

impl Tray {
    fn base_data(&self) -> NOTIFYICONDATAW {
        NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: TRAY_UID,
            ..Default::default()
        }
    }

    fn full_data(&self) -> NOTIFYICONDATAW {
        let mut nid = self.base_data();
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY;
        nid.hIcon = self.icon;
        fill_wide(&mut nid.szTip, "darkshot");
        nid
    }

    pub fn new(hwnd: HWND) -> windows::core::Result<Tray> {
        // SAFETY: tài nguyên biểu tượng số 1 được nhúng bởi build.rs; thiếu thì dùng biểu tượng mặc định.
        // Dùng LR_SHARED nên không cần DestroyIcon. Nạp đúng cỡ nhỏ của khay (16/20/24 vẽ theo lưới).
        let icon = unsafe {
            let hinst = GetModuleHandleW(None)?;
            let res = PCWSTR(std::ptr::without_provenance::<u16>(1));
            let cx = GetSystemMetrics(SM_CXSMICON);
            let cy = GetSystemMetrics(SM_CYSMICON);
            let small = LoadImageW(Some(hinst.into()), res, IMAGE_ICON, cx, cy, LR_SHARED)
                .map(|h| HICON(h.0));
            match small {
                Ok(i) => i,
                Err(_) => match LoadIconW(Some(hinst.into()), res) {
                    Ok(i) => i,
                    Err(_) => LoadIconW(None, IDI_APPLICATION)?,
                },
            }
        };
        let tray = Tray {
            hwnd,
            icon,
            added: Cell::new(false),
            pending: RefCell::new(VecDeque::new()),
            update_notice: Cell::new(false),
        };
        // Explorer có thể chưa sẵn sàng (tự khởi động cùng Windows): không phải lỗi chết.
        tray.try_add();
        Ok(tray)
    }

    /// Biểu tượng đã nằm trong khay chưa.
    pub fn is_added(&self) -> bool {
        self.added.get()
    }

    /// Thử thêm biểu tượng; trả về `true` nếu đã có trong khay.
    pub fn try_add(&self) -> bool {
        let nid = self.full_data();
        // SAFETY: nid được khởi tạo đầy đủ.
        let ok = unsafe { Shell_NotifyIconW(NIM_ADD, &nid).as_bool() };
        self.set_added(ok)
    }

    /// Ghi nhận trạng thái biểu tượng; vừa có thì hiện thông báo đang chờ (nếu có).
    fn set_added(&self, ok: bool) -> bool {
        self.added.set(ok);
        if ok {
            let next = take_notice_to_show(&mut self.pending.borrow_mut());
            if let Some(n) = next {
                self.show_notice(&n);
            }
        }
        ok
    }

    /// Hiện menu tại con trỏ chuột và trả về mục được chọn. `update` là chữ của mục "Có bản mới";
    /// `pins` là số ảnh ghim đang mở (lớn hơn 0 thì có mục "Đóng mọi ảnh ghim (n)"); `can_repeat`
    /// là có lần chụp trước để lặp (không có thì mục "Chụp lại vùng trước" mờ); `steps` là số bước
    /// của phiên chụp từng bước đang có (`None`: không có phiên).
    pub fn show_menu(
        &self,
        update: Option<&str>,
        pins: usize,
        can_repeat: bool,
        steps: Option<usize>,
    ) -> Option<MenuCommand> {
        // SAFETY: menu được tạo và hủy trong hàm này; hwnd thuộc luồng gọi.
        unsafe {
            let menu = CreatePopupMenu().ok()?;
            let capture = HSTRING::from(lang::tr(Msg::TrayCapture));
            let repeat_last = HSTRING::from(lang::tr(Msg::MenuRepeatLast));
            let history = HSTRING::from(lang::tr(Msg::TrayHistory));
            let close_pins = HSTRING::from(i18n::tray_close_pins(lang::current(), pins));
            let steps_start = HSTRING::from(lang::tr(Msg::TrayStepsStart));
            let steps_review =
                HSTRING::from(i18n::tray_steps_review(lang::current(), steps.unwrap_or(0)));
            let options = HSTRING::from(lang::tr(Msg::TrayOptions));
            let about = HSTRING::from(lang::tr(Msg::TrayAbout));
            let help = HSTRING::from(lang::tr(Msg::TrayHelp));
            let exit = HSTRING::from(lang::tr(Msg::TrayExit));
            let update_text = HSTRING::from(update.unwrap_or_default());
            for entry in menu_layout(update.is_some(), pins, can_repeat, steps) {
                match entry {
                    MenuEntry::Separator => {
                        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                    }
                    MenuEntry::Item(MenuCommand::OpenUpdate) => {
                        let _ = AppendMenuW(menu, MF_STRING, ID_UPDATE, &update_text);
                    }
                    MenuEntry::Item(MenuCommand::Capture) => {
                        let _ = AppendMenuW(menu, MF_STRING, ID_CAPTURE, &capture);
                    }
                    MenuEntry::Item(MenuCommand::RepeatLast) => {
                        let _ = AppendMenuW(menu, MF_STRING, ID_REPEAT_LAST, &repeat_last);
                    }
                    MenuEntry::Grayed(MenuCommand::RepeatLast) => {
                        let _ =
                            AppendMenuW(menu, MF_STRING | MF_GRAYED, ID_REPEAT_LAST, &repeat_last);
                    }
                    // `menu_layout` chỉ làm mờ mục lặp lại.
                    MenuEntry::Grayed(_) => {}
                    MenuEntry::Item(MenuCommand::History) => {
                        let _ = AppendMenuW(menu, MF_STRING, ID_HISTORY, &history);
                    }
                    MenuEntry::Item(MenuCommand::StepsStart) => {
                        let _ = AppendMenuW(menu, MF_STRING, ID_STEPS_START, &steps_start);
                    }
                    MenuEntry::Item(MenuCommand::StepsReview) => {
                        let _ = AppendMenuW(menu, MF_STRING, ID_STEPS_REVIEW, &steps_review);
                    }
                    MenuEntry::Item(MenuCommand::ClosePins) => {
                        let _ = AppendMenuW(menu, MF_STRING, ID_CLOSE_PINS, &close_pins);
                    }
                    MenuEntry::Item(MenuCommand::Options) => {
                        let _ = AppendMenuW(menu, MF_STRING, ID_OPTIONS, &options);
                    }
                    MenuEntry::Item(MenuCommand::About) => {
                        let _ = AppendMenuW(menu, MF_STRING, ID_ABOUT, &about);
                    }
                    MenuEntry::Item(MenuCommand::Help) => {
                        let _ = AppendMenuW(menu, MF_STRING, ID_HELP, &help);
                    }
                    MenuEntry::Item(MenuCommand::Quit) => {
                        let _ = AppendMenuW(menu, MF_STRING, ID_QUIT, &exit);
                    }
                }
            }
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            // Cần để menu đóng khi bấm ra ngoài.
            let _ = SetForegroundWindow(self.hwnd);
            let cmd = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
                pt.x,
                pt.y,
                Some(0),
                self.hwnd,
                None,
            );
            let _ = PostMessageW(Some(self.hwnd), WM_NULL, WPARAM(0), LPARAM(0));
            let _ = DestroyMenu(menu);
            match cmd.0 as usize {
                ID_UPDATE => Some(MenuCommand::OpenUpdate),
                ID_CAPTURE => Some(MenuCommand::Capture),
                ID_REPEAT_LAST => Some(MenuCommand::RepeatLast),
                ID_HISTORY => Some(MenuCommand::History),
                ID_STEPS_START => Some(MenuCommand::StepsStart),
                ID_STEPS_REVIEW => Some(MenuCommand::StepsReview),
                ID_CLOSE_PINS => Some(MenuCommand::ClosePins),
                ID_OPTIONS => Some(MenuCommand::Options),
                ID_ABOUT => Some(MenuCommand::About),
                ID_HELP => Some(MenuCommand::Help),
                ID_QUIT => Some(MenuCommand::Quit),
                _ => None,
            }
        }
    }

    /// Thêm lại biểu tượng khi Explorer khởi động lại ("TaskbarCreated").
    /// `NIM_ADD` thất bại (biểu tượng có thể vẫn còn) thì thử `NIM_MODIFY` trước khi coi là chưa có.
    pub fn readd(&self) -> bool {
        let nid = self.full_data();
        // SAFETY: nid được khởi tạo đầy đủ.
        let ok = unsafe {
            Shell_NotifyIconW(NIM_ADD, &nid).as_bool()
                || Shell_NotifyIconW(NIM_MODIFY, &nid).as_bool()
        };
        self.set_added(ok)
    }

    /// Hiện bong bóng thông báo. Chỉ gọi ở luồng chính.
    /// Chưa có biểu tượng thì xếp vào hàng đợi, hiện khi biểu tượng được thêm.
    pub fn show_notice(&self, n: &Notice) {
        self.update_notice.set(false);
        self.display(n);
    }

    /// Hiện thông báo có bản mới và nhớ rằng bong bóng này mở trang tải về khi được bấm.
    /// Biểu tượng chưa có thì thông báo xếp hàng và không mở link (nó có thể bị thay trước khi hiện).
    pub fn show_update_notice(&self, n: &Notice) {
        self.display(n);
        self.update_notice.set(self.added.get());
    }

    /// Thông báo khay gần nhất là thông báo có bản mới.
    pub fn is_update_notice(&self) -> bool {
        self.update_notice.get()
    }

    fn display(&self, n: &Notice) {
        if !self.added.get() {
            enqueue_notice(&mut self.pending.borrow_mut(), n.clone());
            return;
        }
        let mut nid = self.base_data();
        nid.uFlags = NIF_INFO;
        nid.dwInfoFlags = match n.kind {
            NoticeKind::Success | NoticeKind::Info => NIIF_INFO,
            NoticeKind::Error => NIIF_ERROR,
        };
        fill_wide(&mut nid.szInfoTitle, &n.title);
        fill_wide(&mut nid.szInfo, &n.body);
        // SAFETY: nid được khởi tạo đầy đủ.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    }

    pub fn notifier(&self) -> TrayNotifier {
        TrayNotifier {
            hwnd: self.hwnd.0 as isize,
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        let nid = self.base_data();
        // SAFETY: gỡ biểu tượng của chính ta.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        }
    }
}

/// Gửi thông báo về luồng chính; an toàn khi gọi từ luồng bất kỳ.
#[derive(Clone)]
pub struct TrayNotifier {
    hwnd: isize,
}

impl Notifier for TrayNotifier {
    fn notify(&self, kind: NoticeKind, title: &str, body: &str) {
        let raw = Box::into_raw(Box::new(Notice {
            kind,
            title: title.to_owned(),
            body: body.to_owned(),
        }));
        // SAFETY: con trỏ chỉ được giải phóng đúng một lần: ở đây nếu gửi thất bại,
        // hoặc ở luồng chính khi xử lý WM_NOTICE.
        let posted = unsafe {
            PostMessageW(
                Some(hwnd_from(self.hwnd)),
                WM_NOTICE,
                WPARAM(0),
                LPARAM(raw as isize),
            )
        };
        if posted.is_err() {
            // SAFETY: tin nhắn không vào hàng đợi nên raw vẫn thuộc về ta.
            drop(unsafe { Box::from_raw(raw) });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Phân tích ICONDIR của `res/darkshot.ico`: đủ tám cỡ, 32 bit, offset/kích thước hợp lệ.
    #[test]
    fn ico_has_all_sizes_32bit_within_file() {
        let ico: &[u8] = include_bytes!("../res/darkshot.ico");
        assert!(ico.len() >= 6, "file quá ngắn");
        let u16at = |o: usize| u16::from_le_bytes([ico[o], ico[o + 1]]) as usize;
        let u32at =
            |o: usize| u32::from_le_bytes([ico[o], ico[o + 1], ico[o + 2], ico[o + 3]]) as usize;
        assert_eq!(u16at(0), 0, "reserved");
        assert_eq!(u16at(2), 1, "type phải là icon");
        let count = u16at(4);
        assert!(ico.len() >= 6 + 16 * count, "thiếu mục ICONDIRENTRY");
        let mut sizes = Vec::new();
        for i in 0..count {
            let e = 6 + 16 * i;
            let w = if ico[e] == 0 { 256 } else { ico[e] as usize };
            let h = if ico[e + 1] == 0 {
                256
            } else {
                ico[e + 1] as usize
            };
            assert_eq!(w, h, "cỡ phải vuông");
            assert_eq!(u16at(e + 6), 32, "cỡ {w} phải 32 bit");
            let size = u32at(e + 8);
            let off = u32at(e + 12);
            assert!(size > 0 && off + size <= ico.len(), "cỡ {w} vượt file");
            sizes.push(w);
        }
        sizes.sort_unstable();
        assert_eq!(sizes, [16, 20, 24, 32, 40, 48, 64, 256]);
    }

    /// Khung phải là bốn góc chữ L rời nhau: điểm giữa cạnh trên và cạnh trái (cỡ 32) là nền tối.
    #[test]
    fn ico_32_frame_edges_are_open_at_midpoints() {
        let ico: &[u8] = include_bytes!("../res/darkshot.ico");
        let u32at =
            |o: usize| u32::from_le_bytes([ico[o], ico[o + 1], ico[o + 2], ico[o + 3]]) as usize;
        let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
        let e = (0..count)
            .map(|i| 6 + 16 * i)
            .find(|&e| ico[e] == 32)
            .expect("thiếu cỡ 32");
        let base = u32at(e + 12) + 40; // bỏ BITMAPINFOHEADER; điểm ảnh BGRA, từ dưới lên
        let blue = |x: usize, y: usize| ico[base + ((31 - y) * 32 + x) * 4]; // nền blue 0x28, nét nhấn 0xFF
        let m = (0.285f32 * 32.0).round() as usize; // đường giữa nét của cạnh khung
        assert!(blue(16, m) < 0x60, "giữa cạnh trên bị tô màu nhấn");
        assert!(blue(m, 16) < 0x60, "giữa cạnh trái bị tô màu nhấn");
        assert!(blue(m, m) > 200, "góc trên trái phải là nét màu nhấn");
    }

    #[test]
    fn fill_wide_truncates_and_terminates() {
        let mut a = [9u16; 4];
        fill_wide(&mut a, "abcdef");
        assert_eq!(a, [97, 98, 99, 0]);
        fill_wide(&mut a, "a");
        assert_eq!(a, [97, 0, 0, 0]);
    }
}
