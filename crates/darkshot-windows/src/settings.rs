//! Cửa sổ Tùy chọn bốn tab (Win32 thuần, Common Controls v6 qua manifest).
//!
//! Phần logic biểu mẫu (`apply_form`, `key_route`, `tab_of`, `initial_tab`) nằm ở `darkshot_core::settings_form`;
//! `capture_hotkey` là hàm thuần có kiểm thử; phần cửa sổ mỏng.
//! `show` chạy vòng lặp thông điệp riêng (hộp thoại kiểu modal) và bảo đảm khi trả về:
//! OK thì phím tắt mới (nếu đổi) đã được đăng ký cho `parent`; Hủy thì phím tắt cũ đã đăng ký lại.
//! Việc ghi `Config` ra đĩa và bật/tắt khởi động cùng Windows do bên gọi (Task 11) làm.

use crate::hotkey;
use crate::lang;
use crate::theme::{self, ButtonKind};
use crate::winutil::scale;
use darkshot_core::config::{
    clamp_jpeg_quality, Config, Expiry, Hotkey, HotkeyRole, Hotkeys, UploadFormat,
    JPEG_QUALITY_MAX, JPEG_QUALITY_MIN,
};
use darkshot_core::i18n::{self, expiry_label, t, Lang, Msg};
use darkshot_core::settings_form::{
    apply_form, initial_tab, key_route, quality_enabled, tab_of, tabs_fit, Field, Form, FormError,
    KeyRoute, SettingsResult, Tab,
};
use darkshot_core::theme::{
    Rgb, BG, DANGER, INPUT, LINE, SUCCESS, SURFACE, TEXT, TEXT_DISABLED, TEXT_MUTED,
};
use darkshot_core::upload::R2Uploader;
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use windows::core::{w, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, DeleteObject, DrawTextW, EndPaint, ExcludeClipRect, FillRect, GetDC,
    GetTextExtentPoint32W, GetWindowDC, InvalidateRect, RedrawWindow, ReleaseDC, ScreenToClient,
    SelectObject, DT_CALCRECT, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE,
    DT_VCENTER, DT_WORDBREAK, HDC, HFONT, HGDIOBJ, PAINTSTRUCT, RDW_INVALIDATE, RDW_UPDATENOW,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    InitCommonControlsEx, EM_GETRECT, EM_SETMARGINS, ICC_BAR_CLASSES, ICC_TAB_CLASSES,
    INITCOMMONCONTROLSEX, NMHDR, NM_CUSTOMDRAW, TBM_SETLINESIZE, TBM_SETPAGESIZE, TBM_SETPOS,
    TBM_SETRANGE, TBS_HORZ, TCHITTESTINFO, TCIF_TEXT, TCITEMW, TCM_ADJUSTRECT, TCM_GETCURFOCUS,
    TCM_GETCURSEL, TCM_GETITEMCOUNT, TCM_GETITEMRECT, TCM_HITTEST, TCM_INSERTITEMW, TCM_SETCURSEL,
    TCN_SELCHANGE, TCN_SELCHANGING, TRACKBAR_CLASSW, WC_TABCONTROLW, WM_MOUSELEAVE,
};
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, GetFocus, GetKeyState, IsWindowEnabled, SetFocus, TrackMouseEvent, TME_LEAVE,
    TRACKMOUSEEVENT, VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::Shell::{
    DefSubclassProc, FileOpenDialog, IFileOpenDialog, RemoveWindowSubclass, SetWindowSubclass,
    FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetMessageW,
    GetParent, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, IsChild, IsDialogMessageW,
    IsIconic, KillTimer, PostMessageW, PostQuitMessage, RegisterClassW, SendMessageW,
    SetForegroundWindow, SetTimer, SetWindowPos, SetWindowTextW, ShowWindow, TranslateMessage,
    BM_CLICK, BM_GETCHECK, BM_SETCHECK, BS_AUTOCHECKBOX, BS_AUTORADIOBUTTON, BS_DEFPUSHBUTTON,
    BS_PUSHBUTTON, CBS_DROPDOWNLIST, CB_ADDSTRING, CB_GETCURSEL, CB_GETDROPPEDSTATE, CB_SETCURSEL,
    CB_SHOWDROPDOWN, CW_USEDEFAULT, ES_AUTOHSCROLL, ES_PASSWORD, ES_READONLY, GWL_STYLE, HMENU,
    HWND_BOTTOM, MSG, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, SW_RESTORE,
    SW_SHOW, WA_INACTIVE, WINDOW_EX_STYLE, WINDOW_STYLE, WM_ACTIVATE, WM_APP, WM_CLOSE, WM_COMMAND,
    WM_CTLCOLORSTATIC, WM_ERASEBKGND, WM_HSCROLL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN,
    WM_MBUTTONDOWN, WM_NCLBUTTONDOWN, WM_NCRBUTTONDOWN, WM_NOTIFY, WM_PRINTCLIENT, WM_RBUTTONDOWN,
    WM_SETFONT, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER, WNDCLASSW, WS_CAPTION, WS_CHILD,
    WS_CLIPSIBLINGS, WS_DISABLED, WS_EX_APPWINDOW, WS_EX_CONTROLPARENT, WS_GROUP, WS_SYSMENU,
    WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetDlgCtrlID, GetSystemMetrics, GetWindowRect, CB_GETLBTEXT, CB_GETLBTEXTLEN,
    NCCALCSIZE_PARAMS, SM_CXBORDER, SWP_FRAMECHANGED, WM_CHAR, WM_CTLCOLOREDIT, WM_CTLCOLORLISTBOX,
    WM_ENABLE, WM_KILLFOCUS, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCALCSIZE,
    WM_NCDESTROY, WM_NCPAINT, WM_PAINT, WM_SETFOCUS, WM_UPDATEUISTATE, WS_CLIPCHILDREN,
};

/// Lý do mở cửa sổ Tùy chọn (định nghĩa ở core, dùng chung mọi nền tảng).
pub use darkshot_core::app_flow::OpenReason;

/// Kết quả xử lý một lần nhả phím khi đang chọn phím tắt mới.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capture {
    /// Phím bổ trợ đứng một mình: chờ tiếp.
    Ignore,
    /// Phím thường cần kèm Ctrl, Alt hoặc Win (Shift một mình không đủ).
    NeedModifier,
    /// Không nuốt phím: để hệ thống xử lý như thường (Alt+F4 đóng cửa sổ).
    PassThrough,
    Got(Hotkey),
}

/// Sự kiện phím khi đang chọn phím tắt, kèm trạng thái phím bổ trợ lúc xảy ra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEv {
    Down {
        vk: u32,
        ctrl: bool,
        alt: bool,
        shift: bool,
        win: bool,
    },
    Up {
        vk: u32,
        ctrl: bool,
        alt: bool,
        shift: bool,
        win: bool,
    },
}

/// Máy trạng thái của chế độ bắt phím: chốt tổ hợp ở KEYDOWN (đúng phím bổ trợ lúc bấm);
/// KEYUP không có KEYDOWN tương ứng bị bỏ, trừ PrintScreen (không sinh KEYDOWN).
#[derive(Debug, Default)]
pub struct CaptureState {
    downs: Vec<u32>,
}

impl CaptureState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, ev: KeyEv) -> Capture {
        match ev {
            KeyEv::Down {
                vk,
                ctrl,
                alt,
                shift,
                win,
            } => {
                let out = capture_hotkey(vk, ctrl, alt, shift, win);
                if matches!(out, Capture::Got(_) | Capture::NeedModifier)
                    && !self.downs.contains(&vk)
                {
                    self.downs.push(vk);
                }
                out
            }
            KeyEv::Up {
                vk,
                ctrl,
                alt,
                shift,
                win,
            } => {
                if let Some(i) = self.downs.iter().position(|k| *k == vk) {
                    self.downs.swap_remove(i);
                    Capture::Ignore
                } else if vk == 0x2C {
                    // PrintScreen không sinh KEYDOWN: nhận ở KEYUP.
                    capture_hotkey(vk, ctrl, alt, shift, win)
                } else {
                    Capture::Ignore
                }
            }
        }
    }
}

/// Phím cho phép đứng một mình làm phím tắt: PrintScreen, Pause, ScrollLock, F1..F24.
fn allowed_bare(vk: u32) -> bool {
    matches!(vk, 0x2C | 0x13 | 0x91 | 0x70..=0x87)
}

fn is_modifier_vk(vk: u32) -> bool {
    matches!(vk, 0x10 | 0x11 | 0x12 | 0x5B | 0x5C | 0xA0..=0xA5)
}

pub fn capture_hotkey(vk: u32, ctrl: bool, alt: bool, shift: bool, win: bool) -> Capture {
    if is_modifier_vk(vk) {
        return Capture::Ignore;
    }
    // Alt+F4 luôn là lệnh đóng cửa sổ của hệ thống.
    if vk == 0x73 && alt {
        return Capture::PassThrough;
    }
    // Shift một mình không đủ: sẽ chặn việc gõ chữ hoa.
    if ctrl || alt || win || allowed_bare(vk) {
        Capture::Got(Hotkey {
            ctrl,
            alt,
            shift,
            win,
            vk,
        })
    } else {
        Capture::NeedModifier
    }
}

// ---------------------------------------------------------------------------
// Cửa sổ
// ---------------------------------------------------------------------------

const ID_OK: i32 = 1;
const ID_CANCEL: i32 = 2;
const ID_ACCOUNT: i32 = 101;
const ID_KEYID: i32 = 102;
const ID_SECRET: i32 = 103;
const ID_BUCKET: i32 = 104;
const ID_URL: i32 = 105;
const ID_TEST: i32 = 106;
const ID_EXPIRY: i32 = 107;
const ID_DIR: i32 = 108;
const ID_BROWSE: i32 = 109;
const ID_AUTOSTART: i32 = 111;
const ID_LANGUAGE: i32 = 112;
const ID_TABS: i32 = 113;
const ID_HELP: i32 = 114;
const ID_NOTICES: i32 = 115;
const ID_KEEP_SELECTION: i32 = 116;
const ID_INCLUDE_CURSOR: i32 = 117;
/// Tab Định dạng: hai nút chọn PNG/JPEG, thanh trượt chất lượng và ô số cạnh nó.
const ID_FMT_PNG: i32 = 118;
const ID_FMT_JPEG: i32 = 119;
const ID_QUALITY: i32 = 140;
const ID_QUALITY_VALUE: i32 = 141;
/// Ghi chú chữ phụ: dòng "đã lưu khóa" dưới ô Secret và dòng "ảnh lưu vào máy luôn là PNG".
const ID_SECRET_NOTE: i32 = 150;
const ID_PNG_NOTE: i32 = 151;
const ID_CHECK_UPDATES: i32 = 152;
/// Hộp tích "Bật lịch sử ảnh chụp" và dòng ghi chú (chữ phụ, tự xuống dòng) ngay dưới nó.
const ID_HISTORY: i32 = 153;
const ID_HISTORY_NOTE: i32 = 154;
/// Hộp tích "Gợi ý cửa sổ khi chọn vùng" (ngay dưới "Chụp cả con trỏ").
const ID_SNAP_WINDOWS: i32 = 155;
/// `TBM_GETPOS` (= `WM_USER`, commctrl.h); crate `windows` không có hằng này.
const TBM_GETPOS: u32 = 0x0400;
/// Nút "Đổi…" của dòng phím tắt thứ i có ID `ID_HK_CHANGE0 + i`; ô tích của dòng là `ID_HK_CHECK0 + i`.
const ID_HK_CHANGE0: i32 = 130;
const ID_HK_CHECK0: i32 = 134;
/// Khung của tab thứ i có ID `ID_FRAME0 + i` (120 đến 123).
const ID_FRAME0: i32 = 120;
const FRAME_CLASS: PCWSTR = w!("darkshot-settings-frame");

const fn ws(v: i32) -> WINDOW_STYLE {
    WINDOW_STYLE(v as u32)
}

const TEST_TIMER_ID: usize = 1;
/// Thời gian chờ tối đa của `test_connection` (60 s) cộng dư.
const TEST_FAILSAFE_MS: u32 = 70_000;
const WM_APP_TEST_DONE: u32 = WM_APP + 1;
const VK_CONTROL_U: i32 = VK_CONTROL.0 as i32;

#[derive(Clone, Copy)]
struct Ctl {
    tabs: HWND,
    /// Khung của từng tab, theo `Tab::index`.
    frames: [HWND; 4],
    language: HWND,
    fmt_png: HWND,
    fmt_jpeg: HWND,
    quality_label: HWND,
    quality: HWND,
    quality_value: HWND,
    account: HWND,
    keyid: HWND,
    secret: HWND,
    bucket: HWND,
    url: HWND,
    test_btn: HWND,
    test_res: HWND,
    expiry: HWND,
    dir: HWND,
    /// Ô tích, ô chỉ đọc và nút "Đổi…" của từng dòng phím tắt, theo `HotkeyRole::ALL`.
    hk_chk: [HWND; HotkeyRole::ALL.len()],
    hk_edit: [HWND; HotkeyRole::ALL.len()],
    hk_btn: [HWND; HotkeyRole::ALL.len()],
    autostart: HWND,
    notices: HWND,
    check_updates: HWND,
    history: HWND,
    keep_selection: HWND,
    include_cursor: HWND,
    snap_windows: HWND,
    err: HWND,
}

/// Vị trí của dòng `r` trong `HotkeyRole::ALL` (chỉ số của các mảng `hk_*` trong `Ctl`).
fn row(r: HotkeyRole) -> usize {
    HotkeyRole::ALL.iter().position(|x| *x == r).unwrap_or(0)
}

struct St {
    current: Config,
    parent: HWND,
    /// Ba dòng phím tắt đang hiện trên biểu mẫu (bật/tắt theo ô tích).
    hotkeys: Hotkeys,
    /// Dòng đang ở chế độ bắt phím; mỗi lúc tối đa một dòng.
    capturing: Option<HotkeyRole>,
    cap: CaptureState,
    /// Các phím tắt cũ đã bị hủy đăng ký (do "Đổi…" hoặc lần OK lỗi) và cần đăng ký lại nếu Hủy.
    hk_unregistered: bool,
    done: bool,
    result: Option<Config>,
    font: Option<HFONT>,
    /// Tab đang hiện.
    tab: Tab,
}

thread_local! {
    /// Một lần Kiểm tra kết nối đang chạy ở luồng nền (nút đang bị làm mờ).
    static TEST_RUNNING: Cell<bool> = const { Cell::new(false) };
    /// HWND của cửa sổ đang mở (0 = không có).
    static OPEN: Cell<isize> = const { Cell::new(0) };
    static CTL: Cell<Option<Ctl>> = const { Cell::new(None) };
    static ST: RefCell<Option<St>> = const { RefCell::new(None) };
    /// Các dòng không đăng ký lại được ở lượt Hủy vừa rồi; bên gọi lấy bằng `take_cancel_failures`.
    static CANCEL_FAILED: RefCell<Vec<HotkeyRole>> = const { RefCell::new(Vec::new()) };
    /// DPI lúc mở cửa sổ (bố cục và mọi kích thước vẽ tay theo nó).
    static DPI: Cell<u32> = const { Cell::new(96) };
    /// Phông đậm của tab đang chọn và nút OK; tạo khi mở, xóa ở WM_NCDESTROY của cửa sổ.
    static BOLD: Cell<Option<HFONT>> = const { Cell::new(None) };
    /// Tab đang được trỏ chuột (-1 = không có).
    static TAB_HOT: Cell<i32> = const { Cell::new(-1) };
    /// Màu của dòng kết quả Kiểm tra kết nối.
    static TEST_TONE: Cell<Tone> = const { Cell::new(Tone::Neutral) };
}

/// Sắc thái của dòng kết quả Kiểm tra kết nối.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tone {
    Neutral,
    Ok,
    Fail,
}

/// Thế hệ cửa sổ: kết quả của luồng nền thuộc cửa sổ đã đóng sẽ bị bỏ.
static GENERATION: AtomicU64 = AtomicU64::new(0);
/// (thế hệ, thành công, câu báo) của lần Kiểm tra kết nối vừa xong.
static TEST_RESULT: Mutex<Option<(u64, bool, String)>> = Mutex::new(None);

fn with_st<R>(f: impl FnOnce(&mut St) -> R) -> Option<R> {
    ST.with(|s| s.borrow_mut().as_mut().map(f))
}

fn ctl() -> Option<Ctl> {
    CTL.with(|c| c.get())
}

fn get_text(h: HWND) -> String {
    // SAFETY: h là cửa sổ con của luồng này; bộ đệm đủ chỗ cho chuỗi cộng ký tự kết thúc.
    unsafe {
        let n = GetWindowTextLengthW(h);
        if n <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; n as usize + 1];
        let got = GetWindowTextW(h, &mut buf).max(0) as usize;
        let s = String::from_utf16_lossy(&buf[..got.min(buf.len())]);
        buf.fill(0);
        s
    }
}

fn set_text(h: HWND, s: &str) {
    // SAFETY: HSTRING sống suốt lời gọi.
    unsafe {
        let _ = SetWindowTextW(h, &HSTRING::from(s));
    }
}

fn read_form() -> Option<Form> {
    let c = ctl()?;
    // SAFETY: các HWND là điều khiển của cửa sổ đang mở ở luồng này.
    let (sel, lang_sel) = unsafe {
        (
            SendMessageW(c.expiry, CB_GETCURSEL, None, None).0,
            SendMessageW(c.language, CB_GETCURSEL, None, None).0,
        )
    };
    let expiry = usize::try_from(sel)
        .ok()
        .and_then(|i| Expiry::ALL.get(i).copied())
        .unwrap_or_default();
    let (mut hotkeys, old_lang) = with_st(|s| (s.hotkeys, s.current.language))?;
    let upload_format = if is_checked(c.fmt_jpeg) {
        UploadFormat::Jpeg
    } else {
        UploadFormat::Png
    };
    // Ô tích là nguồn đúng của trạng thái bật/tắt.
    for r in HotkeyRole::ALL {
        let mut slot = hotkeys.get(r);
        slot.enabled = is_checked(c.hk_chk[row(r)]);
        hotkeys.set(r, slot);
    }
    let language = usize::try_from(lang_sel)
        .ok()
        .and_then(|i| Lang::ALL.get(i).copied())
        .unwrap_or(old_lang);
    Some(Form {
        account_id: get_text(c.account),
        access_key_id: get_text(c.keyid),
        secret_access_key: get_text(c.secret),
        bucket: get_text(c.bucket),
        public_base_url: get_text(c.url),
        expiry,
        hotkeys,
        save_dir: get_text(c.dir),
        autostart: is_checked(c.autostart),
        language,
        show_success_notices: is_checked(c.notices),
        check_updates: is_checked(c.check_updates),
        keep_selection: is_checked(c.keep_selection),
        include_cursor: is_checked(c.include_cursor),
        upload_format,
        // Đọc cả khi đang chọn PNG: giá trị được giữ cho lần chọn lại JPEG.
        jpeg_quality: trackbar_pos(c.quality),
        history_enabled: is_checked(c.history),
        snap_windows: is_checked(c.snap_windows),
    })
}

/// Vị trí thanh trượt chất lượng, kẹp vào khoảng JPEG cho phép.
fn trackbar_pos(h: HWND) -> u8 {
    // SAFETY: h là thanh trượt của cửa sổ đang mở ở luồng này; chỉ đọc vị trí.
    let pos = unsafe { SendMessageW(h, TBM_GETPOS, None, None).0 };
    clamp_jpeg_quality(pos as i64)
}

/// Ô số cạnh thanh trượt hiện vị trí hiện tại.
fn refresh_quality_text(c: &Ctl) {
    set_text(c.quality_value, &trackbar_pos(c.quality).to_string());
}

/// Nhãn, thanh trượt và ô số chất lượng sáng khi chọn JPEG, mờ khi chọn PNG; giá trị giữ nguyên.
fn apply_quality_enabled(c: &Ctl) {
    let format = if is_checked(c.fmt_jpeg) {
        UploadFormat::Jpeg
    } else {
        UploadFormat::Png
    };
    let on = quality_enabled(format);
    // SAFETY: ba điều khiển thuộc cửa sổ đang mở ở luồng này.
    unsafe {
        for h in [c.quality_label, c.quality, c.quality_value] {
            let _ = EnableWindow(h, on);
        }
    }
}

fn is_checked(h: HWND) -> bool {
    // SAFETY: h là ô tích của cửa sổ đang mở ở luồng này; chỉ đọc trạng thái.
    unsafe { SendMessageW(h, BM_GETCHECK, None, None).0 == 1 }
}

fn set_checked(h: HWND, on: bool) {
    // SAFETY: h là ô tích của cửa sổ đang mở ở luồng này.
    unsafe {
        SendMessageW(h, BM_SETCHECK, Some(WPARAM(usize::from(on))), None);
    }
}

/// Ô đầu tiên (theo thứ tự Tab) của mỗi tab.
fn first_ctl(c: &Ctl, t: Tab) -> HWND {
    match t {
        Tab::General => c.autostart,
        Tab::Hotkeys => c.hk_chk[0],
        // Nút chọn đang được chọn: đặt tiêu điểm vào nút kia sẽ tự chọn nó (BS_AUTORADIOBUTTON).
        Tab::Format => {
            if is_checked(c.fmt_jpeg) {
                c.fmt_jpeg
            } else {
                c.fmt_png
            }
        }
        Tab::R2 => c.account,
    }
}

fn field_ctl(c: &Ctl, f: Field) -> HWND {
    match f {
        Field::SaveDir => c.dir,
        Field::HotkeyRow(r) => c.hk_btn[row(r)],
        Field::AccountId => c.account,
        Field::Bucket => c.bucket,
        Field::PublicUrl => c.url,
    }
}

/// Hiện khung của tab `t`, ẩn và tắt các khung còn lại. Tiêu điểm đang nằm trong khung sắp ẩn
/// thì chuyển sang ô đầu của khung mới trước khi ẩn, nên ô của khung ẩn không bao giờ giữ tiêu điểm.
/// Không gửi thông điệp tới dải tab (bên gọi tự đặt lựa chọn của dải tab).
fn show_frame(c: &Ctl, t: Tab) {
    with_st(|s| s.tab = t);
    let shown = c.frames[t.index()];
    // SAFETY: các HWND là cửa sổ con của cửa sổ Tùy chọn đang mở ở luồng này.
    unsafe {
        let _ = EnableWindow(shown, true);
        let _ = ShowWindow(shown, SW_SHOW);
        let focus = GetFocus();
        let in_hidden = !focus.is_invalid()
            && c.frames
                .iter()
                .any(|f| *f != shown && (focus == *f || IsChild(*f, focus).as_bool()));
        if in_hidden {
            let _ = SetFocus(Some(first_ctl(c, t)));
        }
        for f in c.frames.iter().filter(|f| **f != shown) {
            let _ = ShowWindow(*f, SW_HIDE);
            let _ = EnableWindow(*f, false);
        }
    }
}

/// Chuyển tab bằng mã (Ctrl+Tab, lỗi khi OK): thoát chế độ bắt phím, đóng hộp chọn đang thả,
/// đặt lựa chọn của dải tab (TCM_SETCURSEL không sinh TCN_SELCHANGE) rồi hiện khung.
fn select_tab(t: Tab) {
    let Some(c) = ctl() else { return };
    end_capture();
    // SAFETY: gửi thông điệp tới các điều khiển của cửa sổ đang mở ở luồng này.
    unsafe {
        for combo in [c.expiry, c.language] {
            SendMessageW(combo, CB_SHOWDROPDOWN, Some(WPARAM(0)), None);
        }
        SendMessageW(c.tabs, TCM_SETCURSEL, Some(WPARAM(t.index())), None);
    }
    show_frame(&c, t);
}

/// Ctrl+Tab (`step` = 1) hoặc Ctrl+Shift+Tab (`step` = số tab - 1).
fn cycle_tab(step: usize) {
    if let Some(cur) = with_st(|s| s.tab) {
        select_tab(cur.cycle(step));
    }
}

/// Thủ tục cửa sổ của khung: ô con báo WM_COMMAND, WM_CTLCOLOR*, WM_NOTIFY (vẽ tự chọn) và WM_HSCROLL
/// (thanh trượt) cho khung (cha trực tiếp), nên khung chuyển nguyên chúng lên cửa sổ chính.
/// Nền khung là `SURFACE`.
unsafe extern "system" fn frame_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND | WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX | WM_NOTIFY
        | WM_HSCROLL => {
            // SAFETY: cha của khung là cửa sổ chính cùng luồng; SendMessageW gọi thẳng thủ tục của nó.
            unsafe {
                match GetParent(hwnd) {
                    Ok(p) => SendMessageW(p, msg, Some(wp), Some(lp)),
                    Err(_) => DefWindowProcW(hwnd, msg, wp, lp),
                }
            }
        }
        WM_ERASEBKGND | WM_PRINTCLIENT => {
            let hdc = HDC(wp.0 as *mut c_void);
            let mut rc = RECT::default();
            // SAFETY: hdc do hệ thống (hoặc nút vẽ nền cha) cấp cho lần vẽ này; rc là biến cục bộ.
            unsafe {
                let _ = GetClientRect(hwnd, &mut rc);
                FillRect(hdc, &rc, theme::brush(SURFACE));
            }
            LRESULT(1)
        }
        // SAFETY: chuyển nguyên tham số cho xử lý mặc định.
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

// ---------------------------------------------------------------------------
// Vẽ tối: dải tab, viền ô nhập, hộp chọn, nhãn mờ (subclass của điều khiển gốc)
// ---------------------------------------------------------------------------

/// Mã subclass dùng chung (mỗi điều khiển chỉ có một subclass của module này).
const SUBCLASS_ID: usize = 0xDA5;

fn dpi() -> u32 {
    DPI.with(|d| d.get())
}

fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// Nền dưới một điều khiển: `BG` nếu nó nằm thẳng trên cửa sổ chính, `SURFACE` nếu nằm trong khung tab.
fn bg_under(ctrl: HWND) -> Rgb {
    // SAFETY: chỉ đọc quan hệ cửa sổ.
    let parent = unsafe { GetParent(ctrl) }.unwrap_or_default();
    let main = HWND(OPEN.with(|o| o.get()) as *mut c_void);
    if parent == main {
        BG
    } else {
        SURFACE
    }
}

/// `true` khi `edit` là ô phím của dòng đang chờ bắt phím. Không mượn được trạng thái (đang được dùng)
/// thì coi như không.
fn is_capturing_edit(edit: HWND) -> bool {
    let row_now = ST
        .with(|s| {
            s.try_borrow()
                .ok()
                .and_then(|st| st.as_ref().map(|st| st.capturing))
        })
        .flatten();
    match (row_now, ctl()) {
        (Some(r), Some(c)) => c.hk_edit[row(r)] == edit,
        _ => false,
    }
}

/// Gắn subclass `proc` cho `hwnd`; lỗi thì điều khiển giữ kiểu vẽ gốc.
fn subclass(
    hwnd: HWND,
    proc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM, usize, usize) -> LRESULT,
) {
    // SAFETY: hwnd là điều khiển của luồng này; subclass được gỡ ở WM_NCDESTROY của chính nó.
    unsafe {
        let _ = SetWindowSubclass(hwnd, Some(proc), SUBCLASS_ID, 0);
    }
}

/// Phần chung của các subclass: gỡ ở WM_NCDESTROY rồi chuyển tiếp.
unsafe fn subclass_default(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    proc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM, usize, usize) -> LRESULT,
) -> LRESULT {
    // SAFETY: gọi trong thủ tục subclass của chính hwnd, cùng tham số hệ thống đã cấp.
    unsafe {
        if msg == WM_NCDESTROY {
            let _ = RemoveWindowSubclass(hwnd, Some(proc), SUBCLASS_ID);
        }
        DefSubclassProc(hwnd, msg, wp, lp)
    }
}

/// Vẽ lại ngay vùng client của `hwnd` (đè lên phần điều khiển gốc vừa tự vẽ ngoài WM_PAINT).
fn repaint(hwnd: HWND) {
    // SAFETY: chỉ yêu cầu vẽ lại cửa sổ của luồng này.
    unsafe {
        let _ = RedrawWindow(Some(hwnd), None, None, RDW_INVALIDATE | RDW_UPDATENOW);
    }
}

/// Dải tab: không có hộp quanh tab; tab chọn chữ `TEXT` đậm, gạch dưới `ACCENT` 3 DIP bo hai góc trên;
/// tab khác chữ `TEXT_MUTED`; trỏ chuột nền `TAB_HOVER`; đường kẻ `LINE` dưới dải; phần dưới `SURFACE`.
unsafe extern "system" fn tab_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint_tabs(hwnd);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEMOVE => {
            let pt = POINT {
                x: (lp.0 & 0xFFFF) as i16 as i32,
                y: ((lp.0 >> 16) & 0xFFFF) as i16 as i32,
            };
            let mut hit = TCHITTESTINFO {
                pt,
                ..Default::default()
            };
            let mut tme = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            // SAFETY: hit và tme là biến cục bộ sống suốt lời gọi; hwnd là dải tab của luồng này.
            let idx = unsafe {
                let _ = TrackMouseEvent(&mut tme);
                SendMessageW(
                    hwnd,
                    TCM_HITTEST,
                    None,
                    Some(LPARAM(&mut hit as *mut TCHITTESTINFO as isize)),
                )
                .0 as i32
            };
            if TAB_HOT.with(|h| h.replace(idx)) != idx {
                // SAFETY: vẽ lại dải tab của luồng này.
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            // SAFETY: chuyển tiếp trong thủ tục subclass.
            unsafe { subclass_default(hwnd, msg, wp, lp, tab_proc) }
        }
        WM_MOUSELEAVE => {
            if TAB_HOT.with(|h| h.replace(-1)) != -1 {
                // SAFETY: vẽ lại dải tab của luồng này.
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            // SAFETY: chuyển tiếp trong thủ tục subclass.
            unsafe { subclass_default(hwnd, msg, wp, lp, tab_proc) }
        }
        WM_SETFOCUS | WM_KILLFOCUS | WM_UPDATEUISTATE | WM_ENABLE => {
            // SAFETY: chuyển tiếp rồi vẽ lại (khung tiêu điểm theo tiêu điểm và gợi ý bàn phím).
            unsafe {
                let r = subclass_default(hwnd, msg, wp, lp, tab_proc);
                let _ = InvalidateRect(Some(hwnd), None, false);
                r
            }
        }
        // SAFETY: chuyển tiếp trong thủ tục subclass.
        _ => unsafe { subclass_default(hwnd, msg, wp, lp, tab_proc) },
    }
}

/// Đỉnh của khung tab đầu tiên, theo tọa độ client của dải tab `tabs`.
fn frame_top_in(tabs: HWND) -> Option<i32> {
    let f = ctl()?.frames[0];
    let mut wr = RECT::default();
    let mut pt = POINT::default();
    // SAFETY: chỉ đọc vị trí cửa sổ; POINT và RECT là biến cục bộ.
    unsafe {
        GetWindowRect(f, &mut wr).ok()?;
        pt.y = wr.top;
        ScreenToClient(tabs, &mut pt).ok().ok()?;
    }
    Some(pt.y)
}

fn paint_tabs(hwnd: HWND) {
    let dpi = dpi();
    let mut ps = PAINTSTRUCT::default();
    let mut rc = RECT::default();
    let mut first = RECT::default();
    // SAFETY: BeginPaint/EndPaint cân bằng trong hàm này; RECT là biến cục bộ.
    let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
    if hdc.is_invalid() {
        return;
    }
    // SAFETY: hỏi dải tab của luồng này; RECT là biến cục bộ.
    let (count, cur, focus_i, has_focus) = unsafe {
        let _ = GetClientRect(hwnd, &mut rc);
        SendMessageW(
            hwnd,
            TCM_GETITEMRECT,
            Some(WPARAM(0)),
            Some(LPARAM(&mut first as *mut RECT as isize)),
        );
        (
            SendMessageW(hwnd, TCM_GETITEMCOUNT, None, None).0,
            SendMessageW(hwnd, TCM_GETCURSEL, None, None).0,
            SendMessageW(hwnd, TCM_GETCURFOCUS, None, None).0,
            GetFocus() == hwnd,
        )
    };
    // Khung tab (cửa sổ anh em nằm trên dải tab) phủ từ đỉnh của nó trở xuống: đường kẻ và gạch dưới phải
    // nằm ngay trên đỉnh đó.
    let item_bottom = if first.bottom > rc.top {
        first.bottom
    } else {
        rc.top + scale(28, dpi)
    };
    let strip_bottom = frame_top_in(hwnd)
        .map_or(item_bottom, |t| item_bottom.min(t - 1))
        .max(rc.top);
    let cues = theme::ui_cues(hwnd);
    let font = theme::control_font(hwnd);
    let bold = BOLD.with(|b| b.get()).or(font);
    let hot = TAB_HOT.with(|h| h.get()) as isize;
    theme::buffered(hdc, &rc, |dc| {
        theme::fill(
            dc,
            &RECT {
                bottom: strip_bottom,
                ..rc
            },
            BG,
        );
        theme::fill(
            dc,
            &RECT {
                top: strip_bottom,
                bottom: strip_bottom + 1,
                ..rc
            },
            LINE,
        );
        theme::fill(
            dc,
            &RECT {
                top: strip_bottom + 1,
                ..rc
            },
            SURFACE,
        );
        for (i, tab) in Tab::ALL.iter().enumerate().take(count.max(0) as usize) {
            let mut r = RECT::default();
            // SAFETY: r là biến cục bộ, dải tab chỉ ghi vào nó.
            unsafe {
                SendMessageW(
                    hwnd,
                    TCM_GETITEMRECT,
                    Some(WPARAM(i)),
                    Some(LPARAM(&mut r as *mut RECT as isize)),
                );
            }
            // Ô của tab dừng ở đường kẻ; chữ nằm giữa phần trên gạch dưới.
            r.bottom = strip_bottom;
            let text_rc = RECT {
                bottom: strip_bottom - scale(3, dpi),
                ..r
            };
            let selected = i as isize == cur;
            let c = theme::tab_colors(selected, i as isize == hot);
            if i as isize == hot {
                theme::round_top_rect(dc, &r, scale(6, dpi), c.fill);
            }
            theme::draw_text(
                dc,
                &utf16(lang::tr(tab.label())),
                &text_rc,
                c.text,
                if selected { bold } else { font },
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | theme::prefix_flag(cues.1),
            );
            if selected {
                let inset = scale(10, dpi);
                let ul = RECT {
                    left: r.left + inset,
                    right: r.right - inset,
                    top: strip_bottom + 1 - scale(3, dpi),
                    bottom: strip_bottom + 1,
                };
                theme::round_top_rect(dc, &ul, scale(3, dpi), c.border);
            }
            if has_focus && cues.0 && i as isize == focus_i {
                let k = scale(2, dpi);
                let fr = RECT {
                    left: r.left + k,
                    top: r.top + k,
                    right: r.right - k,
                    bottom: strip_bottom - scale(3, dpi),
                };
                theme::focus_frame(dc, &fr, dpi);
            }
        }
    });
    // SAFETY: kết thúc lần vẽ đã mở ở trên.
    unsafe {
        let _ = EndPaint(hwnd, &ps);
    }
}

/// Độ dày (px) phần viền tự vẽ quanh ô nhập: đủ cho viền 1 DIP và gạch dưới 2 DIP (ở 150%: 3 px,
/// viền 2 px, gạch dưới 3 px), cộng phần trước đây là khung vuông riêng của ô `WS_BORDER`. Ô nhập không
/// còn `WS_BORDER` (nó tự vẽ khung vuông 1 px trong vùng client, chồng lên viền bo) và lùi vùng chữ ít
/// hơn đúng `SM_CXBORDER + 1` px, nên vùng ngoài client nhận thêm chừng ấy để chữ giữ nguyên chỗ.
fn edit_inset() -> i32 {
    // SAFETY: chỉ đọc một số đo hệ thống.
    let old_frame = unsafe { GetSystemMetrics(SM_CXBORDER) }.max(0) + 1;
    scale(2, dpi()) + old_frame
}

/// Ô nhập (kể cả ô bí mật và ô phím): viền tự vẽ ở vùng ngoài client (nền `INPUT`, viền `INPUT_BORDER`
/// bo 4 DIP; có tiêu điểm hoặc đang chờ bắt phím thì gạch dưới `ACCENT` 2 DIP). Ô bị vô hiệu tự vẽ chữ
/// `TEXT_DISABLED` (hệ thống luôn dùng màu xám riêng cho ô nhập bị vô hiệu).
unsafe extern "system" fn edit_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    match msg {
        WM_NCCALCSIZE => {
            let k = edit_inset();
            let deflate = |r: &mut RECT| {
                r.left += k;
                r.top += k;
                r.right = (r.right - k).max(r.left);
                r.bottom = (r.bottom - k).max(r.top);
            };
            // SAFETY: với WM_NCCALCSIZE, lp trỏ tới NCCALCSIZE_PARAMS (wp khác 0) hoặc RECT (wp = 0).
            unsafe {
                if wp.0 != 0 {
                    if let Some(p) = (lp.0 as *mut NCCALCSIZE_PARAMS).as_mut() {
                        deflate(&mut p.rgrc[0]);
                    }
                } else if let Some(r) = (lp.0 as *mut RECT).as_mut() {
                    deflate(r);
                }
            }
            LRESULT(0)
        }
        WM_NCPAINT => {
            paint_edit_frame(hwnd);
            LRESULT(0)
        }
        // Ô bí mật không bao giờ tự vẽ (không đọc chữ thật của nó): để Windows vẽ ký tự che.
        // SAFETY: chỉ đọc trạng thái bật/tắt của ô.
        WM_PAINT if !unsafe { IsWindowEnabled(hwnd) }.as_bool() && !is_password(hwnd) => {
            paint_disabled_edit(hwnd);
            LRESULT(0)
        }
        WM_SETFOCUS | WM_KILLFOCUS | WM_ENABLE => {
            // SAFETY: chuyển tiếp rồi vẽ lại viền theo tiêu điểm mới.
            let r = unsafe { subclass_default(hwnd, msg, wp, lp, edit_proc) };
            paint_edit_frame(hwnd);
            r
        }
        // SAFETY: chuyển tiếp trong thủ tục subclass.
        _ => unsafe { subclass_default(hwnd, msg, wp, lp, edit_proc) },
    }
}

/// Vẽ viền tự vẽ của ô nhập `hwnd` (chỉ vùng ngoài client): một viền bo mịn duy nhất.
fn paint_edit_frame(hwnd: HWND) {
    let dpi = dpi();
    let k = edit_inset();
    let mut wr = RECT::default();
    // SAFETY: DC cửa sổ lấy và trả trong khối này; chỉ vẽ ngoài vùng client (đã loại khỏi vùng cắt).
    unsafe {
        if GetWindowRect(hwnd, &mut wr).is_err() {
            return;
        }
        let (w, h) = (wr.right - wr.left, wr.bottom - wr.top);
        let dc = GetWindowDC(Some(hwnd));
        if dc.is_invalid() {
            return;
        }
        ExcludeClipRect(dc, k, k, w - k, h - k);
        let enabled = IsWindowEnabled(hwnd).as_bool();
        let focused = GetFocus() == hwnd || is_capturing_edit(hwnd);
        let all = RECT {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        };
        theme::buffered(dc, &all, |m| {
            theme::fill(m, &all, SURFACE);
            theme::draw_field(m, &all, dpi, theme::edit_border(focused, enabled));
        });
        ReleaseDC(Some(hwnd), dc);
    }
}

/// Vùng client của ô nhập bị vô hiệu: nền `INPUT`, chữ `TEXT_DISABLED` ở đúng vùng chữ của ô.
/// `true` khi `hwnd` là ô nhập có `ES_PASSWORD`.
fn is_password(hwnd: HWND) -> bool {
    // SAFETY: chỉ đọc kiểu cửa sổ.
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } as u32;
    style & ES_PASSWORD as u32 != 0
}

/// Không bao giờ dùng cho ô bí mật (`edit_proc` đã loại), và tự kiểm lại để không đọc chữ thật của nó.
fn paint_disabled_edit(hwnd: HWND) {
    if is_password(hwnd) {
        return;
    }
    let mut ps = PAINTSTRUCT::default();
    let mut rc = RECT::default();
    let mut text_rc = RECT::default();
    // SAFETY: BeginPaint/EndPaint cân bằng; RECT là biến cục bộ, ô chỉ ghi vào chúng.
    unsafe {
        let hdc = BeginPaint(hwnd, &mut ps);
        if hdc.is_invalid() {
            return;
        }
        let _ = GetClientRect(hwnd, &mut rc);
        SendMessageW(
            hwnd,
            EM_GETRECT,
            None,
            Some(LPARAM(&mut text_rc as *mut RECT as isize)),
        );
        let text = theme::window_text(hwnd);
        let font = theme::control_font(hwnd);
        theme::buffered(hdc, &rc, |dc| {
            theme::fill(dc, &rc, INPUT);
            theme::draw_text(
                dc,
                &text,
                &text_rc,
                TEXT_DISABLED,
                font,
                DT_LEFT | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS,
            );
        });
        let _ = EndPaint(hwnd, &ps);
    }
}

/// Hộp chọn: mặt tự vẽ (nền `INPUT`, viền như ô nhập, chữ mục đang chọn, mũi tên chữ phụ). Hộp gốc có
/// lúc tự vẽ mục đang chọn ngoài WM_PAINT (đổi lựa chọn, tiêu điểm), nên sau các thông điệp đó vẽ lại ngay.
unsafe extern "system" fn combo_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint_combo(hwnd);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_SETFOCUS | WM_KILLFOCUS | WM_ENABLE | WM_KEYDOWN | WM_CHAR | WM_LBUTTONDOWN
        | WM_LBUTTONUP | WM_MOUSEWHEEL | WM_COMMAND | WM_UPDATEUISTATE | CB_SETCURSEL
        | CB_SHOWDROPDOWN => {
            // SAFETY: chuyển tiếp rồi vẽ lại mặt hộp.
            let r = unsafe { subclass_default(hwnd, msg, wp, lp, combo_proc) };
            repaint(hwnd);
            r
        }
        // SAFETY: chuyển tiếp trong thủ tục subclass.
        _ => unsafe { subclass_default(hwnd, msg, wp, lp, combo_proc) },
    }
}

fn paint_combo(hwnd: HWND) {
    let dpi = dpi();
    let mut ps = PAINTSTRUCT::default();
    let mut rc = RECT::default();
    // SAFETY: BeginPaint/EndPaint cân bằng; chỉ hỏi trạng thái và chữ của hộp chọn; bộ đệm đủ chỗ.
    unsafe {
        let hdc = BeginPaint(hwnd, &mut ps);
        if hdc.is_invalid() {
            return;
        }
        let _ = GetClientRect(hwnd, &mut rc);
        let enabled = IsWindowEnabled(hwnd).as_bool();
        let dropped = SendMessageW(hwnd, CB_GETDROPPEDSTATE, None, None).0 != 0;
        let focused = GetFocus() == hwnd || dropped;
        let sel = SendMessageW(hwnd, CB_GETCURSEL, None, None).0;
        let mut text = Vec::new();
        if sel >= 0 {
            let n = SendMessageW(hwnd, CB_GETLBTEXTLEN, Some(WPARAM(sel as usize)), None).0;
            if n > 0 {
                let mut buf = vec![0u16; n as usize + 1];
                let got = SendMessageW(
                    hwnd,
                    CB_GETLBTEXT,
                    Some(WPARAM(sel as usize)),
                    Some(LPARAM(buf.as_mut_ptr() as isize)),
                )
                .0;
                buf.truncate(got.clamp(0, n) as usize);
                text = buf;
            }
        }
        let font = theme::control_font(hwnd);
        theme::buffered(hdc, &rc, |dc| {
            theme::fill(dc, &rc, bg_under(hwnd));
            theme::draw_field(dc, &rc, dpi, theme::edit_border(focused, enabled));
            let tr = RECT {
                left: rc.left + scale(8, dpi),
                right: rc.right - scale(28, dpi),
                ..rc
            };
            let fg = if enabled { TEXT } else { TEXT_DISABLED };
            theme::draw_text(
                dc,
                &text,
                &tr,
                fg,
                font,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS,
            );
            let (cx, cy, a) = (
                rc.right - scale(14, dpi),
                (rc.top + rc.bottom) / 2,
                scale(4, dpi),
            );
            theme::polyline(
                dc,
                &[
                    POINT {
                        x: cx - a,
                        y: cy - a / 2,
                    },
                    POINT {
                        x: cx,
                        y: cy - a / 2 + a,
                    },
                    POINT {
                        x: cx + a + 1,
                        y: cy - a / 2 - 1,
                    },
                ],
                if enabled { TEXT_MUTED } else { TEXT_DISABLED },
                (scale(3, dpi) / 2).max(1),
            );
        });
        let _ = EndPaint(hwnd, &ps);
    }
}

/// Nhãn có thể bị vô hiệu (nhãn và số chất lượng JPEG): tự vẽ để chữ mờ là `TEXT_DISABLED` phẳng thay
/// cho chữ khắc nổi của hệ thống.
unsafe extern "system" fn static_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let mut rc = RECT::default();
            // SAFETY: BeginPaint/EndPaint cân bằng; chỉ đọc chữ, phông và trạng thái của nhãn.
            unsafe {
                let hdc = BeginPaint(hwnd, &mut ps);
                if !hdc.is_invalid() {
                    let _ = GetClientRect(hwnd, &mut rc);
                    let fg = if IsWindowEnabled(hwnd).as_bool() {
                        TEXT
                    } else {
                        TEXT_DISABLED
                    };
                    let text = theme::window_text(hwnd);
                    let font = theme::control_font(hwnd);
                    let bg = bg_under(hwnd);
                    let cues = theme::ui_cues(hwnd);
                    theme::buffered(hdc, &rc, |dc| {
                        theme::fill(dc, &rc, bg);
                        theme::draw_text(
                            dc,
                            &text,
                            &rc,
                            fg,
                            font,
                            DT_LEFT | DT_WORDBREAK | theme::prefix_flag(cues.1),
                        );
                    });
                    let _ = EndPaint(hwnd, &ps);
                }
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_ENABLE => {
            // SAFETY: chuyển tiếp rồi vẽ lại theo trạng thái mới.
            unsafe {
                let r = subclass_default(hwnd, msg, wp, lp, static_proc);
                let _ = InvalidateRect(Some(hwnd), None, false);
                r
            }
        }
        // SAFETY: chuyển tiếp trong thủ tục subclass.
        _ => unsafe { subclass_default(hwnd, msg, wp, lp, static_proc) },
    }
}

/// Gắn phần vẽ tối cho các điều khiển của cửa sổ vừa dựng.
fn apply_dark_controls(c: &Ctl) {
    let dpi = dpi();
    subclass(c.tabs, tab_proc);
    let mut edits = vec![c.account, c.keyid, c.secret, c.bucket, c.url, c.dir];
    edits.extend_from_slice(&c.hk_edit);
    let margin = scale(6, dpi) as usize;
    for e in edits {
        subclass(e, edit_proc);
        // SAFETY: đặt lề chữ và tính lại vùng ngoài client của ô nhập thuộc luồng này.
        unsafe {
            SendMessageW(
                e,
                EM_SETMARGINS,
                Some(WPARAM(0x1 | 0x2)),
                Some(LPARAM((margin | (margin << 16)) as isize)),
            );
            let _ = SetWindowPos(
                e,
                None,
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
    for combo in [c.language, c.expiry] {
        // Danh sách thả xuống tối (thanh cuộn, nền); mặt hộp tự vẽ.
        theme::set_dark_control_theme(combo, theme::DarkClass::Cfd);
        subclass(combo, combo_proc);
    }
    for s in [c.quality_label, c.quality_value] {
        subclass(s, static_proc);
    }
}

/// Màu chữ và nền của điều khiển `from` khi nó hỏi `WM_CTLCOLORSTATIC`.
fn static_colors(from: HWND) -> (Rgb, Rgb) {
    let Some(c) = ctl() else {
        return (TEXT, bg_under(from));
    };
    if from == c.err {
        return (DANGER, BG);
    }
    if from == c.test_res {
        let fg = match TEST_TONE.with(|t| t.get()) {
            Tone::Neutral => TEXT,
            Tone::Ok => SUCCESS,
            Tone::Fail => DANGER,
        };
        return (fg, SURFACE);
    }
    if c.hk_edit.contains(&from) {
        // SAFETY: chỉ đọc trạng thái bật/tắt.
        let fg = if !unsafe { IsWindowEnabled(from) }.as_bool() {
            TEXT_DISABLED
        } else if is_capturing_edit(from) {
            TEXT_MUTED
        } else {
            TEXT
        };
        return (fg, INPUT);
    }
    // SAFETY: chỉ đọc mã điều khiển.
    let id = unsafe { GetDlgCtrlID(from) };
    if id == ID_SECRET_NOTE || id == ID_PNG_NOTE || id == ID_HISTORY_NOTE {
        return (TEXT_MUTED, SURFACE);
    }
    (TEXT, bg_under(from))
}

/// Đặt câu kết quả Kiểm tra kết nối cùng sắc thái màu của nó.
fn set_test_result(c: &Ctl, text: &str, tone: Tone) {
    TEST_TONE.with(|t| t.set(tone));
    set_text(c.test_res, text);
}

fn show_error(msg: &str) {
    if let Some(c) = ctl() {
        set_text(c.err, msg);
    }
}

/// Ô chỉ đọc của dòng `r` hiện tổ hợp đang chọn trên biểu mẫu.
fn refresh_hotkey_text(r: HotkeyRole) {
    if let (Some(c), Some(hk)) = (ctl(), with_st(|s| s.hotkeys.get(r).key)) {
        set_text(c.hk_edit[row(r)], &hotkey::describe(&hk));
        // Gạch dưới của ô đang chờ bắt phím tắt đi khi thoát chế độ bắt.
        paint_edit_frame(c.hk_edit[row(r)]);
    }
}

/// Ô phím và nút "Đổi…" của dòng `r` sáng khi dòng bật, mờ khi tắt; tổ hợp giữ nguyên.
fn apply_row_enabled(c: &Ctl, r: HotkeyRole, on: bool) {
    // SAFETY: hai điều khiển thuộc cửa sổ đang mở ở luồng này.
    unsafe {
        let _ = EnableWindow(c.hk_edit[row(r)], on);
        let _ = EnableWindow(c.hk_btn[row(r)], on);
    }
}

/// Người dùng bấm ô tích của dòng `r`.
fn on_toggle_row(r: HotkeyRole) {
    let Some(c) = ctl() else { return };
    let on = is_checked(c.hk_chk[row(r)]);
    // Bỏ tích dòng đang bắt phím: thoát chế độ bắt trước (giữ phím cũ của dòng).
    if !on && with_st(|s| s.capturing == Some(r)).unwrap_or(false) {
        end_capture();
    }
    with_st(|s| {
        let mut slot = s.hotkeys.get(r);
        slot.enabled = on;
        s.hotkeys.set(r, slot);
    });
    apply_row_enabled(&c, r, on);
}

fn finish(result: Option<Config>) {
    with_st(|s| {
        s.result = result;
        s.done = true;
    });
}

fn cancel() {
    with_st(|s| {
        if s.hk_unregistered {
            // Đăng ký lại các dòng đang bật của cấu hình cũ (dòng tắt không đăng ký). Dòng bị chiếm
            // giữa chừng được giữ lại để bên gọi báo bằng thông báo gộp.
            let failed = hotkey::register_enabled(s.parent, &s.current.hotkeys);
            CANCEL_FAILED.with(|f| *f.borrow_mut() = failed);
            s.hk_unregistered = false;
        }
    });
    finish(None);
}

/// Chuyển sang tab chứa ô sai, đặt tiêu điểm vào ô đó và hiện câu lỗi; giá trị ở mọi tab giữ nguyên.
fn show_form_error(e: &FormError) {
    select_tab(tab_of(e.field));
    if let Some(c) = ctl() {
        // SAFETY: ô thuộc khung vừa hiện của cửa sổ đang mở ở luồng này.
        unsafe {
            let _ = SetFocus(Some(field_ctl(&c, e.field)));
        }
    }
    show_error(&e.message);
}

fn save() {
    let Some(form) = read_form() else { return };
    let Some(current) = with_st(|s| s.current.clone()) else {
        return;
    };
    let cfg = match apply_form(&current, &form, lang::current()) {
        Ok(c) => c,
        Err(e) => {
            show_form_error(&e);
            return;
        }
    };
    let Some((parent, need_rereg)) = with_st(|s| {
        (
            s.parent,
            s.hk_unregistered || cfg.hotkeys != s.current.hotkeys,
        )
    }) else {
        return;
    };
    if need_rereg {
        hotkey::unregister_all(parent);
        // Đăng ký lần lượt các dòng đang bật; dòng tắt không bao giờ được đăng ký.
        for r in HotkeyRole::ALL {
            let slot = cfg.hotkeys.get(r);
            if slot.enabled && !hotkey::register(parent, r, &slot.key) {
                // Không nửa vời: hủy mọi dòng đã đăng ký, chờ OK lần sau hoặc Hủy.
                hotkey::unregister_all(parent);
                with_st(|s| s.hk_unregistered = true);
                select_tab(Tab::Hotkeys);
                if let Some(c) = ctl() {
                    // SAFETY: nút "Đổi…" thuộc khung vừa hiện của cửa sổ đang mở ở luồng này.
                    unsafe {
                        let _ = SetFocus(Some(c.hk_btn[row(r)]));
                    }
                }
                let lang = lang::current();
                show_error(&i18n::hotkey_row_taken(
                    lang,
                    t(lang, r.label()),
                    &hotkey::describe(&slot.key),
                ));
                return;
            }
        }
        with_st(|s| s.hk_unregistered = false);
    }
    finish(Some(cfg));
}

fn on_command(hwnd: HWND, id: i32) {
    match id {
        ID_OK => save(),
        ID_CANCEL => cancel(),
        ID_TEST => on_test(hwnd),
        ID_BROWSE => on_browse(hwnd),
        ID_HELP => on_help(),
        ID_FMT_PNG | ID_FMT_JPEG => {
            if let Some(c) = ctl() {
                apply_quality_enabled(&c);
            }
        }
        _ => {
            for (i, r) in HotkeyRole::ALL.into_iter().enumerate() {
                if id == ID_HK_CHANGE0 + i as i32 {
                    on_change_hotkey(r);
                } else if id == ID_HK_CHECK0 + i as i32 {
                    on_toggle_row(r);
                }
            }
        }
    }
}

/// Mở Trợ giúp với các dòng phím tắt đang hiện trong biểu mẫu. Chủ của cửa sổ Trợ giúp là cửa sổ ẩn của
/// ứng dụng, không phải Tùy chọn, nên đóng Tùy chọn không kéo Trợ giúp theo.
fn on_help() {
    // Đang bắt phím mà bấm nút này: thoát chế độ bắt trước, như bấm ra ngoài.
    end_capture();
    // `St.hotkeys` theo sát ô tích (`on_toggle_row`) và phím vừa bắt; không đọc cả biểu mẫu
    // để khóa bí mật không bị chép ra chỉ vì mở Trợ giúp.
    if let Some((parent, keys)) = with_st(|s| (s.parent, s.hotkeys)) {
        crate::help::show(parent, &hotkey::help_keys(&keys));
    }
}

/// Bấm "Đổi…" của dòng `r`: dòng khác đang bắt thì thoát (giữ phím cũ của nó), rồi dòng `r` vào
/// chế độ bắt. Lần đầu trong phiên mở cửa sổ: hủy đăng ký mọi dòng (Hủy sẽ đăng ký lại).
fn on_change_hotkey(r: HotkeyRole) {
    let Some(c) = ctl() else { return };
    if with_st(|s| s.capturing == Some(r)).unwrap_or(true) {
        return;
    }
    end_capture();
    let started = with_st(|s| {
        hotkey::unregister_all(s.parent);
        s.hk_unregistered = true;
        s.capturing = Some(r);
        s.cap = CaptureState::new();
    })
    .is_some();
    if started {
        set_text(c.hk_edit[row(r)], lang::tr(Msg::HotkeyCapturePrompt));
        // Ô đang chờ: gạch dưới ACCENT dù tiêu điểm ở nút "Đổi…".
        paint_edit_frame(c.hk_edit[row(r)]);
        show_error("");
    }
}

/// Thoát chế độ bắt phím, giữ phím tắt đang chọn trong biểu mẫu.
fn end_capture() {
    if let Some(r) = with_st(|s| s.capturing.take()).flatten() {
        refresh_hotkey_text(r);
    }
}

fn key_down(k: i32) -> bool {
    // SAFETY: chỉ đọc trạng thái phím.
    unsafe { GetKeyState(k) < 0 }
}

/// Xử lý thông điệp khi đang chọn phím tắt. Trả về `true` nếu đã nuốt thông điệp.
fn capture_msg(msg: &MSG, hk_btn: HWND) -> bool {
    match msg.message {
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_NCLBUTTONDOWN | WM_NCRBUTTONDOWN => {
            // Bấm ra ngoài nút "Đổi…": thoát chế độ bắt rồi xử lý cú bấm bình thường.
            if msg.hwnd != hk_btn {
                end_capture();
            }
            false
        }
        WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP => {
            let is_down = msg.message == WM_KEYDOWN || msg.message == WM_SYSKEYDOWN;
            let vk = msg.wParam.0 as u32;
            if is_down {
                if vk == VK_ESCAPE.0 as u32 {
                    end_capture();
                    return true;
                }
                // Tự lặp phím (kể cả phím giữ từ trước khi vào chế độ bắt): bỏ qua.
                if (msg.lParam.0 >> 30) & 1 == 1 {
                    return true;
                }
            }
            let (ctrl, alt, shift, win) = (
                key_down(VK_CONTROL_U),
                key_down(VK_MENU.0 as i32),
                key_down(VK_SHIFT.0 as i32),
                key_down(VK_LWIN.0 as i32) || key_down(VK_RWIN.0 as i32),
            );
            let ev = if is_down {
                KeyEv::Down {
                    vk,
                    ctrl,
                    alt,
                    shift,
                    win,
                }
            } else {
                KeyEv::Up {
                    vk,
                    ctrl,
                    alt,
                    shift,
                    win,
                }
            };
            let Some(out) = with_st(|s| s.cap.feed(ev)) else {
                return false;
            };
            match out {
                Capture::Ignore => true,
                Capture::PassThrough => false,
                Capture::NeedModifier => {
                    if let (Some(c), Some(Some(r))) = (ctl(), with_st(|s| s.capturing)) {
                        set_text(c.hk_edit[row(r)], lang::tr(Msg::HotkeyNeedModifier));
                    }
                    true
                }
                Capture::Got(h) => {
                    if let Some(r) = with_st(|s| s.capturing.take()).flatten() {
                        with_st(|s| {
                            let mut slot = s.hotkeys.get(r);
                            slot.key = h;
                            s.hotkeys.set(r, slot);
                        });
                        refresh_hotkey_text(r);
                    }
                    true
                }
            }
        }
        _ => false,
    }
}

fn on_browse(hwnd: HWND) {
    let Some(c) = ctl() else { return };
    // SAFETY: COM được khởi tạo và giải phóng cân bằng trong hàm này; chuỗi do COM cấp được giải phóng bằng CoTaskMemFree.
    let picked: Option<String> = unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let inited = hr.is_ok();
        let r = (|| -> windows::core::Result<String> {
            let dlg: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            let opts = dlg.GetOptions()?;
            dlg.SetOptions(opts | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM)?;
            dlg.SetTitle(&HSTRING::from(lang::tr(Msg::PickFolderTitle)))?;
            dlg.Show(Some(hwnd))?;
            let item = dlg.GetResult()?;
            let name: PWSTR = item.GetDisplayName(SIGDN_FILESYSPATH)?;
            let s = name.to_string();
            CoTaskMemFree(Some(name.0 as *const c_void));
            Ok(s.unwrap_or_default())
        })();
        if inited {
            CoUninitialize();
        }
        r.ok().filter(|s| !s.is_empty())
    };
    if let Some(p) = picked {
        set_text(c.dir, &p);
    }
}

fn on_test(hwnd: HWND) {
    let Some(c) = ctl() else { return };
    let (Some(form), Some(current)) = (read_form(), with_st(|s| s.current.clone())) else {
        return;
    };
    let cfg = match apply_form(&current, &form, lang::current()) {
        Ok(c) => c,
        Err(e) => {
            // Ô sai có thể ở tab khác (ví dụ Thư mục lưu trống): đưa người dùng tới đúng ô.
            show_form_error(&e);
            return;
        }
    };
    show_error("");
    if !cfg.r2.is_complete() {
        set_test_result(&c, lang::tr(Msg::TestFillAll), Tone::Fail);
        return;
    }
    if TEST_RUNNING.with(|r| r.get()) {
        return;
    }
    // Thế hệ mới: kết quả của lần chạy trước (nếu có) sẽ bị bỏ.
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let target = hwnd.0 as isize;
    let r2 = cfg.r2;
    let spawned = std::thread::Builder::new()
        .name("darkshot-test-connection".into())
        .spawn(move || {
            let (ok, msg) = match R2Uploader::new(&r2).test_connection() {
                Ok(()) => (true, lang::tr(Msg::TestSuccess).to_string()),
                Err(e) => (false, e.message(lang::current())),
            };
            if let Ok(mut slot) = TEST_RESULT.lock() {
                // Cửa sổ đã đóng (thế hệ đã đổi): bỏ kết quả, không đè lên kết quả của cửa sổ mới.
                if generation == GENERATION.load(Ordering::SeqCst) {
                    *slot = Some((generation, ok, msg));
                }
            }
            // SAFETY: nếu cửa sổ đã đóng thì PostMessageW trả lỗi và ta bỏ qua; kết quả nằm trong mutex, không có con trỏ treo.
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(target as *mut c_void)),
                    WM_APP_TEST_DONE,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        });
    if spawned.is_err() {
        set_test_result(&c, lang::tr(Msg::TestThreadFailed), Tone::Fail);
        return;
    }
    TEST_RUNNING.with(|r| r.set(true));
    // SAFETY: chuyển focus sang ô còn bật trước khi làm mờ nút; bộ hẹn giờ dự phòng gắn với cửa sổ này.
    unsafe {
        if GetFocus() == c.test_btn {
            let _ = SetFocus(Some(c.account));
        }
        let _ = EnableWindow(c.test_btn, false);
        SetTimer(Some(hwnd), TEST_TIMER_ID, TEST_FAILSAFE_MS, None);
    }
    set_test_result(&c, lang::tr(Msg::TestRunning), Tone::Neutral);
}

/// Bật lại nút "Kiểm tra kết nối" và hủy bộ hẹn giờ dự phòng.
fn test_finished(hwnd: HWND, c: &Ctl) {
    TEST_RUNNING.with(|r| r.set(false));
    // SAFETY: bật lại nút và hủy bộ hẹn giờ của chính cửa sổ này.
    unsafe {
        let _ = KillTimer(Some(hwnd), TEST_TIMER_ID);
        let _ = EnableWindow(c.test_btn, true);
    }
}

fn on_test_timeout(hwnd: HWND) {
    let Some(c) = ctl() else { return };
    if !TEST_RUNNING.with(|r| r.get()) {
        return;
    }
    // Bỏ kết quả muộn của luồng nền.
    GENERATION.fetch_add(1, Ordering::SeqCst);
    set_test_result(&c, lang::tr(Msg::TestTimeout), Tone::Fail);
    test_finished(hwnd, &c);
}

fn on_test_done(hwnd: HWND) {
    let Some(c) = ctl() else { return };
    let taken = TEST_RESULT.lock().ok().and_then(|mut s| s.take());
    let Some((generation, ok, msg)) = taken else {
        return;
    };
    if generation != GENERATION.load(Ordering::SeqCst) {
        return;
    }
    set_test_result(&c, &msg, if ok { Tone::Ok } else { Tone::Fail });
    test_finished(hwnd, &c);
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let code = (wp.0 >> 16) & 0xFFFF;
            if code == 0 {
                on_command(hwnd, (wp.0 & 0xFFFF) as i32);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            cancel();
            LRESULT(0)
        }
        WM_HSCROLL => {
            // Thanh trượt chất lượng (do khung chuyển lên): số đổi theo ngay khi kéo và khi bấm phím.
            let from = HWND(lp.0 as *mut c_void);
            if let Some(c) = ctl() {
                if from == c.quality {
                    refresh_quality_text(&c);
                }
            }
            LRESULT(0)
        }
        WM_APP_TEST_DONE => {
            on_test_done(hwnd);
            LRESULT(0)
        }
        WM_TIMER if wp.0 == TEST_TIMER_ID => {
            on_test_timeout(hwnd);
            LRESULT(0)
        }
        WM_ACTIVATE => {
            // Mất kích hoạt: thoát chế độ bắt phím (rồi để hệ thống xử lý tiếp, giữ focus đúng).
            if (wp.0 & 0xFFFF) as u32 == WA_INACTIVE {
                end_capture();
            }
            // SAFETY: chuyển nguyên tham số cho xử lý mặc định.
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
        WM_NOTIFY => {
            let hdr = lp.0 as *const NMHDR;
            // SAFETY: với WM_NOTIFY, lParam trỏ tới NMHDR hợp lệ suốt lời gọi.
            if unsafe { hdr.as_ref() }.is_some_and(|h| h.code == NM_CUSTOMDRAW) {
                if let Some(r) = custom_draw(lp) {
                    return r;
                }
            }
            // Thông báo của dải tab (con trực tiếp của cửa sổ này).
            if let (Some(c), false) = (ctl(), hdr.is_null()) {
                // SAFETY: với WM_NOTIFY, lParam trỏ tới NMHDR hợp lệ suốt lời gọi.
                let (from, code) = unsafe { ((*hdr).hwndFrom, (*hdr).code) };
                if from == c.tabs && code == TCN_SELCHANGING {
                    // Đổi tab (chuột hoặc mũi tên) lúc đang bắt phím: thoát chế độ bắt trước.
                    end_capture();
                    return LRESULT(0);
                }
                if from == c.tabs && code == TCN_SELCHANGE {
                    // SAFETY: hỏi tab đang chọn của dải tab thuộc cửa sổ này.
                    let sel = unsafe { SendMessageW(c.tabs, TCM_GETCURSEL, None, None).0 };
                    if let Some(t) = usize::try_from(sel).ok().and_then(|i| Tab::ALL.get(i)) {
                        show_frame(&c, *t);
                    }
                    return LRESULT(0);
                }
            }
            // SAFETY: chuyển nguyên tham số cho xử lý mặc định.
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
        WM_CTLCOLORSTATIC => {
            let (fg, bg) = static_colors(HWND(lp.0 as *mut c_void));
            theme::ctl_color(HDC(wp.0 as *mut c_void), fg, bg)
        }
        // Ô nhập sửa được và danh sách thả xuống của hộp chọn: chữ chính trên nền ô nhập.
        WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
            theme::ctl_color(HDC(wp.0 as *mut c_void), TEXT, INPUT)
        }
        WM_NCDESTROY => {
            // Mọi điều khiển đã bị hủy: xóa phông đậm của tab chọn và nút OK.
            if let Some(f) = BOLD.with(|b| b.take()) {
                // SAFETY: phông do CreateFontIndirectW tạo, không còn điều khiển nào dùng.
                unsafe {
                    let _ = DeleteObject(HGDIOBJ(f.0));
                }
            }
            // SAFETY: chuyển nguyên tham số cho xử lý mặc định.
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
        // SAFETY: chuyển nguyên tham số cho xử lý mặc định.
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

/// `NM_CUSTOMDRAW` của nút bấm, ô tích, nút chọn (cửa sổ chính hoặc khung chuyển lên) và thanh trượt.
fn custom_draw(lp: LPARAM) -> Option<LRESULT> {
    // SAFETY: bên gọi chỉ chuyển lp của WM_NOTIFY mang NM_CUSTOMDRAW.
    let cd = unsafe { theme::nmcd_of(lp) }?;
    let from = cd.hdr.hwndFrom;
    let id = cd.hdr.idFrom as i32;
    if id == ID_QUALITY {
        return Some(theme::custom_draw_trackbar(cd, SURFACE, dpi()));
    }
    if !get_class_is_button(from) {
        return None;
    }
    let kind = if id == ID_OK {
        ButtonKind::Primary
    } else {
        ButtonKind::Normal
    };
    Some(theme::custom_draw_button(
        cd,
        kind,
        bg_under(from),
        dpi(),
        BOLD.with(|b| b.get()),
    ))
}

struct Builder {
    parent: HWND,
    dpi: i32,
    font: Option<HFONT>,
    /// Bố cục theo bề rộng (DIP), tính một lần bằng `Layout::measure` trước khi dựng ô.
    layout: Layout,
}

/// Bề rộng (DIP) suy ra từ nhãn dài nhất của tab Phím tắt: cửa sổ nới ra vừa đủ để ô phím của
/// mỗi dòng rộng ít nhất `HK_EDIT_MIN_W`; mọi tab và hàng nút giãn theo `client_w`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Layout {
    /// Bề rộng vùng client của cửa sổ.
    client_w: i32,
    /// Bề rộng ô nhập ở các tab (cột bắt đầu từ `EX`).
    ew: i32,
    /// Bề rộng ô tích mang nhãn của dòng phím tắt.
    hk_chk_w: i32,
}

/// Bề rộng tối thiểu của vùng client (bố cục đợt 1) và của ô phím trên tab Phím tắt.
const CLIENT_MIN_W: i32 = 480;
const HK_EDIT_MIN_W: i32 = 194;
/// Lề phải từ mép ô nhập tới mép vùng client (dải tab, viền khung, lề trong).
const RIGHT_MARGIN: i32 = 50;
/// Ô vuông của ô tích và khoảng cách tới chữ; bề rộng nút "Đổi…"; khe giữa các ô trong một dòng.
const HK_BOX: i32 = 24;
const HK_BTN_W: i32 = 70;
const HK_GAP: i32 = 6;
/// Phần cộng thêm vào bề rộng chữ của mỗi tab (lề trong của mục tab, rộng rãi hơn mức mặc định
/// 2 × 6 của Common Controls) và phần dải tab không dùng cho mục (tab đang chọn nhô ra hai bên).
const TAB_ITEM_PAD: i32 = 24;
const TAB_STRIP_SLACK: i32 = 8;

impl Layout {
    /// `label_w`: bề rộng chữ (DIP) của nhãn dài nhất trong mọi ngôn ngữ, `None` khi không đo được.
    fn from_label_width(label_w: Option<i32>) -> Layout {
        let hk_chk_w = (label_w.unwrap_or(176) + HK_BOX).clamp(142, 240);
        // Mép phải của nút "Đổi…" khi ô phím rộng đúng `HK_EDIT_MIN_W`.
        let need_right = LX + hk_chk_w + HK_GAP + HK_EDIT_MIN_W + HK_GAP + HK_BTN_W;
        let ew = (CLIENT_MIN_W - RIGHT_MARGIN - EX).max(need_right - EX);
        Layout {
            client_w: EX + ew + RIGHT_MARGIN,
            ew,
            hk_chk_w,
        }
    }

    /// Bề rộng dành cho các mục của dải tab.
    /// Dải tab rộng `client_w - 24` (lề 12 mỗi bên); trừ thêm `TAB_STRIP_SLACK` cho phần nhô của tab
    /// đang chọn và lề trong của dải.
    fn tab_strip_w(self) -> i32 {
        self.client_w - 24 - TAB_STRIP_SLACK
    }

    /// Nới `client_w` (và ô nhập theo cùng lượng) khi `tab_widths` không vừa một hàng.
    fn widen_for_tabs(self, tab_widths: &[i32]) -> Layout {
        if tabs_fit(tab_widths, self.tab_strip_w()) {
            return self;
        }
        let total: i32 = tab_widths.iter().sum();
        let extra = total - self.tab_strip_w();
        Layout {
            client_w: self.client_w + extra,
            ew: self.ew + extra,
            hk_chk_w: self.hk_chk_w,
        }
    }
}

impl Builder {
    fn s(&self, v: i32) -> i32 {
        (v * self.dpi + 48) / 96
    }

    #[allow(clippy::too_many_arguments)]
    fn make(
        &mut self,
        class: PCWSTR,
        text: &str,
        style: WINDOW_STYLE,
        (x, y, w, h): (i32, i32, i32, i32),
        id: i32,
    ) -> HWND {
        // SAFETY: tạo cửa sổ con của cửa sổ Tùy chọn ở luồng chính; lỗi thì trả cửa sổ rỗng (các hàm sau chịu được).
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                &HSTRING::from(text),
                WS_CHILD | WS_VISIBLE | style,
                self.s(x),
                self.s(y),
                self.s(w),
                self.s(h),
                Some(self.parent),
                Some(HMENU(id as usize as *mut c_void)),
                None,
                None,
            )
            .unwrap_or_default()
        };
        if let (Some(f), false) = (self.font, hwnd.is_invalid()) {
            // SAFETY: đặt phông cho điều khiển vừa tạo; phông sống đến khi đóng cửa sổ.
            unsafe {
                SendMessageW(
                    hwnd,
                    WM_SETFONT,
                    Some(WPARAM(f.0 as usize)),
                    Some(LPARAM(1)),
                );
            }
        }
        hwnd
    }

    /// Khung của một tab: con của cửa sổ chính, ẩn và tắt lúc tạo (`show_frame` bật khung đang chọn).
    /// `WS_EX_CONTROLPARENT` để Tab/Shift+Tab của hộp thoại đi vào các ô bên trong.
    fn frame(&mut self, id: i32) -> HWND {
        // SAFETY: tạo cửa sổ con của cửa sổ Tùy chọn ở luồng chính; lớp đã đăng ký trong `show`.
        unsafe {
            let hinst = GetModuleHandleW(None).ok();
            CreateWindowExW(
                WS_EX_CONTROLPARENT,
                FRAME_CLASS,
                PCWSTR::null(),
                WS_CHILD | WS_DISABLED | WS_CLIPCHILDREN,
                0,
                0,
                0,
                0,
                Some(self.parent),
                Some(HMENU(id as usize as *mut c_void)),
                hinst.map(|h| h.into()),
                None,
            )
            .unwrap_or_default()
        }
    }

    fn label(&mut self, text: &str, x: i32, y: i32, w: i32, h: i32) -> HWND {
        self.make(w!("STATIC"), text, WINDOW_STYLE(0), (x, y, w, h), -1)
    }

    fn edit(&mut self, style: WINDOW_STYLE, rect: (i32, i32, i32, i32), id: i32) -> HWND {
        self.make(
            w!("EDIT"),
            "",
            WS_GROUP | WS_TABSTOP | ws(ES_AUTOHSCROLL) | style,
            rect,
            id,
        )
    }

    fn button(
        &mut self,
        text: &str,
        style: WINDOW_STYLE,
        rect: (i32, i32, i32, i32),
        id: i32,
    ) -> HWND {
        self.make(w!("BUTTON"), text, WS_GROUP | WS_TABSTOP | style, rect, id)
    }
}

/// Lề trái nhãn, lề trái ô nhập và bề rộng ô nhập trong một khung (DIP, tính từ góc khung).
const LX: i32 = 12;
const EX: i32 = 160;

/// Dòng nhãn + ô nhập một dòng trong khung; trả về ô nhập.
fn row_edit(b: &mut Builder, label: &str, y: i32, w: i32, style: WINDOW_STYLE, id: i32) -> HWND {
    b.label(label, LX, y + 3, EX - LX - 4, 20);
    b.edit(style, (EX, y, w, 24), id)
}

fn combo(b: &mut Builder, y: i32, id: i32) -> HWND {
    b.make(
        w!("COMBOBOX"),
        "",
        WS_GROUP | WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
        (EX, y, 160, 200),
        id,
    )
}

struct GeneralCtl {
    autostart: HWND,
    notices: HWND,
    check_updates: HWND,
    history: HWND,
    keep_selection: HWND,
    include_cursor: HWND,
    snap_windows: HWND,
    dir: HWND,
    language: HWND,
    height: i32,
}

/// Hình chữ nhật `(x, y, w, h)` theo DIP, tính từ góc khung tab.
type DipRect = (i32, i32, i32, i32);

/// Bố cục tab Thông thường (DIP). Hàm thuần để kiểm thử: không ô nào đè ô nào, tất cả trong khung.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GeneralLayout {
    autostart: DipRect,
    notices: DipRect,
    check_updates: DipRect,
    history: DipRect,
    history_note: DipRect,
    keep_selection: DipRect,
    include_cursor: DipRect,
    snap_windows: DipRect,
    dir_label: DipRect,
    dir: DipRect,
    browse: DipRect,
    lang_label: DipRect,
    /// Phần hộp chọn nhìn thấy khi đóng (cửa sổ thật cao hơn để chứa danh sách thả).
    language: DipRect,
    height: i32,
}

#[cfg(test)]
impl GeneralLayout {
    fn rects(&self) -> [DipRect; 13] {
        [
            self.autostart,
            self.notices,
            self.check_updates,
            self.history,
            self.history_note,
            self.keep_selection,
            self.include_cursor,
            self.snap_windows,
            self.dir_label,
            self.dir,
            self.browse,
            self.lang_label,
            self.language,
        ]
    }
}

/// Ghi chú Lịch sử thụt vào bằng ô vuông của ô tích cộng khe (theme: 18 + 6 DIP) để thẳng hàng với chữ.
const NOTE_INDENT: i32 = 24;
/// Chiều cao ô tích, bước giữa hai ô tích liền nhau, khe dưới ghi chú.
const CHECK_H: i32 = 22;
const CHECK_STEP: i32 = 26;
const NOTE_GAP: i32 = 6;

/// Bề rộng (DIP) của ô tích một dòng trên tab Thông thường.
fn general_check_width(l: Layout) -> i32 {
    l.client_w - 60
}

/// Bề rộng (DIP) của dòng ghi chú Lịch sử.
fn general_note_width(l: Layout) -> i32 {
    general_check_width(l) - NOTE_INDENT
}

/// Vị trí các ô của tab Thông thường; `note_h` là chiều cao (DIP) dòng ghi chú Lịch sử.
/// Nhóm hệ thống (khởi động, thông báo, bản mới, Lịch sử + ghi chú), rồi nhóm chụp, thư mục, ngôn ngữ.
fn general_layout(l: Layout, note_h: i32) -> GeneralLayout {
    let cw = general_check_width(l);
    let mut y = 12;
    let autostart = (LX, y, 300.min(cw), CHECK_H);
    y += CHECK_STEP;
    let notices = (LX, y, cw, CHECK_H);
    y += CHECK_STEP;
    let check_updates = (LX, y, cw, CHECK_H);
    y += CHECK_STEP;
    let history = (LX, y, cw, CHECK_H);
    y += CHECK_H + 2;
    let history_note = (LX + NOTE_INDENT, y, general_note_width(l), note_h);
    y += note_h + NOTE_GAP;
    let keep_selection = (LX, y, cw, CHECK_H);
    y += CHECK_STEP;
    let include_cursor = (LX, y, cw, CHECK_H);
    y += CHECK_STEP;
    let snap_windows = (LX, y, cw, CHECK_H);
    y += 32;
    let dir_label = (LX, y + 3, EX - LX - 4, 20);
    let dir = (EX, y, l.ew - 76, 24);
    let browse = (EX + l.ew - 70, y - 1, 70, 26);
    y += 34;
    let lang_label = (LX, y + 3, EX - LX - 4, 20);
    let language = (EX, y, 160, 24);
    y += 32;
    GeneralLayout {
        autostart,
        notices,
        check_updates,
        history,
        history_note,
        keep_selection,
        include_cursor,
        snap_windows,
        dir_label,
        dir,
        browse,
        lang_label,
        language,
        height: y + 4,
    }
}

/// Chiều cao (pixel) của `text` khi xuống dòng theo từ trong bề rộng `w_px`, vẽ bằng `font`.
fn text_height_px(font: HFONT, text: &str, w_px: i32) -> Option<i32> {
    let mut buf: Vec<u16> = text.encode_utf16().collect();
    let mut rc = RECT {
        left: 0,
        top: 0,
        right: w_px.max(1),
        bottom: 0,
    };
    // SAFETY: DC màn hình lấy và trả trong khối này; phông cũ được chọn lại trước khi trả DC.
    unsafe {
        let hdc = GetDC(None);
        if hdc.is_invalid() {
            return None;
        }
        let old = SelectObject(hdc, HGDIOBJ(font.0));
        let h = DrawTextW(
            hdc,
            &mut buf,
            &mut rc,
            DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX | DT_LEFT,
        );
        SelectObject(hdc, old);
        ReleaseDC(None, hdc);
        (h > 0).then_some(rc.bottom - rc.top)
    }
}

/// Chiều cao (DIP, làm tròn lên, cộng 2 DIP dư) đủ cho chữ cao nhất trong `texts` khi xuống dòng
/// trong bề rộng `w_dip`; đo ở đúng bề rộng pixel mà điều khiển sẽ có.
fn note_height_dip(font: HFONT, dpi: i32, texts: &[&str], w_dip: i32) -> Option<i32> {
    let dpi = dpi.max(1);
    let w_px = (w_dip * dpi + 48) / 96;
    let mut px = 0;
    for t in texts {
        px = px.max(text_height_px(font, t, w_px)?);
    }
    Some((px * 96 + dpi - 1) / dpi + 2)
}

/// Giá trị các ô tích của tab Thông thường theo cấu hình, theo thứ tự Tab.
fn general_checks(c: &Config) -> [(i32, bool); 7] {
    [
        (ID_AUTOSTART, c.autostart),
        (ID_NOTICES, c.show_success_notices),
        (ID_CHECK_UPDATES, c.check_updates),
        (ID_HISTORY, c.history_enabled),
        (ID_KEEP_SELECTION, c.keep_selection),
        (ID_INCLUDE_CURSOR, c.include_cursor),
        (ID_SNAP_WINDOWS, c.snap_windows),
    ]
}

/// Ô của tab Thông thường. Thứ tự tạo là thứ tự Tab.
fn build_general(b: &mut Builder) -> GeneralCtl {
    let notes = Lang::ALL.map(|l| t(l, Msg::NoteHistory));
    let note_w = general_note_width(b.layout);
    // Không đo được (không có phông): đủ ba dòng chữ thường.
    let note_h = b
        .font
        .and_then(|f| note_height_dip(f, b.dpi, &notes, note_w))
        .unwrap_or(54);
    let g = general_layout(b.layout, note_h);
    let check = |b: &mut Builder, msg: Msg, r: DipRect, id: i32| {
        b.button(lang::tr(msg), ws(BS_AUTOCHECKBOX), r, id)
    };
    let autostart = check(b, Msg::ChkAutostart, g.autostart, ID_AUTOSTART);
    let notices = check(b, Msg::ChkShowSuccessNotices, g.notices, ID_NOTICES);
    let check_updates = check(b, Msg::ChkCheckUpdates, g.check_updates, ID_CHECK_UPDATES);
    let history = check(b, Msg::ChkHistory, g.history, ID_HISTORY);
    // STATIC kiểu SS_LEFT tự xuống dòng; không có WS_TABSTOP.
    b.make(
        w!("STATIC"),
        lang::tr(Msg::NoteHistory),
        WINDOW_STYLE(0),
        g.history_note,
        ID_HISTORY_NOTE,
    );
    let keep_selection = check(
        b,
        Msg::ChkKeepSelection,
        g.keep_selection,
        ID_KEEP_SELECTION,
    );
    let include_cursor = check(
        b,
        Msg::ChkIncludeCursor,
        g.include_cursor,
        ID_INCLUDE_CURSOR,
    );
    let snap_windows = check(b, Msg::ChkSnapWindows, g.snap_windows, ID_SNAP_WINDOWS);
    let (lx, ly, lw, lh) = g.dir_label;
    b.label(lang::tr(Msg::LabelSaveDir), lx, ly, lw, lh);
    let dir = b.edit(WINDOW_STYLE(0), g.dir, ID_DIR);
    b.button(
        lang::tr(Msg::BtnChoose),
        ws(BS_PUSHBUTTON),
        g.browse,
        ID_BROWSE,
    );
    let (lx, ly, lw, lh) = g.lang_label;
    b.label(lang::tr(Msg::LabelLanguage), lx, ly, lw, lh);
    let language = combo(b, g.language.1, ID_LANGUAGE);
    GeneralCtl {
        autostart,
        notices,
        check_updates,
        history,
        keep_selection,
        include_cursor,
        snap_windows,
        dir,
        language,
        height: g.height,
    }
}

struct HotkeysCtl {
    chk: [HWND; HotkeyRole::ALL.len()],
    edit: [HWND; HotkeyRole::ALL.len()],
    btn: [HWND; HotkeyRole::ALL.len()],
    height: i32,
}

/// Bề rộng (DIP) của chuỗi dài nhất trong `texts` khi vẽ bằng phông của cửa sổ; `None` khi không đo được.
fn max_text_width_dip(b: &Builder, texts: &[&str]) -> Option<i32> {
    let font = b.font?;
    // SAFETY: DC màn hình lấy và trả trong khối này; phông cũ được chọn lại trước khi trả DC.
    unsafe {
        let hdc = GetDC(None);
        if hdc.is_invalid() {
            return None;
        }
        let old = SelectObject(hdc, HGDIOBJ(font.0));
        let mut widest = 0;
        for s in texts {
            let w: Vec<u16> = s.encode_utf16().collect();
            let mut size = SIZE::default();
            if GetTextExtentPoint32W(hdc, &w, &mut size).as_bool() {
                widest = widest.max(size.cx);
            }
        }
        SelectObject(hdc, old);
        ReleaseDC(None, hdc);
        Some((widest * 96 + b.dpi - 1) / b.dpi.max(1))
    }
}

/// Hình chữ nhật (x, y, w, h) tính bằng DIP, tương đối với khung của tab.
type Rect4 = (i32, i32, i32, i32);

/// Ba ô của một dòng phím tắt.
#[derive(Clone, Copy, Debug)]
struct HotkeyRow {
    chk: Rect4,
    edit: Rect4,
    btn: Rect4,
}

/// Bố cục tab Phím tắt: một dòng cho mỗi phần tử của `HotkeyRole::ALL`, cách nhau `HK_ROW_STEP`.
#[derive(Clone, Debug)]
struct HotkeysLayout {
    rows: [HotkeyRow; HotkeyRole::ALL.len()],
    height: i32,
}

impl HotkeysLayout {
    #[cfg(test)]
    /// Mọi ô, theo thứ tự dòng rồi (ô tích, ô chỉ đọc, nút): cũng là thứ tự Tab.
    fn rects(&self) -> Vec<Rect4> {
        self.rows
            .iter()
            .flat_map(|r| [r.chk, r.edit, r.btn])
            .collect()
    }
}

/// Khoảng cách dọc giữa hai dòng phím tắt (DIP).
const HK_ROW_STEP: i32 = 34;

fn hotkeys_layout(l: Layout) -> HotkeysLayout {
    let right = EX + l.ew;
    let edit_x = LX + l.hk_chk_w + HK_GAP;
    let edit_w = right - HK_BTN_W - HK_GAP - edit_x;
    let mut y = 12;
    let mut rows = [HotkeyRow {
        chk: (0, 0, 0, 0),
        edit: (0, 0, 0, 0),
        btn: (0, 0, 0, 0),
    }; HotkeyRole::ALL.len()];
    for row in rows.iter_mut() {
        *row = HotkeyRow {
            chk: (LX, y + 1, l.hk_chk_w, 22),
            edit: (edit_x, y, edit_w, 24),
            btn: (right - HK_BTN_W, y - 1, HK_BTN_W, 26),
        };
        y += HK_ROW_STEP;
    }
    HotkeysLayout {
        rows,
        height: y + 4,
    }
}

/// Ô của tab Phím tắt: mỗi dòng theo `HotkeyRole::ALL` có ô tích (nhãn của dòng), ô chỉ đọc, nút "Đổi…".
/// Cột nhãn đủ rộng cho nhãn dài nhất của mọi ngôn ngữ; ô phím lấy phần còn lại.
fn build_hotkeys(b: &mut Builder) -> HotkeysCtl {
    let lay = hotkeys_layout(b.layout);
    let (mut chk, mut edit, mut btn) = (
        [HWND::default(); HotkeyRole::ALL.len()],
        [HWND::default(); HotkeyRole::ALL.len()],
        [HWND::default(); HotkeyRole::ALL.len()],
    );
    for (i, r) in HotkeyRole::ALL.into_iter().enumerate() {
        let row = lay.rows[i];
        chk[i] = b.button(
            lang::tr(r.label()),
            ws(BS_AUTOCHECKBOX),
            row.chk,
            ID_HK_CHECK0 + i as i32,
        );
        edit[i] = b.edit(WINDOW_STYLE(ES_READONLY as u32), row.edit, -1);
        btn[i] = b.button(
            lang::tr(Msg::BtnChange),
            ws(BS_PUSHBUTTON),
            row.btn,
            ID_HK_CHANGE0 + i as i32,
        );
    }
    HotkeysCtl {
        chk,
        edit,
        btn,
        height: lay.height,
    }
}

struct FormatCtl {
    png: HWND,
    jpeg: HWND,
    quality_label: HWND,
    quality: HWND,
    quality_value: HWND,
    height: i32,
}

/// Ô của tab Định dạng. Thứ tự Tab: nhóm nút chọn (một điểm dừng, mũi tên đổi giữa hai nút), rồi
/// thanh trượt. Nhãn chất lượng đứng ngay trước thanh trượt để trình đọc màn hình lấy làm tên.
fn build_format(b: &mut Builder) -> FormatCtl {
    let w = b.layout.client_w - 60;
    let mut y = 12;
    b.label(lang::tr(Msg::LabelUploadFormat), LX, y, w, 20);
    y += 24;
    let png = b.button(
        lang::tr(UploadFormat::Png.label()),
        ws(BS_AUTORADIOBUTTON),
        (LX + 12, y, w - 12, 22),
        ID_FMT_PNG,
    );
    y += 26;
    // Nút thứ hai cùng nhóm: không WS_GROUP, không WS_TABSTOP.
    let jpeg = b.make(
        w!("BUTTON"),
        lang::tr(UploadFormat::Jpeg.label()),
        ws(BS_AUTORADIOBUTTON),
        (LX + 12, y, w - 12, 22),
        ID_FMT_JPEG,
    );
    y += 36;
    // WS_GROUP khép nhóm nút chọn ở trên.
    let quality_label = b.make(
        w!("STATIC"),
        lang::tr(Msg::LabelJpegQuality),
        WS_GROUP,
        (LX, y + 5, EX - LX - 4, 20),
        -1,
    );
    let value_w = 40;
    let quality = b.make(
        TRACKBAR_CLASSW,
        "",
        WS_GROUP | WS_TABSTOP | ws(TBS_HORZ as i32),
        (EX, y, b.layout.ew - value_w - HK_GAP, 30),
        ID_QUALITY,
    );
    let quality_value = b.make(
        w!("STATIC"),
        "",
        WINDOW_STYLE(0),
        (EX + b.layout.ew - value_w, y + 5, value_w, 20),
        ID_QUALITY_VALUE,
    );
    y += 40;
    b.make(
        w!("STATIC"),
        lang::tr(Msg::NoteSavedAlwaysPng),
        WINDOW_STYLE(0),
        (LX, y, w, 20),
        ID_PNG_NOTE,
    );
    y += 28;
    FormatCtl {
        png,
        jpeg,
        quality_label,
        quality,
        quality_value,
        height: y + 4,
    }
}

struct R2Ctl {
    account: HWND,
    keyid: HWND,
    secret: HWND,
    bucket: HWND,
    url: HWND,
    expiry: HWND,
    test_btn: HWND,
    test_res: HWND,
    height: i32,
}

/// Ô của tab Lưu trữ R2. "Account ID", "Access Key ID", "Secret Access Key" là tên riêng, không dịch.
fn build_r2(b: &mut Builder, current: &Config) -> R2Ctl {
    let ew = b.layout.ew;
    let mut y = 12;
    let account = row_edit(b, "Account ID", y, ew, WINDOW_STYLE(0), ID_ACCOUNT);
    y += 28;
    let keyid = row_edit(b, "Access Key ID", y, ew, WINDOW_STYLE(0), ID_KEYID);
    y += 28;
    let secret = row_edit(
        b,
        "Secret Access Key",
        y,
        ew,
        WINDOW_STYLE(ES_PASSWORD as u32),
        ID_SECRET,
    );
    y += 28;
    let hint = if current.r2.secret_access_key.is_empty() {
        lang::tr(Msg::SecretNone)
    } else {
        lang::tr(Msg::SecretSaved)
    };
    b.make(
        w!("STATIC"),
        hint,
        WINDOW_STYLE(0),
        (EX, y - 2, ew, 18),
        ID_SECRET_NOTE,
    );
    y += 20;
    let bucket = row_edit(
        b,
        lang::tr(Msg::LabelBucket),
        y,
        ew,
        WINDOW_STYLE(0),
        ID_BUCKET,
    );
    y += 28;
    let url = row_edit(
        b,
        lang::tr(Msg::LabelPublicUrl),
        y,
        ew,
        WINDOW_STYLE(0),
        ID_URL,
    );
    y += 28;
    b.label(lang::tr(Msg::LabelExpiry), LX, y + 3, EX - LX - 4, 20);
    let expiry = combo(b, y, ID_EXPIRY);
    y += 32;
    let test_btn = b.button(
        lang::tr(Msg::BtnTest),
        ws(BS_PUSHBUTTON),
        (LX, y, 140, 26),
        ID_TEST,
    );
    let test_res = b.label("", EX, y + 4, ew, 36);
    y += 44;
    R2Ctl {
        account,
        keyid,
        secret,
        bucket,
        url,
        expiry,
        test_btn,
        test_res,
        height: y + 4,
    }
}

/// Dựng các điều khiển; trả về chiều cao vùng client (đơn vị DIP) và bộ HWND.
///
/// Thứ tự Z (cũng là thứ tự Tab của `IsDialogMessageW`): khung các tab, dòng lỗi, Trợ giúp, OK, Hủy, rồi
/// dải tab ở đáy. Vòng Tab vì vậy là: dải tab → ô của khung đang hiện → Trợ giúp → OK → Hủy → dải tab. Dải tab có
/// `WS_CLIPSIBLINGS` và nằm dưới các khung nên không vẽ đè lên chúng.
fn build_controls(b: &mut Builder, current: &Config, notice: Option<&str>) -> (i32, Option<Ctl>) {
    let main = b.parent;
    let labels: Vec<&str> = HotkeyRole::ALL
        .iter()
        .flat_map(|r| Lang::ALL.map(|l| t(l, r.label())))
        .collect();
    let mut layout = Layout::from_label_width(max_text_width_dip(b, &labels));
    // Dải tab bốn mục phải vừa một hàng ở mọi ngôn ngữ: đo nhãn tab của từng ngôn ngữ và nới nếu cần.
    for l in Lang::ALL {
        let widths: Option<Vec<i32>> = Tab::ALL
            .iter()
            .map(|tab| max_text_width_dip(b, &[t(l, tab.label())]).map(|w| w + TAB_ITEM_PAD))
            .collect();
        if let Some(tw) = widths {
            layout = layout.widen_for_tabs(&tw);
        }
    }
    b.layout = layout;
    let inner_w = b.layout.client_w - 24;
    let mut y = 12;
    if let Some(n) = notice {
        b.label(n, 12, y, inner_w, 34);
        y += 42;
    }
    let tab_top = y;

    // Dải tab: phải có phông và các mục trước khi hỏi TCM_ADJUSTRECT.
    let tabs = b.make(
        WC_TABCONTROLW,
        "",
        WS_GROUP | WS_TABSTOP | WS_CLIPSIBLINGS,
        (12, tab_top, inner_w, 300),
        ID_TABS,
    );
    for (i, t) in Tab::ALL.iter().enumerate() {
        let mut text: Vec<u16> = lang::tr(t.label())
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let item = TCITEMW {
            mask: TCIF_TEXT,
            pszText: PWSTR(text.as_mut_ptr()),
            ..Default::default()
        };
        // SAFETY: `item` và bộ đệm chữ sống suốt lời gọi; dải tab tự chép chữ.
        unsafe {
            SendMessageW(
                tabs,
                TCM_INSERTITEMW,
                Some(WPARAM(i)),
                Some(LPARAM(&item as *const TCITEMW as isize)),
            );
        }
    }

    // Bốn khung (ẩn, tắt) và ô của chúng; kích thước khung đặt sau khi biết tab cao nhất.
    let frames = [
        b.frame(ID_FRAME0),
        b.frame(ID_FRAME0 + 1),
        b.frame(ID_FRAME0 + 2),
        b.frame(ID_FRAME0 + 3),
    ];
    b.parent = frames[Tab::General.index()];
    let g = build_general(b);
    b.parent = frames[Tab::Hotkeys.index()];
    let hk = build_hotkeys(b);
    b.parent = frames[Tab::Format.index()];
    let fmt = build_format(b);
    b.parent = frames[Tab::R2.index()];
    let r2 = build_r2(b, current);
    b.parent = main;
    // Cửa sổ cao theo tab cao nhất.
    let frame_h = g.height.max(hk.height).max(fmt.height).max(r2.height);

    // Vùng hiển thị của dải tab cao đúng bằng tab cao nhất; khung phủ kín vùng đó.
    let (left, top, width) = (b.s(12), b.s(tab_top), b.s(inner_w));
    let probe_h = b.s(300);
    let mut probe = RECT {
        left,
        top,
        right: left + width,
        bottom: top + probe_h,
    };
    // SAFETY: probe là biến cục bộ; dải tab chỉ đổi giá trị của nó.
    unsafe {
        SendMessageW(
            tabs,
            TCM_ADJUSTRECT,
            Some(WPARAM(0)),
            Some(LPARAM(&mut probe as *mut RECT as isize)),
        );
    }
    let bottom_inset = (top + probe_h - probe.bottom).max(0);
    let disp = RECT {
        left: probe.left,
        top: probe.top.max(top),
        right: probe.right.max(probe.left),
        bottom: probe.top.max(top) + b.s(frame_h),
    };
    let tab_bottom = disp.bottom + bottom_inset;
    // SAFETY: đổi vị trí và kích thước các cửa sổ con vừa tạo.
    unsafe {
        for f in frames {
            let _ = SetWindowPos(
                f,
                None,
                disp.left,
                disp.top,
                disp.right - disp.left,
                disp.bottom - disp.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        let _ = SetWindowPos(
            tabs,
            None,
            left,
            top,
            width,
            tab_bottom - top,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }

    // Dòng lỗi và hàng nút, dưới dải tab. Nút Trợ giúp ở x = 12; OK và Hủy ở bên phải.
    y = (tab_bottom * 96 + b.dpi - 1) / b.dpi.max(1) + 8;
    let err = b.label("", 12, y, inner_w, 36);
    y += 40;
    // Nút Trợ giúp tạo ngay trước OK để đứng liền trước OK trong thứ tự Tab; không phải nút mặc định.
    b.button(
        lang::tr(Msg::BtnHelp),
        ws(BS_PUSHBUTTON),
        (12, y, 80, 28),
        ID_HELP,
    );
    b.button(
        i18n::OK_LABEL,
        ws(BS_DEFPUSHBUTTON),
        (b.layout.client_w - 180, y, 80, 28),
        ID_OK,
    );
    // Hủy không có WS_GROUP: OK và Hủy cùng một nhóm mũi tên.
    b.make(
        w!("BUTTON"),
        lang::tr(Msg::BtnCancel),
        WS_TABSTOP | ws(BS_PUSHBUTTON),
        (b.layout.client_w - 92, y, 80, 28),
        ID_CANCEL,
    );
    y += 40;
    // Đặt lại dải tab xuống đáy thứ tự Z sau khi mọi cửa sổ con đã có.
    // SAFETY: đổi thứ tự Z của dải tab vừa tạo.
    unsafe {
        let _ = SetWindowPos(
            tabs,
            Some(HWND_BOTTOM),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }

    let all = [
        tabs,
        frames[0],
        frames[1],
        frames[2],
        frames[3],
        fmt.png,
        fmt.jpeg,
        fmt.quality_label,
        fmt.quality,
        fmt.quality_value,
        g.language,
        r2.account,
        r2.keyid,
        r2.secret,
        r2.bucket,
        r2.url,
        r2.test_btn,
        r2.test_res,
        r2.expiry,
        g.dir,
        g.autostart,
        g.notices,
        g.check_updates,
        g.history,
        g.keep_selection,
        g.include_cursor,
        g.snap_windows,
        err,
    ];
    if all
        .iter()
        .chain(hk.chk.iter())
        .chain(hk.edit.iter())
        .chain(hk.btn.iter())
        .any(|h| h.is_invalid())
    {
        return (y, None);
    }
    let ctl = Ctl {
        tabs,
        frames,
        language: g.language,
        fmt_png: fmt.png,
        fmt_jpeg: fmt.jpeg,
        quality_label: fmt.quality_label,
        quality: fmt.quality,
        quality_value: fmt.quality_value,
        account: r2.account,
        keyid: r2.keyid,
        secret: r2.secret,
        bucket: r2.bucket,
        url: r2.url,
        test_btn: r2.test_btn,
        test_res: r2.test_res,
        expiry: r2.expiry,
        dir: g.dir,
        hk_chk: hk.chk,
        hk_edit: hk.edit,
        hk_btn: hk.btn,
        autostart: g.autostart,
        notices: g.notices,
        check_updates: g.check_updates,
        history: g.history,
        keep_selection: g.keep_selection,
        include_cursor: g.include_cursor,
        snap_windows: g.snap_windows,
        err,
    };
    (y, Some(ctl))
}

/// Đưa cửa sổ Tùy chọn đang mở (nếu có) ra trước; không làm gì khác.
pub fn focus_existing() {
    let open = OPEN.with(|o| o.get());
    if open == 0 {
        return;
    }
    let h = HWND(open as *mut c_void);
    // SAFETY: `h` là cửa sổ Tùy chọn của luồng này, OPEN chỉ khác 0 khi cửa sổ còn sống.
    unsafe {
        if IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_RESTORE);
        }
        let _ = SetForegroundWindow(h);
    }
}

/// Các dòng phím tắt không đăng ký lại được khi lượt `show` vừa rồi kết thúc bằng Hủy (rỗng nếu lượt
/// đó không phải đăng ký lại, hoặc đăng ký lại đủ). Lấy ra một lần.
pub fn take_cancel_failures() -> Vec<HotkeyRole> {
    CANCEL_FAILED.with(|f| std::mem::take(&mut *f.borrow_mut()))
}

/// Mở cửa sổ Tùy chọn và chờ đến khi người dùng bấm OK hoặc Hủy. `None` khi Hủy (hoặc đã có cửa sổ mở).
pub fn show(
    parent: HWND,
    current: &Config,
    notice: Option<&str>,
    reason: OpenReason,
) -> Option<SettingsResult> {
    let open = OPEN.with(|o| o.get());
    if open != 0 {
        let h = HWND(open as *mut c_void);
        // SAFETY: đưa cửa sổ Tùy chọn đang có ra trước.
        unsafe {
            if IsIconic(h).as_bool() {
                let _ = ShowWindow(h, SW_RESTORE);
            }
            let _ = SetForegroundWindow(h);
        }
        return None;
    }
    CANCEL_FAILED.with(|f| f.borrow_mut().clear());

    let title = HSTRING::from(lang::tr(Msg::OptionsTitle));
    // SAFETY: đăng ký lớp và tạo cửa sổ ở luồng chính; mọi handle được giải phóng ở cuối hàm.
    let hwnd = unsafe {
        let hinst = GetModuleHandleW(None).ok()?;
        let class = w!("darkshot-settings");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            lpszClassName: class,
            hbrBackground: theme::brush(BG),
            ..Default::default()
        };
        // Đăng ký lại lần hai báo lỗi vô hại.
        RegisterClassW(&wc);
        // Lớp của khung tab: nền tự tô trong `frame_proc`.
        let frame_wc = WNDCLASSW {
            lpfnWndProc: Some(frame_proc),
            hInstance: hinst.into(),
            lpszClassName: FRAME_CLASS,
            ..Default::default()
        };
        RegisterClassW(&frame_wc);
        // Đăng ký lớp dải tab và thanh trượt của Common Controls v6.
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_TAB_CLASSES | ICC_BAR_CLASSES,
        };
        let _ = InitCommonControlsEx(&icc);
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT | WS_EX_APPWINDOW,
            class,
            &title,
            WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            100,
            100,
            Some(parent),
            None,
            Some(hinst.into()),
            None,
        )
        .ok()?;
        crate::winutil::set_app_icons(hwnd);
        hwnd
    };
    // Cửa sổ mới chưa có lần kiểm tra nào: bỏ kết quả và cờ của cửa sổ trước (nếu còn sót).
    GENERATION.fetch_add(1, Ordering::SeqCst);
    TEST_RUNNING.with(|r| r.set(false));
    if let Ok(mut s) = TEST_RESULT.lock() {
        *s = None;
    }
    // SAFETY: hwnd vừa tạo.
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let font = crate::winutil::system_font(dpi);
    DPI.with(|d| d.set(dpi));
    // Xóa ở WM_NCDESTROY của cửa sổ, kể cả khi dựng điều khiển lỗi.
    BOLD.with(|b| b.set(crate::winutil::system_font_styled(dpi, 100, true)));
    TAB_HOT.with(|h| h.set(-1));
    TEST_TONE.with(|t| t.set(Tone::Neutral));
    let mut b = Builder {
        parent: hwnd,
        dpi: dpi as i32,
        font,
        layout: Layout::from_label_width(None),
    };
    let (client_h, controls) = build_controls(&mut b, current, notice);
    let Some(c) = controls else {
        // SAFETY: dọn cửa sổ vừa tạo (kéo theo mọi cửa sổ con) rồi phông.
        unsafe {
            let _ = DestroyWindow(hwnd);
            if let Some(f) = font {
                let _ = DeleteObject(HGDIOBJ(f.0));
            }
        }
        return None;
    };
    CTL.with(|x| x.set(Some(c)));
    OPEN.with(|o| o.set(hwnd.0 as isize));
    apply_dark_controls(&c);
    ST.with(|s| {
        *s.borrow_mut() = Some(St {
            current: current.clone(),
            parent,
            hotkeys: current.hotkeys,
            capturing: None,
            cap: CaptureState::new(),
            hk_unregistered: false,
            done: false,
            result: None,
            font,
            tab: Tab::General,
        })
    });
    let first_tab = initial_tab(reason);
    // SAFETY: chọn tab mở đầu trên dải tab của cửa sổ này (không sinh TCN_SELCHANGE).
    unsafe {
        SendMessageW(c.tabs, TCM_SETCURSEL, Some(WPARAM(first_tab.index())), None);
    }
    show_frame(&c, first_tab);

    // Điền giá trị ban đầu. Ô Secret luôn để trống.
    set_text(c.account, &current.r2.account_id);
    set_text(c.keyid, &current.r2.access_key_id);
    set_text(c.bucket, &current.r2.bucket);
    set_text(c.url, &current.r2.public_base_url);
    set_text(c.dir, &current.save_dir.to_string_lossy());
    for r in HotkeyRole::ALL {
        let on = current.hotkeys.get(r).enabled;
        set_checked(c.hk_chk[row(r)], on);
        apply_row_enabled(&c, r, on);
        refresh_hotkey_text(r);
    }
    for (id, on) in general_checks(current) {
        let h = match id {
            ID_AUTOSTART => c.autostart,
            ID_NOTICES => c.notices,
            ID_CHECK_UPDATES => c.check_updates,
            ID_HISTORY => c.history,
            ID_KEEP_SELECTION => c.keep_selection,
            ID_SNAP_WINDOWS => c.snap_windows,
            _ => c.include_cursor,
        };
        set_checked(h, on);
    }
    set_checked(c.fmt_png, current.upload_format == UploadFormat::Png);
    set_checked(c.fmt_jpeg, current.upload_format == UploadFormat::Jpeg);
    let range = (usize::from(JPEG_QUALITY_MAX) << 16) | usize::from(JPEG_QUALITY_MIN);
    let pos = clamp_jpeg_quality(i64::from(current.jpeg_quality));
    // SAFETY: gửi thông điệp tới thanh trượt của cửa sổ này; tham số là số nguyên.
    unsafe {
        SendMessageW(
            c.quality,
            TBM_SETRANGE,
            Some(WPARAM(0)),
            Some(LPARAM(range as isize)),
        );
        SendMessageW(c.quality, TBM_SETLINESIZE, None, Some(LPARAM(1)));
        SendMessageW(c.quality, TBM_SETPAGESIZE, None, Some(LPARAM(10)));
        SendMessageW(
            c.quality,
            TBM_SETPOS,
            Some(WPARAM(1)),
            Some(LPARAM(isize::from(pos))),
        );
    }
    refresh_quality_text(&c);
    apply_quality_enabled(&c);
    let labels = Expiry::ALL.map(|e| expiry_label(lang::current(), e));
    // SAFETY: gửi thông điệp tới các điều khiển của cửa sổ này; HSTRING sống suốt lời gọi.
    unsafe {
        for l in labels {
            let hs = HSTRING::from(l);
            SendMessageW(
                c.expiry,
                CB_ADDSTRING,
                None,
                Some(LPARAM(hs.as_ptr() as isize)),
            );
        }
        let idx = Expiry::ALL
            .iter()
            .position(|e| *e == current.expiry)
            .unwrap_or(2);
        SendMessageW(c.expiry, CB_SETCURSEL, Some(WPARAM(idx)), None);
        // Mỗi ngôn ngữ luôn hiện bằng chính ngôn ngữ đó.
        for l in Lang::ALL {
            let hs = HSTRING::from(l.native_name());
            SendMessageW(
                c.language,
                CB_ADDSTRING,
                None,
                Some(LPARAM(hs.as_ptr() as isize)),
            );
        }
        let lang_idx = Lang::ALL
            .iter()
            .position(|l| *l == current.language)
            .unwrap_or(0);
        SendMessageW(c.language, CB_SETCURSEL, Some(WPARAM(lang_idx)), None);
    }

    // Đổi kích thước theo DPI, căn giữa màn hình đang có con trỏ.
    // SAFETY: các lời gọi hệ thống chỉ dùng giá trị cục bộ.
    unsafe {
        let mut rc = RECT {
            left: 0,
            top: 0,
            right: b.s(b.layout.client_w),
            bottom: b.s(client_h),
        };
        let _ = AdjustWindowRectExForDpi(
            &mut rc,
            WS_CAPTION | WS_SYSMENU,
            false,
            WS_EX_CONTROLPARENT | WS_EX_APPWINDOW,
            dpi,
        );
        crate::winutil::center_on_cursor_monitor(hwnd, rc.right - rc.left, rc.bottom - rc.top);
        // Thanh tiêu đề tối trước khi hiện, để không nhá trắng.
        theme::apply_dark_title_bar(hwnd);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(first_ctl(&c, first_tab)));
    }

    run_loop(hwnd, c);

    // Dọn dẹp: cửa sổ, phông, trạng thái.
    let result = with_st(|s| s.result.take()).flatten();
    // SAFETY: hủy cửa sổ và phông do hàm này tạo.
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
    if let Some(Some(f)) = ST.with(|s| s.borrow_mut().take().map(|st| st.font)) {
        // SAFETY: phông do CreateFontIndirectW tạo, cửa sổ dùng nó đã bị hủy.
        unsafe {
            let _ = DeleteObject(HGDIOBJ(f.0));
        }
    }
    CTL.with(|x| x.set(None));
    OPEN.with(|o| o.set(0));
    // Thế hệ mới: kết quả muộn của luồng kiểm tra đang chạy sẽ bị bỏ. Hẹn giờ dự phòng đã chết
    // theo cửa sổ, nên cờ phải hạ ở đây, không thì nút Kiểm tra kết nối của lần mở sau bị khóa.
    GENERATION.fetch_add(1, Ordering::SeqCst);
    TEST_RUNNING.with(|r| r.set(false));
    result.map(|config| SettingsResult { config })
}

fn run_loop(hwnd: HWND, c: Ctl) {
    let mut msg = MSG::default();
    loop {
        if with_st(|s| s.done).unwrap_or(true) {
            break;
        }
        // SAFETY: vòng lặp thông điệp chuẩn của luồng này.
        let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if r.0 == 0 {
            // WM_QUIT: trả lại cho vòng lặp ngoài rồi thoát.
            // SAFETY: gửi lại yêu cầu thoát.
            unsafe { PostQuitMessage(msg.wParam.0 as i32) };
            cancel();
            break;
        }
        if r.0 < 0 {
            cancel();
            break;
        }
        // Thông tin, Trợ giúp và cửa sổ Nhận dạng chữ dùng được cả khi Tùy chọn đang mở: chúng xử lý
        // phím của chính chúng trước.
        if crate::about::pre_translate(&msg)
            || crate::help::pre_translate(&msg)
            || crate::ocr_window::pre_translate(&msg)
        {
            continue;
        }
        // SAFETY: IsChild chỉ đọc quan hệ cửa sổ.
        let mine = msg.hwnd == hwnd || unsafe { IsChild(hwnd, msg.hwnd).as_bool() };
        if mine {
            let capturing_row = with_st(|s| s.capturing).flatten();
            let capturing = capturing_row.is_some();
            if let Some(r) = capturing_row {
                if capture_msg(&msg, c.hk_btn[row(r)]) {
                    continue;
                }
            }
            if msg.message == WM_KEYDOWN {
                let vk = msg.wParam.0 as u32;
                // SAFETY: chỉ đọc trạng thái hộp thả.
                let dropped = [c.expiry, c.language]
                    .iter()
                    .any(|h| unsafe { SendMessageW(*h, CB_GETDROPPEDSTATE, None, None).0 != 0 });
                let route = key_route(
                    capturing,
                    vk,
                    key_down(VK_CONTROL_U),
                    key_down(VK_SHIFT.0 as i32),
                    dropped,
                );
                match route {
                    KeyRoute::NextTab => {
                        cycle_tab(1);
                        continue;
                    }
                    KeyRoute::PrevTab => {
                        cycle_tab(Tab::ALL.len() - 1);
                        continue;
                    }
                    KeyRoute::Cancel => {
                        cancel();
                        continue;
                    }
                    // Đang bắt phím thì `capture_msg` ở trên đã nhận phím; còn lại cho hộp thoại.
                    KeyRoute::Capture | KeyRoute::Dialog => {}
                    KeyRoute::Accept => {
                        // SAFETY: chỉ đọc điều khiển đang giữ focus.
                        let focus = unsafe { GetFocus() };
                        let is_push = unsafe {
                            let st = GetWindowLongPtrW(focus, GWL_STYLE) as u32;
                            let kind = st & 0xF;
                            focus != HWND::default()
                                && (kind == BS_PUSHBUTTON as u32 || kind == BS_DEFPUSHBUTTON as u32)
                                && focus != c.autostart
                                && get_class_is_button(focus)
                        };
                        if is_push {
                            // SAFETY: bấm nút đang giữ focus.
                            unsafe {
                                SendMessageW(focus, BM_CLICK, None, None);
                            }
                        } else {
                            save();
                        }
                        continue;
                    }
                }
            }
            // SAFETY: xử lý phím điều hướng (Tab, mũi tên) của hộp thoại.
            if unsafe { IsDialogMessageW(hwnd, &msg).as_bool() } {
                continue;
            }
        }
        // SAFETY: chuyển thông điệp chuẩn.
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn get_class_is_button(h: HWND) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetClassNameW;
    let mut buf = [0u16; 16];
    // SAFETY: bộ đệm cục bộ đủ chỗ.
    let n = unsafe { GetClassNameW(h, &mut buf) }.max(0) as usize;
    String::from_utf16_lossy(&buf[..n.min(buf.len())]).eq_ignore_ascii_case("button")
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkshot_core::config::R2Config;
    use std::path::Path;

    fn base() -> Config {
        let mut c = Config::defaults(Path::new("C:/pics"), Lang::Vi);
        c.r2 = R2Config {
            account_id: "acc".into(),
            access_key_id: "kid".into(),
            secret_access_key: "OLD".into(),
            bucket: "b".into(),
            public_base_url: "https://img.example.com".into(),
        };
        c
    }

    fn form_of(c: &Config) -> Form {
        Form {
            account_id: c.r2.account_id.clone(),
            access_key_id: c.r2.access_key_id.clone(),
            secret_access_key: String::new(),
            bucket: c.r2.bucket.clone(),
            public_base_url: c.r2.public_base_url.clone(),
            expiry: c.expiry,
            hotkeys: c.hotkeys,
            save_dir: c.save_dir.to_string_lossy().into_owned(),
            autostart: c.autostart,
            language: c.language,
            show_success_notices: c.show_success_notices,
            check_updates: c.check_updates,
            keep_selection: c.keep_selection,
            include_cursor: c.include_cursor,
            upload_format: c.upload_format,
            jpeg_quality: c.jpeg_quality,
            history_enabled: c.history_enabled,
            snap_windows: c.snap_windows,
        }
    }

    #[test]
    fn snap_windows_round_trips_and_others_unchanged() {
        for on in [false, true] {
            let mut c = base();
            c.snap_windows = on;
            let n = apply_form(&c, &form_of(&c), Lang::Vi).unwrap();
            assert_eq!(n.snap_windows, on);
            assert_eq!(n, c);
            let mut f = form_of(&c);
            f.snap_windows = !on;
            let n = apply_form(&c, &f, Lang::Vi).unwrap();
            assert_eq!(n.snap_windows, !on);
            let mut want = c.clone();
            want.snap_windows = !on;
            assert_eq!(n, want);
            assert!(general_checks(&c).contains(&(ID_SNAP_WINDOWS, on)));
        }
        assert_eq!(ID_SNAP_WINDOWS, 155);
    }

    #[test]
    fn history_ids_are_153_and_154() {
        assert_eq!(ID_HISTORY, 153);
        assert_eq!(ID_HISTORY_NOTE, 154);
    }

    /// Cấu hình → hộp tích (`general_checks`, theo thứ tự Tab) và hộp tích → cấu hình (`apply_form`).
    #[test]
    fn history_check_box_round_trips_config() {
        for on in [false, true] {
            let mut c = base();
            c.history_enabled = on;
            let checks = general_checks(&c);
            assert_eq!(
                checks.map(|(id, _)| id),
                [
                    ID_AUTOSTART,
                    ID_NOTICES,
                    ID_CHECK_UPDATES,
                    ID_HISTORY,
                    ID_KEEP_SELECTION,
                    ID_INCLUDE_CURSOR,
                    ID_SNAP_WINDOWS
                ]
            );
            assert!(checks.contains(&(ID_HISTORY, on)));
            let mut f = form_of(&c);
            f.history_enabled = !on;
            assert_eq!(apply_form(&c, &f, Lang::Vi).unwrap().history_enabled, !on);
        }
    }

    /// Hộp Lịch sử và ghi chú nằm sau "Báo có bản mới", trước nhóm chụp; ghi chú đủ cao cho chữ dài
    /// nhất (đo thật bằng phông hệ thống) ở 100/150/200%; không ô nào đè ô nào và tất cả trong khung.
    #[test]
    fn general_tab_fits_history_note_at_common_dpis() {
        // Tạo phông và DC: không chen vào kiểm thử đếm GDI của cả tiến trình.
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        let texts = Lang::ALL.map(|l| t(l, Msg::NoteHistory));
        for label_w in [None, Some(250)] {
            for dpi in [96u32, 144, 192] {
                let font = crate::winutil::system_font(dpi).expect("system font");
                let l = Layout::from_label_width(label_w);
                let w = general_note_width(l);
                let note_h = note_height_dip(font, dpi as i32, &texts, w).expect("measure");
                for text in texts {
                    let need = text_height_px(font, text, scale(w, dpi)).expect("measure px");
                    assert!(
                        scale(note_h, dpi) >= need,
                        "dpi {dpi}: note {note_h} DIP < {need} px"
                    );
                }
                // Câu dài: ít nhất hai dòng ở bề rộng tối thiểu, không quá năm dòng.
                let one = text_height_px(font, "Ag", 10_000).unwrap();
                assert!(scale(note_h, dpi) >= one, "dpi {dpi}");
                assert!(scale(note_h, dpi) <= one * 5 + scale(4, dpi), "dpi {dpi}");
                // SAFETY: phông tạo riêng cho kiểm thử, không DC nào còn chọn nó.
                unsafe {
                    let _ = DeleteObject(HGDIOBJ(font.0));
                }

                let g = general_layout(l, note_h);
                assert!(g.check_updates.1 < g.history.1, "dpi {dpi}");
                assert!(g.history.1 < g.history_note.1, "dpi {dpi}");
                assert!(g.history_note.1 < g.keep_selection.1, "dpi {dpi}");
                assert!(g.include_cursor.1 < g.snap_windows.1, "dpi {dpi}");
                assert!(g.snap_windows.1 < g.dir_label.1, "dpi {dpi}");
                assert_eq!(g.snap_windows.1 - g.include_cursor.1, CHECK_STEP);
                assert_eq!(g.history_note.3, note_h);
                let rects = g.rects();
                let px = |r: (i32, i32, i32, i32)| {
                    let (x, y) = (scale(r.0, dpi), scale(r.1, dpi));
                    (x, y, x + scale(r.2, dpi), y + scale(r.3, dpi))
                };
                for (i, a) in rects.iter().enumerate() {
                    assert!(a.0 >= LX && a.1 >= 0, "dpi {dpi}: {a:?}");
                    // Khung rộng `client_w - 24` trừ viền dải tab: chừa thêm 12 DIP.
                    assert!(a.0 + a.2 <= l.client_w - 36, "dpi {dpi}: {a:?}");
                    assert!(a.1 + a.3 <= g.height, "dpi {dpi}: {a:?}");
                    for b in &rects[i + 1..] {
                        let (pa, pb) = (px(*a), px(*b));
                        let apart = pa.2 <= pb.0 || pb.2 <= pa.0 || pa.3 <= pb.1 || pb.3 <= pa.1;
                        assert!(apart, "dpi {dpi}: {a:?} overlaps {b:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn layout_keeps_hotkey_edit_wide_enough() {
        for label_w in [None, Some(0), Some(100), Some(168), Some(250), Some(400)] {
            let l = Layout::from_label_width(label_w);
            let edit_x = LX + l.hk_chk_w + HK_GAP;
            let edit_w = EX + l.ew - HK_BTN_W - HK_GAP - edit_x;
            assert!(edit_w >= HK_EDIT_MIN_W, "{label_w:?}: {edit_w}");
            assert!(l.client_w >= CLIENT_MIN_W, "{label_w:?}");
            assert_eq!(l.client_w, EX + l.ew + RIGHT_MARGIN);
        }
        // Nhãn ngắn: giữ bố cục cũ 480 DIP, ô nhập 270 DIP.
        let l = Layout::from_label_width(Some(50));
        assert_eq!((l.client_w, l.ew), (CLIENT_MIN_W, 270));
    }

    #[test]
    fn hotkeys_layout_has_one_row_per_role_without_overlap() {
        assert_eq!(HotkeyRole::ALL.len(), 4);
        for label_w in [None, Some(100), Some(168), Some(250)] {
            let l = Layout::from_label_width(label_w);
            let h = hotkeys_layout(l);
            assert_eq!(h.rows.len(), HotkeyRole::ALL.len());
            for dpi in [96u32, 120, 144, 192] {
                let px = |r: (i32, i32, i32, i32)| {
                    let (x, y) = (scale(r.0, dpi), scale(r.1, dpi));
                    (x, y, x + scale(r.2, dpi), y + scale(r.3, dpi))
                };
                let rects = h.rects();
                assert_eq!(rects.len(), 3 * HotkeyRole::ALL.len());
                for (i, a) in rects.iter().enumerate() {
                    assert!(a.0 >= LX && a.1 >= 0, "dpi {dpi}: {a:?}");
                    assert!(a.0 + a.2 <= l.client_w - 36, "dpi {dpi}: {a:?}");
                    assert!(a.1 + a.3 <= h.height, "dpi {dpi}: {a:?}");
                    for b in &rects[i + 1..] {
                        let (pa, pb) = (px(*a), px(*b));
                        let apart = pa.2 <= pb.0 || pb.2 <= pa.0 || pa.3 <= pb.1 || pb.3 <= pa.1;
                        assert!(apart, "dpi {dpi}: {a:?} overlaps {b:?}");
                    }
                }
            }
            // Dòng sau nằm dưới dòng trước.
            for w in h.rows.windows(2) {
                assert!(w[0].chk.1 + w[0].chk.3 <= w[1].chk.1);
                assert!(w[0].btn.1 + w[0].btn.3 <= w[1].btn.1);
            }
        }
    }

    #[test]
    fn hotkey_labels_fit_on_one_line_at_all_dpi() {
        // Tạo phông và DC: không chen vào kiểm thử đếm GDI của cả tiến trình.
        let _gdi = crate::capture::GDI_TEST_LOCK.lock();
        for dpi in [96u32, 120, 144, 192] {
            let font = crate::winutil::system_font(dpi).expect("system font");
            // Bề rộng nhãn như `build_controls` đo: dài nhất mọi dòng, mọi ngôn ngữ.
            let one = text_height_px(font, "Ag", 10_000).unwrap();
            let mut widest = 0;
            for r in HotkeyRole::ALL {
                for lang in Lang::ALL {
                    let text = t(lang, r.label());
                    // Tìm bề rộng nhỏ nhất để chữ vẫn một dòng.
                    let mut w = 1;
                    while text_height_px(font, text, w).unwrap() > one {
                        w += 1;
                    }
                    widest = widest.max(w);
                }
            }
            let widest_dip = (widest * 96 + dpi as i32 - 1) / dpi as i32;
            let l = Layout::from_label_width(Some(widest_dip));
            assert!(
                scale(l.hk_chk_w - HK_BOX, dpi) >= widest,
                "dpi {dpi}: {} < {widest}",
                scale(l.hk_chk_w - HK_BOX, dpi)
            );
            // SAFETY: phông tạo riêng cho kiểm thử, không DC nào còn chọn nó.
            unsafe {
                let _ = DeleteObject(HGDIOBJ(font.0));
            }
        }
    }

    #[test]
    fn layout_widens_for_tab_strip() {
        let base = Layout::from_label_width(Some(50));
        // Dải tab ngắn: giữ nguyên bố cục.
        assert_eq!(base.widen_for_tabs(&[60, 60, 60, 60]), base);
        // Dải tab dài: client_w và ô nhập nới cùng một lượng, dải tab vừa khít.
        let wide = [200, 200, 200, 200];
        let l = base.widen_for_tabs(&wide);
        assert!(l.client_w > base.client_w);
        assert_eq!(l.client_w - base.client_w, l.ew - base.ew);
        assert_eq!(l.hk_chk_w, base.hk_chk_w);
        assert_eq!(l.client_w, EX + l.ew + RIGHT_MARGIN);
        assert!(tabs_fit(&wide, l.tab_strip_w()));
        let mut one_less = l;
        one_less.client_w -= 1;
        assert!(!tabs_fit(&wide, one_less.tab_strip_w()));
    }

    #[test]
    fn capture_hotkey_rules() {
        // PrintScreen đứng một mình được phép; phím chữ phải kèm phím bổ trợ.
        assert_eq!(
            capture_hotkey(0x2C, false, false, false, false),
            Capture::Got(Hotkey::default())
        );
        assert_eq!(
            capture_hotkey(0x53, false, false, false, false),
            Capture::NeedModifier
        );
        assert_eq!(
            capture_hotkey(0x53, true, false, true, false),
            Capture::Got(Hotkey {
                ctrl: true,
                alt: false,
                shift: true,
                win: false,
                vk: 0x53
            })
        );
        // Chỉ phím bổ trợ: bỏ qua.
        assert_eq!(
            capture_hotkey(0x11, true, false, false, false),
            Capture::Ignore
        );
        assert_eq!(
            capture_hotkey(0x5B, false, false, false, true),
            Capture::Ignore
        );
        // Shift + chữ không đủ.
        assert_eq!(
            capture_hotkey(0x41, false, false, true, false),
            Capture::NeedModifier
        );
        // F-key đứng một mình được phép.
        assert!(matches!(
            capture_hotkey(0x71, false, false, false, false),
            Capture::Got(_)
        ));
    }

    fn dn(vk: u32, ctrl: bool, alt: bool, shift: bool, win: bool) -> KeyEv {
        KeyEv::Down {
            vk,
            ctrl,
            alt,
            shift,
            win,
        }
    }
    fn up(vk: u32, ctrl: bool, alt: bool, shift: bool, win: bool) -> KeyEv {
        KeyEv::Up {
            vk,
            ctrl,
            alt,
            shift,
            win,
        }
    }
    fn hk(ctrl: bool, alt: bool, shift: bool, win: bool, vk: u32) -> Capture {
        Capture::Got(Hotkey {
            ctrl,
            alt,
            shift,
            win,
            vk,
        })
    }

    #[test]
    fn ctrl_released_before_main_key_still_gives_full_combo() {
        let mut st = CaptureState::new();
        // Nhấn Ctrl, Shift (bổ trợ: bỏ qua), rồi S với cả hai đang giữ.
        assert_eq!(
            st.feed(dn(0x11, true, false, false, false)),
            Capture::Ignore
        );
        assert_eq!(st.feed(dn(0x10, true, false, true, false)), Capture::Ignore);
        assert_eq!(
            st.feed(dn(0x53, true, false, true, false)),
            hk(true, false, true, false, 0x53)
        );
        // Ctrl nhả trước S: KEYUP không đổi kết quả.
        assert_eq!(
            st.feed(up(0x11, false, false, true, false)),
            Capture::Ignore
        );
        assert_eq!(
            st.feed(up(0x53, false, false, true, false)),
            Capture::Ignore
        );
    }

    #[test]
    fn orphan_keyup_is_ignored() {
        let mut st = CaptureState::new();
        assert_eq!(
            st.feed(up(0x0D, false, false, false, false)),
            Capture::Ignore
        );
        assert_eq!(
            st.feed(up(0x20, false, false, false, false)),
            Capture::Ignore
        );
    }

    #[test]
    fn printscreen_keyup_only_is_accepted() {
        let mut st = CaptureState::new();
        assert_eq!(
            st.feed(up(0x2C, false, false, false, false)),
            Capture::Got(Hotkey::default())
        );
    }

    #[test]
    fn shift_plus_letter_rejected_but_other_modifiers_accepted() {
        let mut st = CaptureState::new();
        assert_eq!(
            st.feed(dn(0x41, false, false, true, false)),
            Capture::NeedModifier
        );
        assert_eq!(
            st.feed(dn(0x41, true, false, false, false)),
            hk(true, false, false, false, 0x41)
        );
        assert_eq!(
            st.feed(dn(0x41, false, true, false, false)),
            hk(false, true, false, false, 0x41)
        );
        assert_eq!(
            st.feed(dn(0x41, false, false, false, true)),
            hk(false, false, false, true, 0x41)
        );
    }

    #[test]
    fn function_keys_alone_or_with_shift_accepted() {
        let mut st = CaptureState::new();
        assert_eq!(
            st.feed(dn(0x74, false, false, true, false)),
            hk(false, false, true, false, 0x74)
        );
        assert_eq!(
            st.feed(dn(0x74, false, false, false, false)),
            hk(false, false, false, false, 0x74)
        );
    }

    #[test]
    fn alt_f4_is_never_a_combo() {
        let mut st = CaptureState::new();
        assert_eq!(
            st.feed(dn(0x73, false, true, false, false)),
            Capture::PassThrough
        );
        assert_eq!(
            capture_hotkey(0x73, true, true, false, false),
            Capture::PassThrough
        );
    }

    /// Cửa sổ cha ẩn riêng của một kiểm thử cửa sổ thật.
    fn test_parent(class: PCWSTR) -> HWND {
        unsafe extern "system" fn proc_(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
            // SAFETY: chuyển nguyên tham số.
            unsafe { DefWindowProcW(h, m, w, l) }
        }
        // SAFETY: đăng ký lớp và tạo cửa sổ ẩn ở luồng kiểm thử; bên gọi hủy nó.
        unsafe {
            let hinst = GetModuleHandleW(None).unwrap();
            let wc = WNDCLASSW {
                lpfnWndProc: Some(proc_),
                hInstance: hinst.into(),
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassW(&wc);
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                w!("parent"),
                WS_CAPTION,
                0,
                0,
                10,
                10,
                None,
                None,
                Some(hinst.into()),
                None,
            )
            .unwrap()
        }
    }

    /// Chờ cửa sổ Tùy chọn hiện (tối đa 5 s); trả HWND dưới dạng số để gửi qua luồng. Cửa sổ chỉ hiện
    /// sau khi đã dựng xong mọi điều khiển, nên chờ cờ hiện chứ không chỉ chờ cửa sổ có mặt.
    fn wait_settings_window() -> isize {
        use windows::Win32::System::Threading::GetCurrentProcessId;
        use windows::Win32::UI::WindowsAndMessaging::{
            FindWindowExW, GetWindowThreadProcessId, IsWindowVisible,
        };
        for _ in 0..100 {
            // Chỉ nhận cửa sổ của tiến trình kiểm thử, không bao giờ đụng cửa sổ Tùy chọn của darkshot thật.
            let mut prev: Option<HWND> = None;
            // SAFETY: chỉ duyệt cửa sổ cấp cao theo tên lớp, đọc mã tiến trình và cờ hiện.
            while let Ok(h) = unsafe { FindWindowExW(None, prev, w!("darkshot-settings"), None) } {
                let mut pid = 0u32;
                unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
                if pid == unsafe { GetCurrentProcessId() }
                    && unsafe { IsWindowVisible(h) }.as_bool()
                {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    return h.0 as isize;
                }
                prev = Some(h);
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        panic!("settings window not found");
    }

    /// Dải tab của cửa sổ Tùy chọn `h`.
    fn find_tabs(h: HWND) -> HWND {
        // SAFETY: chỉ tìm cửa sổ con theo tên lớp.
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::FindWindowExW(
                Some(h),
                None,
                w!("SysTabControl32"),
                None,
            )
            .expect("tab control not found")
        }
    }

    /// Chọn tab `i` như khi người dùng bấm chuột: đặt lựa chọn rồi báo TCN_SELCHANGE cho cửa sổ.
    fn click_tab(h: HWND, tabs: HWND, i: usize) {
        let nm = NMHDR {
            hwndFrom: tabs,
            idFrom: ID_TABS as usize,
            code: TCN_SELCHANGE,
        };
        // SAFETY: SendMessageW chờ luồng cửa sổ xử lý xong, nên `nm` sống suốt lời gọi.
        unsafe {
            SendMessageW(tabs, TCM_SETCURSEL, Some(WPARAM(i)), None);
            SendMessageW(
                h,
                WM_NOTIFY,
                Some(WPARAM(ID_TABS as usize)),
                Some(LPARAM(&nm as *const NMHDR as isize)),
            );
        }
    }

    /// Vẽ lại ngay toàn bộ cửa sổ `h` (kể cả viền ô nhập và mọi cửa sổ con).
    fn repaint_now(h: HWND) {
        use windows::Win32::Graphics::Gdi::{
            RedrawWindow, RDW_ALLCHILDREN, RDW_ERASE, RDW_FRAME, RDW_INVALIDATE, RDW_UPDATENOW,
        };
        // SAFETY: chỉ yêu cầu vẽ lại cửa sổ của tiến trình này.
        unsafe {
            let _ = RedrawWindow(
                Some(h),
                None,
                None,
                RDW_INVALIDATE | RDW_ERASE | RDW_FRAME | RDW_ALLCHILDREN | RDW_UPDATENOW,
            );
        }
    }

    fn gdi_count() -> u32 {
        use windows::Win32::System::Threading::{
            GetCurrentProcess, GetGuiResources, GR_GDIOBJECTS,
        };
        // SAFETY: chỉ đọc bộ đếm của tiến trình hiện tại.
        unsafe { GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS) }
    }

    /// Mở và đóng Tùy chọn 20 lần, mỗi lần đi qua đủ bốn tab và vẽ lại toàn bộ: số đối tượng GDI sau lần
    /// cuối không lớn hơn sau lần đầu. Cần màn hình thật.
    /// Chạy tay: `cargo test -p darkshot-windows -- --ignored options_no_gdi_leak`.
    #[test]
    #[ignore]
    fn options_no_gdi_leak() {
        let parent = test_parent(w!("darkshot-settings-gdi-parent"));
        let cfg = Config::defaults(
            std::path::Path::new("C:/darkshot-test-not-created"),
            Lang::Vi,
        );
        let mut counts = Vec::new();
        for _ in 0..20 {
            let closer = std::thread::spawn(|| {
                let h = HWND(wait_settings_window() as *mut c_void);
                let tabs = find_tabs(h);
                for i in [1, 2, 3, 0] {
                    click_tab(h, tabs, i);
                    repaint_now(h);
                    std::thread::sleep(std::time::Duration::from_millis(30));
                }
                // SAFETY: đăng thông điệp đóng cho cửa sổ Tùy chọn của tiến trình này.
                unsafe {
                    PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0)).unwrap();
                }
            });
            assert!(show(parent, &cfg, None, OpenReason::User).is_none());
            closer.join().unwrap();
            counts.push(gdi_count());
        }
        assert!(
            counts[19] <= counts[0],
            "GDI objects grew across opens: {counts:?}"
        );
        // SAFETY: dọn cửa sổ cha.
        unsafe {
            let _ = DestroyWindow(parent);
        }
    }

    /// Bàn phím vẫn đi được qua các điều khiển tự vẽ: Tab đi đúng thứ tự cũ trên tab Thông thường, Space
    /// bật tắt ô tích, Ctrl+Tab đổi tab. Cần màn hình thật; phím Ctrl giả bằng bảng phím của luồng cửa sổ.
    /// Chạy tay: `cargo test -p darkshot-windows -- --ignored options_keyboard_still_works`.
    #[test]
    #[ignore]
    fn options_keyboard_still_works() {
        use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            GetKeyboardState, SetKeyboardState, VK_SPACE, VK_TAB,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            GetDlgCtrlID, GetGUIThreadInfo, GUITHREADINFO,
        };
        let parent = test_parent(w!("darkshot-settings-kbd-parent"));
        let cfg = Config::defaults(
            std::path::Path::new("C:/darkshot-test-not-created"),
            Lang::Vi,
        );
        // SAFETY: chỉ đọc mã luồng hiện tại.
        let gui_tid = unsafe { GetCurrentThreadId() };
        let checker = std::thread::spawn(move || {
            let h = HWND(wait_settings_window() as *mut c_void);
            let pause = || std::thread::sleep(std::time::Duration::from_millis(80));
            let focus = || {
                let mut gi = GUITHREADINFO {
                    cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                    ..Default::default()
                };
                // SAFETY: gi có cbSize đúng; chỉ đọc trạng thái luồng cửa sổ.
                unsafe { GetGUIThreadInfo(gui_tid, &mut gi).unwrap() };
                gi.hwndFocus
            };
            // SAFETY: các HWND thuộc cửa sổ Tùy chọn của tiến trình này; chỉ đăng thông điệp phím.
            let post_key = |target: HWND, vk: u16| unsafe {
                PostMessageW(Some(target), WM_KEYDOWN, WPARAM(vk as usize), LPARAM(1)).unwrap();
                PostMessageW(
                    Some(target),
                    WM_KEYUP,
                    WPARAM(vk as usize),
                    LPARAM(0xC000_0001u32 as isize),
                )
                .unwrap();
            };
            pause();
            // SAFETY: chỉ đọc mã điều khiển.
            let id_of = |h: HWND| unsafe { GetDlgCtrlID(h) };
            let mut seen = vec![id_of(focus())];
            for _ in 0..14 {
                post_key(focus(), VK_TAB.0);
                pause();
                seen.push(id_of(focus()));
            }
            let want = vec![
                ID_AUTOSTART,
                ID_NOTICES,
                ID_CHECK_UPDATES,
                ID_HISTORY,
                ID_KEEP_SELECTION,
                ID_INCLUDE_CURSOR,
                ID_SNAP_WINDOWS,
                ID_DIR,
                ID_BROWSE,
                ID_LANGUAGE,
                ID_HELP,
                ID_OK,
                ID_CANCEL,
                ID_TABS,
                ID_AUTOSTART,
            ];
            let tab_order_ok = seen == want;

            // Space trên ô tích đang có tiêu điểm đổi trạng thái.
            let chk = focus();
            let before = is_checked(chk);
            post_key(chk, VK_SPACE.0);
            pause();
            let space_ok = is_checked(chk) != before;

            // Ctrl+Tab: giữ Ctrl trong bảng phím chung của hai luồng rồi gửi Tab.
            let tabs = find_tabs(h);
            // SAFETY: gắn hàng đợi nhập của luồng này với luồng cửa sổ, đặt rồi trả lại bảng phím.
            let sel = unsafe {
                let me = GetCurrentThreadId();
                let _ = AttachThreadInput(me, gui_tid, true);
                let mut keys = [0u8; 256];
                let _ = GetKeyboardState(&mut keys);
                let saved = keys;
                keys[VK_CONTROL.0 as usize] = 0x80;
                let _ = SetKeyboardState(&keys);
                PostMessageW(
                    Some(focus()),
                    WM_KEYDOWN,
                    WPARAM(VK_TAB.0 as usize),
                    LPARAM(1),
                )
                .unwrap();
                pause();
                let sel = SendMessageW(tabs, TCM_GETCURSEL, None, None).0;
                let _ = SetKeyboardState(&saved);
                let _ = AttachThreadInput(me, gui_tid, false);
                sel
            };
            // SAFETY: đăng thông điệp đóng cho cửa sổ Tùy chọn của tiến trình này.
            unsafe {
                PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0)).unwrap();
            }
            (seen, tab_order_ok, space_ok, sel)
        });
        assert!(show(parent, &cfg, None, OpenReason::User).is_none());
        let (seen, tab_order_ok, space_ok, sel) = checker.join().unwrap();
        assert!(tab_order_ok, "tab order changed: {seen:?}");
        assert!(space_ok, "Space must toggle the focused check box");
        assert_eq!(
            sel,
            Tab::Hotkeys.index() as isize,
            "Ctrl+Tab must go to the next tab"
        );
        // SAFETY: dọn cửa sổ cha.
        unsafe {
            let _ = DestroyWindow(parent);
        }
    }

    /// Mở cửa sổ thật, đóng bằng WM_CLOSE rồi bằng nút Hủy. Không ghi đĩa, không gọi mạng.
    /// Chạy tay: `cargo test -p darkshot-windows -- --ignored settings_window`.
    #[test]
    #[ignore]
    fn settings_window_opens_and_cancels() {
        use windows::Win32::UI::WindowsAndMessaging::{
            DefWindowProcW as Def, FindWindowW, IsWindow, WM_COMMAND,
        };
        unsafe extern "system" fn proc_(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
            // SAFETY: chuyển nguyên tham số.
            unsafe { Def(h, m, w, l) }
        }
        // Cửa sổ cha ẩn riêng của kiểm thử (phím tắt toàn cục gắn với nó, không đụng ứng dụng thật).
        let parent = unsafe {
            let hinst = GetModuleHandleW(None).unwrap();
            let class = w!("darkshot-settings-test-parent");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(proc_),
                hInstance: hinst.into(),
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassW(&wc);
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                w!("parent"),
                WS_CAPTION,
                0,
                0,
                10,
                10,
                None,
                None,
                Some(hinst.into()),
                None,
            )
            .unwrap()
        };
        let cfg = Config::defaults(
            std::path::Path::new("C:/darkshot-test-not-created"),
            Lang::Vi,
        );
        for (use_command, reason) in [
            (false, OpenReason::User),
            (true, OpenReason::NeedsR2),
            (true, OpenReason::User),
            (false, OpenReason::NeedsR2),
        ] {
            let closer = std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(500));
                // SAFETY: tìm cửa sổ theo tên lớp, hỏi tab đang chọn rồi đăng thông điệp; HWND chỉ dùng như số.
                let sel = unsafe {
                    let h = FindWindowW(w!("darkshot-settings"), None)
                        .expect("settings window not found");
                    let tab = windows::Win32::UI::WindowsAndMessaging::FindWindowExW(
                        Some(h),
                        None,
                        w!("SysTabControl32"),
                        None,
                    )
                    .expect("tab control not found");
                    let sel =
                        SendMessageW(tab, windows::Win32::UI::Controls::TCM_GETCURSEL, None, None)
                            .0;
                    // Mép phải của tab cuối và bề rộng dải tab: tab cuối phải nằm trọn trong dải.
                    let mut last = RECT::default();
                    SendMessageW(
                        tab,
                        windows::Win32::UI::Controls::TCM_GETITEMRECT,
                        Some(WPARAM(Tab::ALL.len() - 1)),
                        Some(LPARAM(&mut last as *mut RECT as isize)),
                    );
                    let mut client = RECT::default();
                    let _ = GetClientRect(tab, &mut client);
                    let rows = (last.right, client.right);
                    if use_command {
                        PostMessageW(Some(h), WM_COMMAND, WPARAM(ID_CANCEL as usize), LPARAM(0))
                            .unwrap();
                    } else {
                        PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0)).unwrap();
                    }
                    (sel, rows)
                };
                let (sel, rows) = sel;
                let want = match initial_tab(reason) {
                    Tab::General => 0,
                    Tab::Hotkeys => 1,
                    Tab::Format => 2,
                    Tab::R2 => 3,
                };
                assert_eq!(sel, want, "{reason:?}");
                // Bốn tab nằm trọn trên một hàng (không cần nút cuộn của dải tab).
                let (last_right, strip_right) = rows;
                assert!(
                    last_right > 0 && last_right <= strip_right,
                    "{reason:?}: {last_right} > {strip_right}"
                );
            });
            // Giả lập lần Kiểm tra kết nối trước bị bỏ dở (Hủy ngay sau khi bấm): cờ kẹt ở true.
            TEST_RUNNING.with(|r| r.set(true));
            assert!(show(parent, &cfg, Some("Thông báo thử"), reason).is_none());
            assert!(
                !TEST_RUNNING.with(|r| r.get()),
                "TEST_RUNNING must be cleared when the window closes"
            );
            closer.join().unwrap();
            // SAFETY: tìm lại cửa sổ sau khi show trả về.
            assert!(unsafe { FindWindowW(w!("darkshot-settings"), None) }.is_err());
        }
        // SAFETY: dọn cửa sổ cha.
        unsafe {
            assert!(IsWindow(Some(parent)).as_bool());
            let _ = DestroyWindow(parent);
        }
    }
}
