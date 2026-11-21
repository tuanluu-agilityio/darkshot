mod about;
mod annotate_draw;
mod app;
mod autostart;
mod capture;
mod clipboard;
mod gdiplus;
mod help;
mod history_layout;
mod history_paint;
mod history_service;
mod history_window;
mod hotkey;
mod lang;
mod legacy;
mod ocr;
mod ocr_window;
mod overlay;
mod paths;
mod pin;
mod secret;
mod settings;
mod snap;
mod steps_bar;
mod steps_export;
mod steps_store;
mod steps_window;
mod theme;
mod tray;
mod wer;
mod winutil;

use app::{AppState, UploadJob};
use darkshot_core::actions::{
    notice_visible, run_action, Action, Clipboard, Ctx, NoticeKind, Notifier,
};
use darkshot_core::app_flow::{
    config_ok_at_start, custom_colors_need_save, history_window_stale, hotkey_defers,
    info_windows_stale, lang_after_save, merge_reason, parse_args, pin_origin, remaining,
    timer_should_wait, without_frame, OpenReason, EXIT_BUDGET,
};
use darkshot_core::capture_gate::{
    enumerates_windows, gate_step, kind_of, next_kind, CaptureGate, CaptureKind, GateAction,
    GateEvent,
};
use darkshot_core::color::CustomColors;
use darkshot_core::config::{HotkeyRole, Hotkeys, UploadFormat, JPEG_QUALITY_DEFAULT};
use darkshot_core::geometry::{full_screen_rect, restore_selection, to_screen_coords, Rect};
use darkshot_core::i18n::{self, Msg};
use darkshot_core::last_capture::{
    last_capture_after, read_last_capture, remember_after, repeat_plan, write_last_capture,
    LastCapture, RepeatPlan,
};
use darkshot_core::notices::{hotkeys_taken_notice, pin_failure_notice, step_error_notice};
use darkshot_core::update::{
    self, update_after_check, update_outcome, Release, Version, RELEASE_URL_PREFIX,
};
use darkshot_core::upload::{R2Uploader, Uploader};
use secret::DpapiStore;
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tray::{MenuCommand, Notice, Tray, WM_NOTICE, WM_TRAY};
use windows::core::{w, HSTRING};
use windows::Win32::Foundation::{
    GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::Com::{
    CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Shell::{ShellExecuteW, NIN_BALLOONUSERCLICK};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EndMenu, GetMessageW,
    KillTimer, PostMessageW, PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SetTimer,
    TranslateMessage, CW_USEDEFAULT, MSG, SW_SHOWNORMAL, WM_APP, WM_DESTROY, WM_ENDSESSION,
    WM_HOTKEY, WM_LBUTTONUP, WM_QUERYENDSESSION, WM_RBUTTONUP, WM_TIMER, WNDCLASSW,
    WS_EX_TOOLWINDOW, WS_OVERLAPPED,
};

/// Sự kiện nội bộ; Task 11 mở rộng.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppEvent {
    /// Phím tắt chụp (một trong các loại).
    Capture(CaptureKind),
    OpenSettings,
    Quit,
}

/// Nhớ lượt chụp vùng vừa kết thúc bằng `action` với vùng `screen` (tọa độ màn hình); giá trị đổi
/// thì ghi tệp. Lỗi ghi bị nuốt: không bao giờ làm hỏng luồng chụp.
fn remember_last_capture(action: Action, screen: Rect) {
    let prev = LAST_CAPTURE.with(|l| l.get());
    let on_disk = LAST_CAPTURE_ON_DISK.with(|l| l.get());
    let (next, write) = last_capture_after(prev, on_disk, action, screen);
    LAST_CAPTURE.with(|l| l.set(next));
    if let (true, Some(l)) = (write, next) {
        if write_last_capture(&paths::last_capture_path(), &l).is_ok() {
            LAST_CAPTURE_ON_DISK.with(|d| d.set(Some(l)));
        }
    }
}

/// Sự kiện chuột `ev` (từ `WM_TRAY`) có mở menu khay không. Menu đang mở thì không mở lồng.
pub fn tray_click_opens_menu(ev: u32, menu_open: bool) -> bool {
    !menu_open && (ev == WM_RBUTTONUP || ev == WM_LBUTTONUP)
}

thread_local! {
    static TRAY: RefCell<Option<Rc<Tray>>> = const { RefCell::new(None) };
    /// Cửa sổ ẩn của ứng dụng (đích của Lưu/Chép từ ảnh ghim).
    static MAIN_HWND: Cell<Option<HWND>> = const { Cell::new(None) };
    static TASKBAR_CREATED: Cell<u32> = const { Cell::new(0) };
    static ADD_ATTEMPTS: Cell<u32> = const { Cell::new(0) };
    static APP: RefCell<Option<AppState>> = const { RefCell::new(None) };
    static GATE: Cell<CaptureGate> = const { Cell::new(CaptureGate::Idle) };
    /// Loại của lượt chụp đã được cổng nhận (đang chờ menu hoặc đang chạy); chỉ `request` đổi nó.
    static CAPTURE_KIND: Cell<CaptureKind> = const { Cell::new(CaptureKind::Region) };
    /// Khung đã chọn ở lượt chụp vùng gần nhất (tọa độ màn hình); lúc khởi động lấy từ tệp lần chụp trước.
    static LAST_SELECTION: Cell<Option<Rect>> = const { Cell::new(None) };
    /// Lần chụp vùng trước (vùng tọa độ màn hình và hành động) cho "Chụp lại vùng trước"; đọc từ
    /// tệp lúc khởi động.
    static LAST_CAPTURE: Cell<Option<LastCapture>> = const { Cell::new(None) };
    /// Giá trị đã nằm trên đĩa (đọc lúc khởi động hoặc ghi thành công); ghi lỗi thì lần sau thử lại.
    static LAST_CAPTURE_ON_DISK: Cell<Option<LastCapture>> = const { Cell::new(None) };
    /// Lý do của yêu cầu mở Tùy chọn đang chờ (gộp bằng `merge_reason`); `run_settings` lấy ra.
    static OPEN_REASON: Cell<OpenReason> = const { Cell::new(OpenReason::User) };
    /// Cấu hình trong bộ nhớ khớp file trên đĩa (đọc bình thường lúc khởi động, hoặc Tùy chọn đã
    /// lưu thành công): chỉ khi đó màu tự chọn mới được ghi ra file.
    static CONFIG_WRITABLE: Cell<bool> = const { Cell::new(false) };
    /// Menu khay đang hiện.
    static MENU_OPEN: Cell<bool> = const { Cell::new(false) };
    /// Một lần kiểm tra bản mới đang chạy ở luồng nền.
    static UPDATE_CHECKING: Cell<bool> = const { Cell::new(false) };
    /// Sổ các lượt tải lên đang chạy; luồng tải lên giữ bản sao `Arc`.
    static PENDING: Arc<app::PendingUploads> = Arc::new(app::PendingUploads::new());
}

/// Mở trang tải về bằng trình duyệt mặc định. Chỉ mở link thuộc repo darkshot; lỗi thì bỏ qua.
fn open_release_url(url: &str) {
    if !url.starts_with(RELEASE_URL_PREFIX) {
        return;
    }
    let url = HSTRING::from(url);
    // SAFETY: các chuỗi sống đến hết lời gọi; ShellExecuteW không giữ con trỏ. COM được khởi
    // tạo (nếu được) rồi giải phóng cân bằng quanh lời gọi, vì ShellExecuteW cần COM ở luồng gọi.
    unsafe {
        let inited =
            CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).is_ok();
        let _ = ShellExecuteW(None, w!("open"), &url, None, None, SW_SHOWNORMAL);
        if inited {
            CoUninitialize();
        }
    }
}

fn running_version() -> Option<Version> {
    Version::parse(env!("CARGO_PKG_VERSION"))
}

/// Hẹn giờ kiểm tra bản mới: chạy lần đầu sau 60 giây.
fn start_update_timer(hwnd: HWND) {
    if !side_effects_allowed() {
        return;
    }
    // SAFETY: bộ hẹn giờ gắn với cửa sổ của luồng này; WM_TIMER xử lý ở wndproc.
    unsafe {
        SetTimer(Some(hwnd), UPDATE_TIMER_ID, UPDATE_FIRST_DELAY_MS, None);
    }
}

fn stop_update_timer(hwnd: HWND) {
    // SAFETY: hủy bộ hẹn giờ của chính cửa sổ này (không có thì lỗi, bỏ qua).
    unsafe {
        let _ = KillTimer(Some(hwnd), UPDATE_TIMER_ID);
    }
    with_app(|a| a.update = None);
}

/// Hẹn giờ kiểm tra chạy: đặt lại 24 giờ rồi hỏi GitHub ở luồng nền (không chặn giao diện).
fn on_update_timer(hwnd: HWND) {
    let enabled = with_app(|a| a.config.check_updates).unwrap_or(false);
    if !enabled {
        stop_update_timer(hwnd);
        return;
    }
    // SAFETY: đặt lại bộ hẹn giờ của chính cửa sổ này.
    unsafe {
        SetTimer(Some(hwnd), UPDATE_TIMER_ID, UPDATE_INTERVAL_MS, None);
    }
    let Some(current) = running_version() else {
        return;
    };
    if UPDATE_CHECKING.with(|c| c.replace(true)) {
        return;
    }
    let raw_hwnd = hwnd.0 as isize;
    let spawned = std::thread::Builder::new()
        .name("update-check".into())
        .spawn(move || {
            let found =
                update::newer_than(current, update::fetch_latest(update::LATEST_URL, current));
            let raw = Box::into_raw(Box::new(found));
            // SAFETY: con trỏ chỉ được giải phóng đúng một lần: ở đây nếu gửi thất bại, hoặc ở
            // luồng chính khi xử lý WM_UPDATE_RESULT.
            let posted = unsafe {
                PostMessageW(
                    Some(HWND(raw_hwnd as *mut c_void)),
                    WM_UPDATE_RESULT,
                    WPARAM(0),
                    LPARAM(raw as isize),
                )
            };
            if posted.is_err() {
                // SAFETY: tin nhắn không vào hàng đợi nên raw vẫn thuộc về ta.
                drop(unsafe { Box::from_raw(raw) });
            }
        });
    if spawned.is_err() {
        UPDATE_CHECKING.with(|c| c.set(false));
    }
}

/// Kết quả kiểm tra về tới luồng chính. Bỏ qua nếu ô kiểm tra đã tắt.
fn on_update_result(found: Option<Release>) {
    UPDATE_CHECKING.with(|c| c.set(false));
    let Some((enabled, notified)) =
        with_app(|a| (a.config.check_updates, a.config.update_notified))
    else {
        return;
    };
    if !enabled {
        return;
    }
    let (_, notify) = update_outcome(found.as_ref(), notified);
    with_app(|a| a.update = update_after_check(a.update.take(), found.clone()));
    let (Some(rel), true) = (found, notify) else {
        return;
    };
    if let Some(t) = current_tray() {
        let v = rel.version.to_string();
        let lang = lang::current();
        // Thông tin, không qua bộ lọc "thông báo thành công".
        t.show_update_notice(&Notice {
            kind: NoticeKind::Info,
            title: i18n::t(lang, Msg::UpdateNoticeTitle).to_string(),
            body: i18n::update_notice_body(lang, &v),
        });
    }
    // Nhớ trong bộ nhớ; ghi file chỉ khi cấu hình khớp file (quyết định 72), lỗi thì bỏ qua.
    let write = CONFIG_WRITABLE.with(|w| w.get());
    let cfg = with_app(|a| {
        a.config.update_notified = Some(rel.version);
        a.config.clone()
    });
    if let (Some(cfg), true) = (cfg, write) {
        let _ = darkshot_core::config::save(&paths::config_path(), &cfg, &DpapiStore);
    }
}

/// Những việc khởi động tùy theo chế độ cách ly (`DARKSHOT_PROFILE_DIR`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupMode {
    pub sandbox: bool,
    pub mutex_name: String,
    /// Chuyển dữ liệu của bản cũ.
    pub run_legacy: bool,
    /// Đồng bộ và ghi khóa Run.
    pub sync_autostart: bool,
    /// Loại khỏi Windows Error Reporting.
    pub exclude_wer: bool,
    /// Hẹn giờ kiểm tra bản mới (gọi mạng).
    pub update_timer: bool,
}

const MUTEX_NAME: &str = "Local\\darkshot-single-instance";

/// Băm FNV-1a 64 bit của đường dẫn (chữ thường): tên mutex ổn định cho mỗi thư mục cách ly.
fn short_hash(p: &Path) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in p.to_string_lossy().to_lowercase().bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Chế độ khởi động: có thư mục cách ly thì mutex riêng và bỏ mọi tác dụng phụ ra ngoài thư mục đó.
pub fn startup_mode(profile: Option<&Path>) -> StartupMode {
    match profile {
        None => StartupMode {
            sandbox: false,
            mutex_name: MUTEX_NAME.to_string(),
            run_legacy: true,
            sync_autostart: true,
            exclude_wer: true,
            update_timer: true,
        },
        Some(dir) => StartupMode {
            sandbox: true,
            mutex_name: format!("{MUTEX_NAME}-sandbox-{}", short_hash(dir)),
            run_legacy: false,
            sync_autostart: false,
            exclude_wer: false,
            update_timer: false,
        },
    }
}

/// Sai ở chế độ cách ly: không hẹn giờ cập nhật (không mạng), không ghi khóa Run.
static SIDE_EFFECTS_ALLOWED: AtomicBool = AtomicBool::new(true);

fn side_effects_allowed() -> bool {
    SIDE_EFFECTS_ALLOWED.load(Ordering::SeqCst)
}

/// Lưu vào máy ảnh của mọi lượt tải lên chưa xong, rồi chờ hàng ghi lịch sử. Chạy trên luồng
/// chính, chỉ ghi file, không mạng. Thứ tự: `drain` (giành mọi lượt còn dở; lượt nào đã được trao
/// cho luồng tải lên thì móc của nó đã đẩy việc vào hàng) → lưu ảnh, đẩy mục dự phòng vào CUỐI
/// hàng ghi (mới hơn mọi mục Lưu/Chép còn trong hàng) → `flush` một lần (tối đa `timeout`).
/// Gọi lại lần hai thì vô hại: sổ rỗng, hàng rỗng.
fn save_pending_uploads(timeout: Duration) {
    let items = PENDING.with(|p| p.drain());
    let now = paths::local_now();
    // Hết giờ thì bỏ: không treo khi thoát.
    let _ = history_service::finish_on_exit(now, timeout, |record| {
        if !items.is_empty() {
            app::save_pending(items, now, record);
        }
    });
}

/// Luồng nền (hoặc lúc khởi động) nhờ luồng chính mở Cài đặt.
const WM_OPEN_SETTINGS: u32 = WM_APP + 3;
/// Kết quả kiểm tra bản mới từ luồng nền: `lparam` là `Box<Option<Release>>` đã chuyển quyền sở hữu.
const WM_UPDATE_RESULT: u32 = WM_APP + 4;
/// Mở cửa sổ Lịch sử (cờ `--history` lúc khởi động).
const WM_OPEN_HISTORY: u32 = WM_APP + 5;
/// Nút Chụp bước của thanh phiên (WM_APP+13 là `steps_store::WM_STEPS_SAVED`).
const WM_STEPS_CAPTURE: u32 = WM_APP + 14;
const RETRY_TIMER_ID: usize = 1;
const UPDATE_TIMER_ID: usize = 3;
const UPDATE_FIRST_DELAY_MS: u32 = 60_000;
const UPDATE_INTERVAL_MS: u32 = 24 * 60 * 60 * 1000;
const MENU_CAPTURE_TIMER_ID: usize = 2;
const MENU_CAPTURE_DELAY_MS: u32 = 250;
const RETRY_INTERVAL_MS: u32 = 2000;

fn on_retry_timer(hwnd: HWND) {
    let attempts = ADD_ATTEMPTS.with(|c| c.get());
    let added = match current_tray() {
        Some(t) => {
            if tray::should_retry(t.is_added(), attempts) {
                ADD_ATTEMPTS.with(|c| c.set(attempts + 1));
                t.try_add()
            } else {
                t.is_added()
            }
        }
        None => true,
    };
    let attempts = ADD_ATTEMPTS.with(|c| c.get());
    if !tray::should_retry(added, attempts) {
        // SAFETY: hủy bộ hẹn giờ của chính cửa sổ này.
        unsafe {
            let _ = KillTimer(Some(hwnd), RETRY_TIMER_ID);
        }
    }
}

fn current_tray() -> Option<Rc<Tray>> {
    TRAY.with(|t| t.borrow().clone())
}

/// Nhờ luồng chính mở Tùy chọn; `wp` như `WM_OPEN_SETTINGS` (0: ứng dụng tự mở, 1: người dùng).
fn post_open_settings(hwnd: isize, wp: usize) {
    // SAFETY: PostMessageW an toàn từ luồng bất kỳ; cửa sổ đã hủy thì lỗi và ta bỏ qua.
    unsafe {
        let _ = PostMessageW(
            Some(HWND(hwnd as *mut c_void)),
            WM_OPEN_SETTINGS,
            WPARAM(wp),
            LPARAM(0),
        );
    }
}

/// Ghi lý do của một yêu cầu mở Tùy chọn, gộp với yêu cầu đang chờ.
fn note_open_reason(reason: OpenReason) {
    OPEN_REASON.with(|r| r.set(merge_reason(r.get(), reason)));
}

fn with_app<R>(f: impl FnOnce(&mut AppState) -> R) -> Option<R> {
    APP.with(|a| a.borrow_mut().as_mut().map(f))
}

/// Lớp phủ chọn vùng đang mở. Trạng thái đang bận (không xảy ra ngoài `with_app`) thì coi là đóng.
pub fn overlay_open() -> bool {
    APP.with(|a| {
        a.try_borrow()
            .ok()
            .and_then(|g| g.as_ref().map(|s| s.overlay_open))
            .unwrap_or(false)
    })
}

/// Lịch sử đang bật theo cấu hình hiện hành (`Config.history_enabled`). Cấu hình đang bận hoặc chưa
/// nạp thì theo cờ của dịch vụ ghi lịch sử (luôn khớp cấu hình).
pub fn history_enabled_now() -> bool {
    APP.with(|a| {
        a.try_borrow()
            .ok()
            .and_then(|g| g.as_ref().map(|s| s.config.history_enabled))
    })
    .unwrap_or_else(history_service::is_enabled)
}

/// Cửa sổ Lịch sử nhờ mở Tùy chọn như mục menu khay (`wParam` = 1: người dùng tự mở).
pub fn post_open_options(owner: HWND) {
    // SAFETY: PostMessageW tới cửa sổ chính của luồng này; lỗi thì bỏ qua.
    unsafe {
        let _ = PostMessageW(Some(owner), WM_OPEN_SETTINGS, WPARAM(1), LPARAM(0));
    }
}

/// Đăng ký các dòng đang bật của `keys` (dòng tắt không bao giờ được đăng ký); dòng nào không đăng
/// ký được thì báo **một** thông báo lỗi liệt kê mọi tổ hợp bị chiếm.
fn register_hotkeys_or_notify(hwnd: HWND, keys: &Hotkeys, notifier: &dyn Notifier) {
    let failed = hotkey::register_enabled(hwnd, keys);
    notify_hotkeys_taken(keys, &failed, notifier);
}

/// Báo **một** thông báo lỗi cho các dòng `failed` (nếu có).
fn notify_hotkeys_taken(keys: &Hotkeys, failed: &[HotkeyRole], notifier: &dyn Notifier) {
    if let Some((title, body)) = hotkeys_taken_notice(lang::current(), keys, failed) {
        notifier.notify(NoticeKind::Error, title, &body);
    }
}

/// Chụp theo loại `CAPTURE_KIND` rồi thực hiện hành động: chụp vùng mở lớp phủ, hai loại toàn
/// màn hình cắt lấy màn hình chứa con trỏ. Chỉ gọi qua `request` (trạng thái `Capturing`), nên
/// không bao giờ có hai lớp phủ; mọi đường ra đều trả về để `request` đóng cổng.
fn capture_flow(hwnd: HWND) {
    let notifier: Option<Arc<dyn Notifier>> =
        current_tray().map(|t| Arc::new(t.notifier()) as Arc<dyn Notifier>);
    capture_flow_with(hwnd, CAPTURE_KIND.with(|k| k.get()), notifier);
}

/// Thân của `capture_flow` với loại `kind` và bộ thông báo truyền vào.
fn capture_flow_with(hwnd: HWND, requested: CaptureKind, notifier: Option<Arc<dyn Notifier>>) {
    let last = LAST_CAPTURE.with(|l| l.get());
    // Chưa có lần chụp trước: phím lặp lại chạy đúng như phím chụp vùng (kể cả gợi ý cửa sổ).
    let kind = if requested == CaptureKind::RepeatLast && last.is_none() {
        CaptureKind::Region
    } else {
        requested
    };
    let notify = |kind: NoticeKind, title: &str, body: &str| {
        if let Some(n) = &notifier {
            n.notify(kind, title, body);
        }
    };
    let (include_cursor, keep_selection, snap_windows) = with_app(|a| {
        (
            a.config.include_cursor,
            a.config.keep_selection,
            a.config.snap_windows,
        )
    })
    .unwrap_or((false, false, false));
    // Liệt kê ngay trước khi chụp để khung khớp ảnh; chỉ khi có thể mở lớp phủ (chọn vùng, hoặc lặp
    // lại mà vùng trước có thể đã mất) và ô gợi ý đang bật.
    // Thanh phiên không loại khỏi ảnh chụp được (Windows cũ): ẩn nó tới hết lượt chụp.
    let _bar = BarHiddenForCapture(steps_bar::hide_for_capture());
    // Thanh phiên không có trong ảnh: không gợi ý nó như một cửa sổ.
    let frames = enumerates_windows(kind, snap_windows)
        .then(snap::enumerate_windows)
        .map(|w| without_frame(w, steps_bar::frame()));
    let mut shot = match capture::capture_virtual_screen(include_cursor) {
        Ok(s) => s,
        Err(_) => {
            notify(
                NoticeKind::Error,
                lang::tr(Msg::CaptureFailedTitle),
                lang::tr(Msg::CaptureFailedBody),
            );
            return;
        }
    };
    shot.windows = frames.map(|w| snap::windows_to_image(&w, shot.origin));
    let img_w = i32::try_from(shot.image.width).unwrap_or(i32::MAX);
    let img_h = i32::try_from(shot.image.height).unwrap_or(i32::MAX);
    let mut repeat = None;
    // Vùng trước đã mất: báo sau khi lớp phủ đóng (bong bóng có thể bị hoãn hay ẩn khi cửa sổ
    // toàn màn hình trên cùng đang mở).
    let mut region_gone = false;
    if kind == CaptureKind::RepeatLast {
        match repeat_plan(last, shot.origin, img_w, img_h, &shot.monitors) {
            RepeatPlan::Run(r, a) => {
                // Không mở lớp phủ: bỏ khung cửa sổ đã liệt kê.
                shot.windows = None;
                repeat = Some((a, r));
            }
            RepeatPlan::OverlayWithNotice => region_gone = true,
            RepeatPlan::Overlay => {}
        }
    }
    let (action, rect, shapes) = match (kind, repeat) {
        // Lặp lại: không mở lớp phủ, không đổi lần chụp trước, không có hình đánh dấu.
        (_, Some((action, rect))) => (action, rect, Vec::new()),
        // Như phím chụp vùng; với phím lặp lại thì trên cùng ảnh đã chụp (vùng trước đã mất).
        (CaptureKind::Region | CaptureKind::RepeatLast, None) => {
            let prev = LAST_SELECTION.with(|s| s.get());
            let initial = if keep_selection {
                // `restore_selection` nhận màn hình theo tọa độ màn hình (nó tự trừ gốc), còn
                // `shot.monitors` ở tọa độ ảnh: truyền thẳng thì lệch khi gốc âm (màn hình bên trái).
                let screens: Vec<Rect> = shot
                    .monitors
                    .iter()
                    .map(|m| to_screen_coords(*m, shot.origin))
                    .collect();
                prev.and_then(|r| restore_selection(r, shot.origin, img_w, img_h, &screens))
            } else {
                None
            };
            with_app(|a| a.overlay_open = true);
            let before = with_app(|a| a.config.custom_colors).unwrap_or_default();
            let mut custom = before;
            let picked = overlay::pick_region(&shot, initial, &mut custom, steps_store::active());
            with_app(|a| a.overlay_open = false);
            // Mọi kết quả của lớp phủ (chọn, hủy, lỗi) đều báo vùng trước đã mất.
            if region_gone {
                notify(
                    NoticeKind::Info,
                    lang::tr(Msg::RepeatRegionGoneTitle),
                    lang::tr(Msg::RepeatRegionGoneBody),
                );
            }
            // Ảnh ghim từ Lịch sử tới lúc lớp phủ mở: mở bây giờ, trước mọi nhánh của kết quả.
            history_window::open_pending_pin();
            if custom != before {
                let write = CONFIG_WRITABLE.with(|w| w.get());
                store_custom_colors(custom, custom_colors_need_save(&before, &custom, write));
            }
            match picked {
                Ok(Some(p)) => {
                    // Nhớ cả khi ô "giữ vị trí" đang tắt, để bật lên là có ngay.
                    let screen = to_screen_coords(p.rect, shot.origin);
                    LAST_SELECTION.with(|s| s.set(remember_after(prev, p.action, screen)));
                    // Trước khi chạy hành động: tải lên lâu thì lần chụp vẫn đã được nhớ.
                    remember_last_capture(p.action, screen);
                    (p.action, p.rect, p.shapes)
                }
                Ok(None) => return,
                Err(e) => {
                    notify(
                        NoticeKind::Error,
                        lang::tr(Msg::OverlayFailedTitle),
                        &i18n::reason_try_again(lang::current(), &e),
                    );
                    return;
                }
            }
        }
        (CaptureKind::SaveFull | CaptureKind::UploadFull, None) => {
            let rect = full_screen_rect(shot.cursor, shot.origin, &shot.monitors, img_w, img_h);
            let action = if kind == CaptureKind::SaveFull {
                Action::Save
            } else {
                Action::Upload
            };
            (action, rect, Vec::new())
        }
    };
    let image = match picked_image(&shot.image, rect, &shapes) {
        Ok(image) => image,
        Err(_) => {
            // Không gửi đi ảnh thiếu nét đã thấy trên màn hình.
            notify(
                NoticeKind::Error,
                lang::tr(Msg::AnnotateFailedTitle),
                lang::tr(Msg::AnnotateFailedBody),
            );
            return;
        }
    };
    let pin_at = pin_origin(rect, shot.origin);
    // Giải phóng ảnh chụp ngay khi đã cắt xong.
    drop(shot);
    if action == Action::Pin {
        // Ảnh ghim không đi qua `run_on_image`: không lưu, không ghi Lịch sử.
        if let Err(e) = pin::open(image, pin::PinPlace::At(pin_at)) {
            let (kind, title, body) = pin_failure_notice(lang::current(), &e);
            notify(kind, title, &body);
        }
        return;
    }
    if action == Action::Ocr {
        // Nhận dạng chữ: cửa sổ kết quả, luồng nền; không lưu, không ghi Lịch sử.
        open_ocr(image, None, &notify);
        return;
    }
    if action == Action::AddStep {
        // Bước của phiên: ghi nền vào thư mục phiên; không lưu, không ghi Lịch sử.
        if steps_store::add(image).is_err() {
            notify(
                NoticeKind::Info,
                lang::tr(Msg::StepsFullTitle),
                lang::tr(Msg::StepsFullBody),
            );
        }
        return;
    }
    let Some(notifier) = notifier else {
        return;
    };
    run_on_image(hwnd, action, image, notifier);
}

/// Hiện lại thanh phiên đã ẩn quanh lượt chụp khi rời phạm vi (mọi đường ra của `capture_flow_with`).
struct BarHiddenForCapture(bool);

impl Drop for BarHiddenForCapture {
    fn drop(&mut self) {
        if self.0 {
            steps_bar::show_after_capture();
        }
    }
}

/// Phiên chụp từng bước đổi (bắt đầu, thêm, sửa, kết thúc): hiện/cập nhật hoặc đóng thanh phiên.
fn on_steps_changed() {
    if steps_store::active() {
        steps_bar::show();
    } else {
        steps_bar::close();
    }
}

/// Thư mục mặc định của hộp chọn thư mục khi xuất các bước: thư mục lưu ảnh của cấu hình.
fn steps_export_dir() -> Option<std::path::PathBuf> {
    APP.with(|a| {
        a.try_borrow()
            .ok()
            .and_then(|a| a.as_ref().map(|s| s.config.save_dir.clone()))
    })
}

/// Nút Chụp bước của thanh phiên: nhờ cửa sổ chính chụp vùng (không chạy lớp phủ trong thủ tục
/// cửa sổ của thanh).
fn post_steps_capture() {
    if let Some(h) = MAIN_HWND.with(|h| h.get()) {
        // SAFETY: cửa sổ chính của luồng này; lỗi đăng thông điệp thì bỏ qua.
        unsafe {
            let _ = PostMessageW(Some(h), WM_STEPS_CAPTURE, WPARAM(0), LPARAM(0));
        }
    }
}

/// Việc lúc thoát (Thoát, tắt máy): lưu ảnh của lượt tải lên còn dở, ghi Lịch sử, chờ ảnh bước và
/// lượt xuất, tất cả trong một hạn chót chung `EXIT_BUDGET`. Cửa sổ Xem lại đóng trước tiên để
/// ghi chú đang sửa được lưu trước mọi lần chờ.
fn exit_cleanup() {
    steps_window::close();
    let deadline = std::time::Instant::now() + EXIT_BUDGET;
    save_pending_uploads(remaining(deadline, std::time::Instant::now()));
    finish_steps_on_exit(deadline);
}

/// Báo các lỗi của phiên chụp từng bước qua khay.
fn notify_step_errors(errors: &[steps_store::StepError]) {
    let Some(t) = current_tray() else {
        return;
    };
    for e in errors {
        let (title, body) = step_error_notice(lang::current(), e);
        t.notifier().notify(NoticeKind::Error, title, &body);
    }
}

/// Lúc thoát: chờ ảnh bước đang ghi và lượt xuất (trong hạn chót chung) rồi ghi vào phiên; giữ thư mục phiên; đóng thanh.
fn finish_steps_on_exit(deadline: std::time::Instant) {
    // Cửa sổ Xem lại lưu ghi chú đang sửa khi bị hủy (trước khi chờ ghi ảnh).
    steps_window::close();
    steps_store::flush(remaining(deadline, std::time::Instant::now()));
    // Lượt xuất đang chạy: chờ có hạn để thư mục kịp đổi sang tên cuối.
    steps_window::wait_export_on_exit(remaining(deadline, std::time::Instant::now()));
    steps_bar::close();
}

/// Một yêu cầu chụp loại `kind` từ phím tắt hay nút Chụp bước: menu đang mở thì đóng nó rồi chụp
/// sau khi menu tắt hẳn; không thì chụp ngay.
fn capture_request(hwnd: HWND, kind: CaptureKind) {
    let other_menu = pin::menu_open() || history_window::menu_open();
    if hotkey_defers(MENU_OPEN.with(|m| m.get()), other_menu) {
        // Một menu (khay, ảnh ghim, Lịch sử) đang mở: đóng nó rồi chụp sau khi menu tắt
        // hẳn, như chọn từ menu khay; lớp phủ không chạy trong vòng lặp modal của menu.
        // SAFETY: đóng menu đang hiện của luồng này.
        unsafe {
            let _ = EndMenu();
        }
        request(hwnd, GateEvent::MenuCapture, kind);
    } else {
        handle_event(hwnd, AppEvent::Capture(kind));
    }
}

/// Mở cửa sổ Nhận dạng chữ cho `image`; không mở được thì báo lỗi (không hiện lý do kỹ thuật).
fn open_ocr(
    image: darkshot_core::image::Image,
    avoid: Option<Rect>,
    notify: &dyn Fn(NoticeKind, &str, &str),
) {
    let opened = match avoid {
        Some(_) => ocr_window::open_avoiding(image, avoid),
        None => ocr_window::open(image),
    };
    if opened.is_err() {
        notify(
            NoticeKind::Error,
            lang::tr(Msg::OcrWindowTitle),
            lang::tr(Msg::OcrFailed),
        );
    }
}

/// Thông báo của cửa sổ Nhận dạng chữ (Đã chép chữ, lỗi clipboard) qua khay; Success được lọc ở
/// `WM_NOTICE` theo ô "hiện thông báo lưu và sao chép".
fn ocr_notify(kind: NoticeKind, title: &str, body: &str) {
    if let Some(t) = current_tray() {
        t.notifier().notify(kind, title, body);
    }
}

/// Lưu/Chép từ menu hay phím của ảnh ghim: đúng đường `run_on_image` (thông báo, Lịch sử);
/// Nhận dạng chữ mở cửa sổ kết quả với ảnh gốc của ảnh ghim.
fn pin_action(action: Action, image: darkshot_core::image::Image, pin_rect: Option<Rect>) {
    if action == Action::Ocr {
        // Ảnh ghim luôn ở trên cùng: đặt cửa sổ kết quả cạnh nó, không bị nó che.
        open_ocr(image, pin_rect, &ocr_notify);
        return;
    }
    let Some(hwnd) = MAIN_HWND.with(|h| h.get()) else {
        return;
    };
    let Some(tray) = current_tray() else {
        return;
    };
    let notifier: Arc<dyn Notifier> = Arc::new(tray.notifier());
    run_on_image(hwnd, action, image, notifier);
}

/// Cập nhật màu tự chọn trong cấu hình đang dùng; `write` thì ghi file bằng đúng đường lưu cấu
/// hình của cửa sổ Tùy chọn (khóa R2 được mã hóa lại như khi lưu Tùy chọn). Ghi lỗi thì bỏ qua.
fn store_custom_colors(custom: CustomColors, write: bool) {
    let Some(cfg) = with_app(|a| {
        a.config.custom_colors = custom;
        a.config.clone()
    }) else {
        return;
    };
    if write {
        let _ = darkshot_core::config::save(&paths::config_path(), &cfg, &DpapiStore);
    }
}

/// Ảnh cắt theo `rect` (tọa độ ảnh chụp) có in các hình `shapes`; không hình thì đúng `crop`.
pub fn picked_image(
    image: &darkshot_core::image::Image,
    rect: Rect,
    shapes: &[darkshot_core::annotate::Shape],
) -> Result<darkshot_core::image::Image, String> {
    let mut out = image.crop(rect);
    if !shapes.is_empty() {
        // `crop` kẹp vùng vào ảnh: gốc của ảnh cắt là góc trên trái sau khi kẹp.
        let origin = Rect {
            x: rect.x.max(0),
            y: rect.y.max(0),
            w: i32::try_from(out.width).unwrap_or(i32::MAX),
            h: i32::try_from(out.height).unwrap_or(i32::MAX),
        };
        annotate_draw::burn(&mut out, shapes, origin)?;
    }
    Ok(out)
}

/// Thực hiện `action` trên ảnh đã cắt; chung cho cả ba loại chụp. Tải lên dùng sổ `PENDING`,
/// lưu dự phòng khi không tạo được luồng và mở Tùy chọn khi thiếu cấu hình R2.
fn run_on_image(
    hwnd: HWND,
    action: Action,
    image: darkshot_core::image::Image,
    notifier: Arc<dyn Notifier>,
) {
    let Some(config) = with_app(|a| a.config.clone()) else {
        return;
    };
    let clipboard: Arc<dyn Clipboard> = Arc::new(clipboard::WinClipboard);
    match action {
        Action::Cancel | Action::Pin | Action::Ocr | Action::AddStep => {}
        Action::Save | Action::Copy => {
            let ctx = Ctx {
                save_dir: &config.save_dir,
                expiry: config.expiry,
                now: paths::local_now(),
                uploader: None,
                clipboard: clipboard.as_ref(),
                notifier: notifier.as_ref(),
                lang: lang::current(),
                upload_format: UploadFormat::Png,
                jpeg_quality: JPEG_QUALITY_DEFAULT,
            };
            let outcome = run_action(&ctx, action, &image);
            history_service::record_outcome(&outcome, &image, config.expiry, ctx.now);
        }
        Action::Upload => {
            let uploader: Option<Arc<dyn Uploader>> = if config.r2.is_complete() {
                Some(Arc::new(R2Uploader::new(&config.r2)))
            } else {
                None
            };
            let image = Arc::new(image);
            let expiry = config.expiry;
            let hwnd_raw = hwnd.0 as isize;
            let registry = PENDING.with(Arc::clone);
            let pending_id = registry.add(image.clone(), config.save_dir.clone());
            let job = UploadJob {
                save_dir: config.save_dir.clone(),
                expiry: config.expiry,
                uploader,
                clipboard: clipboard.clone(),
                notifier: notifier.clone(),
                image: image.clone(),
                clock: paths::local_now,
                on_needs_settings: Box::new(move || post_open_settings(hwnd_raw, 0)),
                pending: Some((registry.clone(), pending_id)),
                lang: lang::current(),
                upload_format: config.upload_format,
                jpeg_quality: config.jpeg_quality,
                on_outcome: Some({
                    // Cùng ảnh của lượt (`Arc`): hàng ghi giữ nó, không sao chép lần hai.
                    let shared = image.clone();
                    Box::new(move |o, _img| {
                        // Hạn tính từ lúc tải xong: R2 đếm vòng đời từ lúc đối tượng được tạo (PUT xong).
                        let now = paths::local_now();
                        history_service::record_outcome_arc(o, shared.clone(), expiry, now)
                    })
                }),
            };
            if let Err(e) = app::spawn_upload(job) {
                // Không có luồng nào giữ lượt này: lưu ngay ở dưới, rút khỏi sổ.
                registry.remove(pending_id);
                // Không tạo được luồng: lưu ảnh ngay thay vì mất nó.
                notifier.notify(
                    NoticeKind::Error,
                    lang::tr(Msg::UploadStartFailedTitle),
                    &i18n::upload_start_failed_body(lang::current(), &e),
                );
                let ctx = Ctx {
                    save_dir: &config.save_dir,
                    expiry: config.expiry,
                    now: paths::local_now(),
                    uploader: None,
                    clipboard: clipboard.as_ref(),
                    notifier: notifier.as_ref(),
                    lang: lang::current(),
                    upload_format: UploadFormat::Png,
                    jpeg_quality: JPEG_QUALITY_DEFAULT,
                };
                let outcome = history_service::as_fallback(run_action(&ctx, Action::Save, &image));
                history_service::record_outcome(&outcome, &image, config.expiry, ctx.now);
            }
        }
    }
}

/// Mở Cài đặt, lưu kết quả. Chỉ gọi qua `gate` (trạng thái `SettingsOpen`).
fn open_settings(hwnd: HWND, reason: OpenReason) {
    let Some((config, notice)) = with_app(|a| (a.config.clone(), a.config_notice.take())) else {
        return;
    };
    let Some(result) = settings::show(
        hwnd,
        &config,
        notice.map(|n| n.message(lang::current())).as_deref(),
        reason,
    ) else {
        // Hủy: `show` đã đăng ký lại các dòng của cấu hình cũ; dòng nào lỗi thì báo gộp một lần.
        let failed = settings::take_cancel_failures();
        if let Some(t) = current_tray() {
            notify_hotkeys_taken(&config.hotkeys, &failed, &t.notifier());
        }
        return;
    };
    let notify = |kind: NoticeKind, title: &str, body: &str| {
        if let Some(t) = current_tray() {
            t.notifier().notify(kind, title, body);
        }
    };
    // `show` đã đăng ký phím mới cho hwnd; ở đây chỉ ghi đĩa.
    let mut new = result.config;
    // Một lần kiểm tra có thể báo xong trong lúc Tùy chọn mở: giữ số bản đã báo mới nhất.
    if let Some(n) = with_app(|a| a.config.update_notified) {
        new.update_notified = n;
    }
    let saved = darkshot_core::config::save(&paths::config_path(), &new, &DpapiStore);
    // Ghi thất bại thì ngôn ngữ giữ nguyên, cùng với cấu hình cũ và phím cũ.
    lang::set(lang_after_save(
        config.language,
        new.language,
        saved.is_ok(),
    ));
    match saved {
        Ok(()) => {
            history_service::set_enabled(new.history_enabled);
            let autostart_result = if side_effects_allowed() {
                autostart::set_enabled(new.autostart)
            } else {
                Ok(())
            };
            if let Err(e) = autostart_result {
                notify(
                    NoticeKind::Error,
                    lang::tr(Msg::AutostartFailedTitle),
                    &i18n::reason(lang::current(), &e),
                );
            }
            // Chữ trong hai cửa sổ này (ngôn ngữ, phím tắt) không còn đúng thì đóng chúng.
            if info_windows_stale(&config, &new) {
                about::close();
                help::close();
                // Cửa sổ Nhận dạng chữ không đóng (giữ chữ người dùng đang sửa): đổi chữ giao diện.
                ocr_window::relabel();
                steps_bar::relabel();
                steps_window::relabel();
            }
            if history_window_stale(&config, &new) {
                history_window::close_if_open();
            }
            let check_after = new.check_updates;
            with_app(|a| a.config = new);
            // Lịch sử vừa bật/tắt: cửa sổ đang mở vẽ lại dải thông báo.
            history_window::refresh_enabled();
            if check_after && !config.check_updates {
                start_update_timer(hwnd);
            } else if !check_after && config.check_updates {
                stop_update_timer(hwnd);
            }
            // File giờ khớp cấu hình trong bộ nhớ: từ đây màu tự chọn được ghi.
            CONFIG_WRITABLE.with(|w| w.set(true));
        }
        Err(e) => {
            // Không ghi được: giữ cấu hình cũ và phím cũ (chỉ các dòng đang bật của cấu hình cũ).
            hotkey::unregister_all(hwnd);
            let why = match e {
                darkshot_core::config::ConfigError::Io(m)
                | darkshot_core::config::ConfigError::Corrupt(m)
                | darkshot_core::config::ConfigError::Secret(m) => m,
            };
            notify(
                NoticeKind::Error,
                lang::tr(Msg::OptionsSaveFailedTitle),
                &i18n::options_save_failed_body(lang::current(), &why),
            );
            if let Some(t) = current_tray() {
                register_hotkeys_or_notify(hwnd, &config.hotkeys, &t.notifier());
            } else {
                let _ = hotkey::register_enabled(hwnd, &config.hotkeys);
            }
        }
    }
}

/// Mở Cài đặt (cổng đã ở `SettingsOpen`), rồi đóng cổng lại.
fn run_settings(hwnd: HWND) {
    // Lấy lý do đang chờ ra, đặt lại `User` cho lần mở sau.
    let reason = OPEN_REASON.with(|r| r.replace(OpenReason::User));
    open_settings(hwnd, reason);
    let (next, _) = gate_step(GATE.with(|g| g.get()), GateEvent::SettingsClosed);
    GATE.with(|g| g.set(next));
}

/// Đưa một sự kiện không phải yêu cầu chụp (hẹn giờ, bấm khay, mở Tùy chọn) qua cổng:
/// loại chụp đang có giữ nguyên.
fn gate(hwnd: HWND, ev: GateEvent) -> GateAction {
    let current = CAPTURE_KIND.with(|k| k.get());
    request(hwnd, ev, current)
}

/// Lối vào duy nhất của cổng chụp: tính `gate_step`, đặt loại chụp theo `next_kind` (chỉ đổi khi
/// yêu cầu `requested` được nhận), rồi làm việc mà cổng yêu cầu. Mọi yêu cầu chụp (ba phím tắt,
/// "Chụp màn hình" ở menu) gọi thẳng hàm này với loại của mình.
fn request(hwnd: HWND, ev: GateEvent, requested: CaptureKind) -> GateAction {
    let (next, act) = gate_step(GATE.with(|g| g.get()), ev);
    GATE.with(|g| g.set(next));
    CAPTURE_KIND.with(|k| k.set(next_kind(k.get(), requested, act)));
    match act {
        GateAction::StartTimer => {
            // Chờ menu khay tắt hẳn (hiệu ứng mờ dần) để nó không lọt vào ảnh.
            // SAFETY: bộ hẹn giờ gắn với cửa sổ của luồng này; WM_TIMER xử lý ở wndproc.
            let timer = unsafe {
                SetTimer(
                    Some(hwnd),
                    MENU_CAPTURE_TIMER_ID,
                    MENU_CAPTURE_DELAY_MS,
                    None,
                )
            };
            if timer == 0 {
                // Không đặt được hẹn giờ: chụp ngay thay vì để cổng kẹt ở PendingMenu.
                return gate(hwnd, GateEvent::TimerFired);
            }
        }
        GateAction::CaptureNow => {
            // SAFETY: hủy bộ hẹn giờ của menu (nếu có) của chính cửa sổ này.
            unsafe {
                let _ = KillTimer(Some(hwnd), MENU_CAPTURE_TIMER_ID);
            }
            capture_flow(hwnd);
            let (next, after) = gate_step(GATE.with(|g| g.get()), GateEvent::OverlayClosed);
            GATE.with(|g| g.set(next));
            if after == GateAction::OpenSettings {
                run_settings(hwnd);
            }
        }
        GateAction::OpenSettings => {
            run_settings(hwnd);
        }
        GateAction::FocusSettings => {
            // Yêu cầu mở đã được đáp bằng cửa sổ đang mở: không để lý do của nó dính sang lần mở sau.
            if ev == GateEvent::OpenSettings {
                OPEN_REASON.with(|r| r.set(OpenReason::User));
            }
            settings::focus_existing();
        }
        GateAction::None | GateAction::Ignore => {}
    }
    act
}

fn handle_event(hwnd: HWND, ev: AppEvent) {
    match ev {
        AppEvent::Capture(kind) => {
            request(hwnd, GateEvent::Hotkey, kind);
        }
        AppEvent::OpenSettings => {
            gate(hwnd, GateEvent::OpenSettings);
        }
        AppEvent::Quit => {
            // Lưu ghi chú đang sửa của cửa sổ Xem lại trước mọi việc khác.
            steps_window::close();
            // Đóng mọi ảnh ghim (đồng bộ, không chờ gì); ảnh ghim đang chờ thì bỏ.
            history_window::drop_pending_pin();
            pin::close_all();
            ocr_window::close_all();
            // Thoát lúc đang tải lên: lưu ảnh vào máy trước, không chờ mạng; chờ trong một hạn chót.
            exit_cleanup();
            // SAFETY: hwnd là cửa sổ của luồng này.
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let tb = TASKBAR_CREATED.with(|c| c.get());
    if tb != 0 && msg == tb {
        if let Some(t) = current_tray() {
            if !t.readd() {
                // Explorer chưa nhận biểu tượng: thử lại theo hẹn giờ như lúc khởi động.
                ADD_ATTEMPTS.with(|c| c.set(0));
                // SAFETY: bộ hẹn giờ gắn với cửa sổ của luồng này; WM_TIMER xử lý ở wndproc.
                unsafe {
                    SetTimer(Some(hwnd), RETRY_TIMER_ID, RETRY_INTERVAL_MS, None);
                }
            }
        }
        return LRESULT(0);
    }
    match msg {
        WM_HOTKEY if hotkey::role_of(wp.0 as i32).is_some() => {
            // Mọi phím (chụp vùng, lưu và tải lên toàn màn hình, lặp lại) dùng chung một luật.
            let kind = hotkey::role_of(wp.0 as i32)
                .map(kind_of)
                .unwrap_or(CaptureKind::Region);
            capture_request(hwnd, kind);
            LRESULT(0)
        }
        // Chụp bước: như phím chụp vùng.
        WM_STEPS_CAPTURE => {
            capture_request(hwnd, CaptureKind::Region);
            LRESULT(0)
        }
        steps_store::WM_STEPS_SAVED => {
            steps_store::drain_saved();
            LRESULT(0)
        }
        steps_window::WM_STEPS_EXPORTED => {
            steps_window::on_exported();
            LRESULT(0)
        }
        WM_TRAY => {
            let ev = (lp.0 & 0xFFFF) as u32;
            if ev == NIN_BALLOONUSERCLICK {
                // Chỉ mở link khi bong bóng vừa bấm là thông báo có bản mới.
                if current_tray().is_some_and(|t| t.is_update_notice()) {
                    if let Some(url) =
                        with_app(|a| a.update.as_ref().map(|r| r.url.clone())).flatten()
                    {
                        open_release_url(&url);
                    }
                }
                return LRESULT(0);
            }
            // Menu đang mở thì bỏ qua hẳn: không mở menu lồng và không hạ cờ MENU_OPEN sớm.
            if tray_click_opens_menu(ev, MENU_OPEN.with(|m| m.get()))
                && gate(hwnd, GateEvent::TrayClick) == GateAction::None
            {
                MENU_OPEN.with(|m| m.set(true));
                let label = with_app(|a| {
                    a.update
                        .as_ref()
                        .map(|r| i18n::update_menu(lang::current(), &r.version.to_string()))
                })
                .flatten();
                let pins = pin::count();
                let can_repeat = LAST_CAPTURE.with(|l| l.get()).is_some();
                let steps = steps_store::active().then(steps_store::count);
                let cmd = current_tray()
                    .and_then(|t| t.show_menu(label.as_deref(), pins, can_repeat, steps));
                MENU_OPEN.with(|m| m.set(false));
                match cmd {
                    Some(MenuCommand::OpenUpdate) => {
                        if let Some(url) =
                            with_app(|a| a.update.as_ref().map(|r| r.url.clone())).flatten()
                        {
                            open_release_url(&url);
                        }
                    }
                    Some(MenuCommand::Capture) => {
                        request(hwnd, GateEvent::MenuCapture, CaptureKind::Region);
                    }
                    Some(MenuCommand::RepeatLast) => {
                        request(hwnd, GateEvent::MenuCapture, CaptureKind::RepeatLast);
                    }
                    Some(MenuCommand::Options) => {
                        note_open_reason(OpenReason::User);
                        handle_event(hwnd, AppEvent::OpenSettings);
                    }
                    // Lịch sử, Thông tin và Trợ giúp không đi qua cổng chụp: không chặn gì và không bị chặn.
                    Some(MenuCommand::History) => history_window::show(hwnd),
                    Some(MenuCommand::ClosePins) => pin::close_all(),
                    Some(MenuCommand::StepsStart) => {
                        steps_store::start();
                    }
                    Some(MenuCommand::StepsReview) => steps_window::open(),
                    Some(MenuCommand::About) => about::show(hwnd),
                    Some(MenuCommand::Help) => {
                        if let Some(keys) = with_app(|a| hotkey::help_keys(&a.config.hotkeys)) {
                            help::show(hwnd, &keys);
                        }
                    }
                    Some(MenuCommand::Quit) => handle_event(hwnd, AppEvent::Quit),
                    None => {}
                }
            }
            LRESULT(0)
        }
        WM_OPEN_SETTINGS => {
            // Ứng dụng tự mở (khởi động thiếu cấu hình, tải lên báo thiếu cấu hình): mở ở tab R2.
            // wParam 1: người dùng bấm "Mở Tùy chọn" trong cửa sổ Lịch sử, như mục menu khay.
            note_open_reason(if wp.0 == 1 {
                OpenReason::User
            } else {
                OpenReason::NeedsR2
            });
            if MENU_OPEN.with(|m| m.get()) {
                // Menu khay đang mở: đóng nó rồi mở Cài đặt sau khi menu thoát, không mở lồng.
                // Gửi lại đúng `wp` gốc để lý do mở (người dùng hay ứng dụng) không bị đổi.
                // SAFETY: đóng menu đang hiện của luồng này.
                unsafe {
                    let _ = EndMenu();
                }
                post_open_settings(hwnd.0 as isize, wp.0);
            } else {
                handle_event(hwnd, AppEvent::OpenSettings);
            }
            LRESULT(0)
        }
        WM_NOTICE => {
            // SAFETY: con trỏ do TrayNotifier::notify cấp phát và chuyển quyền sở hữu cho ta.
            let notice = unsafe { Box::from_raw(lp.0 as *mut Notice) };
            // Chỗ lọc duy nhất: thông báo thành công theo cài đặt; lỗi và thông tin luôn hiện.
            let show = with_app(|a| a.config.show_success_notices).unwrap_or(true);
            if notice_visible(notice.kind, show) {
                if let Some(t) = current_tray() {
                    t.show_notice(&notice);
                }
            }
            LRESULT(0)
        }
        WM_TIMER if wp.0 == MENU_CAPTURE_TIMER_ID => {
            // SAFETY: hủy bộ hẹn giờ một lần của chính cửa sổ này.
            unsafe {
                let _ = KillTimer(Some(hwnd), MENU_CAPTURE_TIMER_ID);
            }
            if timer_should_wait(pin::menu_open(), history_window::menu_open()) {
                // Người dùng vừa mở menu ảnh ghim/Lịch sử trong lúc chờ: đóng nó và chờ thêm một
                // lượt, để lớp phủ không chạy trong vòng lặp modal của menu.
                // SAFETY: đóng menu đang hiện của luồng này; hẹn giờ gắn với chính cửa sổ này.
                let timer = unsafe {
                    let _ = EndMenu();
                    SetTimer(
                        Some(hwnd),
                        MENU_CAPTURE_TIMER_ID,
                        MENU_CAPTURE_DELAY_MS,
                        None,
                    )
                };
                if timer != 0 {
                    return LRESULT(0);
                }
                // Không đặt được hẹn giờ: giữ hành vi sẵn có (chụp ngay) để cổng không kẹt.
            }
            gate(hwnd, GateEvent::TimerFired);
            LRESULT(0)
        }
        WM_OPEN_HISTORY => {
            history_window::show(hwnd);
            LRESULT(0)
        }
        WM_UPDATE_RESULT => {
            // SAFETY: con trỏ do luồng kiểm tra cấp phát và chuyển quyền sở hữu cho ta.
            let found = unsafe { Box::from_raw(lp.0 as *mut Option<Release>) };
            on_update_result(*found);
            LRESULT(0)
        }
        WM_TIMER if wp.0 == UPDATE_TIMER_ID => {
            on_update_timer(hwnd);
            LRESULT(0)
        }
        WM_TIMER if wp.0 == RETRY_TIMER_ID => {
            on_retry_timer(hwnd);
            LRESULT(0)
        }
        // Tài liệu WM_QUERYENDSESSION: trả TRUE ngay, để việc dọn dẹp tới WM_ENDSESSION.
        WM_QUERYENDSESSION => LRESULT(1),
        WM_ENDSESSION => {
            if wp.0 != 0 {
                // Phiên sắp kết thúc (tắt máy, đăng xuất): đóng ảnh ghim, ghi file ảnh của các lượt
                // còn dở. Ghi chú đang sửa của cửa sổ Xem lại được lưu trước tiên.
                steps_window::close();
                pin::close_all();
                ocr_window::close_all();
                exit_cleanup();
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            // Cửa sổ Thông tin, Trợ giúp và Lịch sử thuộc cửa sổ ẩn này: hủy cả ba, trả tài nguyên.
            about::close();
            help::close();
            history_window::close_if_open();
            // Lưới an toàn cho mọi đường thoát: ảnh ghim không sống quá cửa sổ chính.
            history_window::drop_pending_pin();
            pin::close_all();
            ocr_window::close_all();
            steps_window::close();
            steps_bar::close();
            hotkey::unregister_all(hwnd);
            TRAY.with(|t| t.borrow_mut().take());
            // SAFETY: kết thúc vòng lặp thông điệp của luồng này.
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        // SAFETY: chuyển nguyên tham số cho xử lý mặc định.
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

fn create_window() -> windows::core::Result<HWND> {
    // SAFETY: đăng ký lớp và tạo cửa sổ ẩn (không bao giờ ShowWindow) ở luồng chính.
    unsafe {
        let hinst = GetModuleHandleW(None)?;
        let class = w!("darkshot-hidden-window");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            lpszClassName: class,
            ..Default::default()
        };
        // Lớp đã tồn tại không xảy ra vì chỉ có một bản chạy; lỗi thật sẽ lộ ra ở CreateWindowExW.
        RegisterClassW(&wc);
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class,
            w!("darkshot"),
            WS_OVERLAPPED,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            0,
            0,
            None,
            None,
            Some(hinst.into()),
            None,
        )
    }
}

fn main() {
    let args = parse_args(&std::env::args().skip(1).collect::<Vec<_>>());
    let mode = startup_mode(paths::profile_override().as_deref());
    SIDE_EFFECTS_ALLOWED.store(!mode.sandbox, Ordering::SeqCst);
    let mutex_name = HSTRING::from(mode.mutex_name.as_str());
    // SAFETY: mutex được giữ đến khi tiến trình thoát (biến `_instance`); tên là HSTRING còn sống.
    let _instance = unsafe {
        let h = CreateMutexW(None, false, &mutex_name);
        if h.is_ok() && GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }
        h
    };
    // Không để Windows gửi bản kết xuất bộ nhớ (có thể chứa khóa, ảnh) đi khi sự cố.
    if mode.exclude_wer {
        wer::exclude_from_error_reporting();
    }

    // Menu khay và menu hệ thống theo chế độ tối; phải gọi trước khi tạo cửa sổ.
    theme::enable_dark_menus();
    let Ok(hwnd) = create_window() else { return };
    MAIN_HWND.with(|h| h.set(Some(hwnd)));
    pin::set_action_sink(pin_action);
    ocr_window::set_notifier(ocr_notify);
    // SAFETY: chuỗi hằng kết thúc bằng 0.
    TASKBAR_CREATED.with(|c| c.set(unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) }));

    let Ok(tray) = Tray::new(hwnd) else {
        // SAFETY: hwnd vừa tạo ở luồng này.
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        return;
    };
    let notifier = tray.notifier();
    if !tray.is_added() {
        // SAFETY: bộ hẹn giờ gắn với cửa sổ của luồng này; WM_TIMER xử lý ở wndproc.
        unsafe {
            SetTimer(Some(hwnd), RETRY_TIMER_ID, RETRY_INTERVAL_MS, None);
        }
    }
    TRAY.with(|t| *t.borrow_mut() = Some(Rc::new(tray)));

    // Chép cấu hình của bản cũ (nếu cần) trước khi đọc cấu hình; lỗi thì bỏ qua, báo sau.
    let mut migration_failed = mode.run_legacy && legacy::run_before_startup();
    let config_path = paths::config_path();
    let state = app::startup(
        &config_path,
        &DpapiStore,
        &paths::default_save_dir(),
        lang::os_default(),
    );
    lang::set(state.config.language);
    history_service::set_enabled(state.config.history_enabled);
    let config_existed = config_path.exists();
    let open_settings_now = app::needs_settings_on_start(&state, config_existed);
    let keys = state.config.hotkeys;
    // Khóa Run theo đúng cấu hình darkshot: bỏ giá trị cũ, ghi lại đường dẫn exe hiện tại.
    // Cấu hình hỏng hoặc không đọc được thì không đụng khóa Run.
    if mode.sync_autostart && app::should_sync_autostart(state.config_notice) {
        migration_failed |= legacy::sync_autostart(state.config.autostart);
    }
    CONFIG_WRITABLE.with(|w| w.set(config_ok_at_start(state.config_notice, config_existed)));
    APP.with(|a| *a.borrow_mut() = Some(state));
    // Lần chụp trước (nếu có) cho phím lặp lại và cho "giữ vị trí vùng"; lỗi đọc thì coi như chưa có.
    let last = read_last_capture(&paths::last_capture_path());
    LAST_CAPTURE.with(|l| l.set(last));
    LAST_CAPTURE_ON_DISK.with(|l| l.set(last));
    LAST_SELECTION.with(|s| s.set(last.map(|l| l.rect)));
    register_hotkeys_or_notify(hwnd, &keys, &notifier);
    // Chụp từng bước: phiên còn từ lần chạy trước thì tiếp tục và hiện thanh phiên.
    steps_store::set_window(hwnd);
    steps_store::set_on_error(notify_step_errors);
    steps_store::on_change(on_steps_changed);
    steps_store::on_change(steps_window::on_store_changed);
    steps_bar::set_on_capture(post_steps_capture);
    steps_bar::set_on_review(steps_window::open);
    steps_bar::set_on_confirm_end(steps_window::open_confirm_end);
    steps_window::set_main_window(hwnd);
    steps_window::set_notifier(ocr_notify);
    steps_window::set_default_dir(steps_export_dir);
    steps_store::restore();
    if mode.update_timer && with_app(|a| a.config.check_updates).unwrap_or(false) {
        start_update_timer(hwnd);
    }

    if open_settings_now {
        post_open_settings(hwnd.0 as isize, 0);
    }
    if args.open_history {
        // SAFETY: cửa sổ của luồng này; lỗi đăng thông điệp thì bỏ qua.
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_OPEN_HISTORY, WPARAM(0), LPARAM(0));
        }
    }
    // Sau thông báo cấu hình (nếu có); loại Info nên không bị ô "thông báo lưu và sao chép" lọc.
    if migration_failed {
        notifier.notify(
            NoticeKind::Info,
            lang::tr(Msg::NoticeMigrationIncompleteTitle),
            lang::tr(Msg::NoticeMigrationIncomplete),
        );
    }

    let mut msg = MSG::default();
    // SAFETY: vòng lặp thông điệp chuẩn; GetMessageW trả về 0 (WM_QUIT) hoặc -1 (lỗi) thì dừng.
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            if about::pre_translate(&msg)
                || help::pre_translate(&msg)
                || history_window::pre_translate(&msg)
                || ocr_window::pre_translate(&msg)
                || steps_window::pre_translate(&msg)
            {
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    // Lưới an toàn: vòng lặp kết thúc theo đường nào thì ảnh của lượt còn dở cũng được lưu.
    let deadline = std::time::Instant::now() + EXIT_BUDGET;
    save_pending_uploads(remaining(deadline, std::time::Instant::now()));
    steps_store::flush(remaining(deadline, std::time::Instant::now()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkshot_core::config::Config;
    use darkshot_core::i18n::Lang;

    #[test]
    fn normal_mode_keeps_everything() {
        let m = startup_mode(None);
        assert_eq!(m.mutex_name, "Local\\darkshot-single-instance");
        assert!(m.run_legacy && m.sync_autostart && m.exclude_wer && m.update_timer);
        assert!(!m.sandbox);
    }

    #[test]
    fn sandbox_mode_skips_side_effects() {
        let m = startup_mode(Some(std::path::Path::new("D:\\sb\\one")));
        assert!(m.sandbox);
        assert!(m
            .mutex_name
            .starts_with("Local\\darkshot-single-instance-sandbox-"));
        let suffix = &m.mutex_name["Local\\darkshot-single-instance-sandbox-".len()..];
        assert!(!suffix.is_empty() && suffix.len() <= 16);
        assert!(suffix.bytes().all(|b| b.is_ascii_hexdigit()));
        assert!(!m.run_legacy && !m.sync_autostart && !m.exclude_wer && !m.update_timer);
        // Đường dẫn khác → tên khác; cùng đường dẫn → cùng tên.
        let other = startup_mode(Some(std::path::Path::new("D:\\sb\\two")));
        assert_ne!(m.mutex_name, other.mutex_name);
        assert_eq!(
            m.mutex_name,
            startup_mode(Some(std::path::Path::new("D:\\sb\\one"))).mutex_name
        );
        assert_ne!(m.mutex_name, startup_mode(None).mutex_name);
    }

    /// Mã thông điệp của chụp từng bước không trùng mã nào đã dùng (WM_APP+1..+12).
    #[test]
    fn steps_message_ids() {
        assert_eq!(steps_store::WM_STEPS_SAVED, WM_APP + 13);
        assert_eq!(WM_STEPS_CAPTURE, WM_APP + 14);
    }

    #[test]
    fn history_message_ids() {
        assert_eq!(WM_OPEN_HISTORY, WM_APP + 5);
        assert_ne!(WM_OPEN_HISTORY, WM_OPEN_SETTINGS);
        assert_ne!(WM_OPEN_HISTORY, WM_UPDATE_RESULT);
        assert_ne!(WM_OPEN_HISTORY, history_service::WM_HISTORY_CHANGED);
    }

    #[test]
    fn annotated_capture_writes_marked_image() {
        use darkshot_core::annotate::{Pt, Shape};
        use darkshot_core::image::Image;
        use darkshot_core::theme::Rgb;
        let (w, h) = (200u32, 100u32);
        let mut bgra = Vec::with_capacity((w * h * 4) as usize);
        for i in 0..w * h {
            bgra.extend_from_slice(&[(i % 251) as u8, 50, (i % 13) as u8, 255]);
        }
        let shot = Image {
            width: w,
            height: h,
            bgra,
        };
        let rect = Rect {
            x: 40,
            y: 20,
            w: 100,
            h: 60,
        };
        // Không hình: đúng từng byte như crop.
        assert_eq!(picked_image(&shot, rect, &[]), Ok(shot.crop(rect)));
        let line = Shape::Line {
            a: Pt { x: 60.0, y: 50.0 },
            b: Pt { x: 120.0, y: 50.0 },
            width: 3.0,
            color: Rgb(255, 0, 0),
        };
        let out = picked_image(&shot, rect, &[line]).unwrap();
        let plain = shot.crop(rect);
        assert_eq!((out.width, out.height), (100, 60));
        let px = |img: &Image, x: u32, y: u32| {
            let i = ((y * img.width + x) * 4) as usize;
            [
                img.bgra[i],
                img.bgra[i + 1],
                img.bgra[i + 2],
                img.bgra[i + 3],
            ]
        };
        // Điểm ảnh chụp (90, 50) nằm ở (50, 30) của ảnh cắt: màu nét.
        let p = px(&out, 50, 30);
        assert!(
            p[0] <= 2 && p[1] <= 2 && p[2] >= 253 && p[3] == 255,
            "{p:?}"
        );
        // Xa nét: không đổi.
        assert_eq!(px(&out, 50, 10), px(&plain, 50, 10));
        assert_eq!(px(&out, 5, 30), px(&plain, 5, 30));
    }

    #[test]
    fn tray_click_while_menu_open_is_ignored() {
        assert!(tray_click_opens_menu(WM_RBUTTONUP, false));
        assert!(tray_click_opens_menu(WM_LBUTTONUP, false));
        assert!(!tray_click_opens_menu(WM_RBUTTONUP, true));
        assert!(!tray_click_opens_menu(WM_LBUTTONUP, true));
        assert!(!tray_click_opens_menu(WM_TIMER, false));
    }

    fn rc(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    fn lc(rect: Rect, action: Action) -> LastCapture {
        LastCapture { rect, action }
    }

    fn temp_dir_for(name: &str) -> std::path::PathBuf {
        let d =
            std::env::temp_dir().join(format!("darkshot_lastcap_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    struct NullNotifier;
    impl Notifier for NullNotifier {
        fn notify(&self, _: NoticeKind, _: &str, _: &str) {}
    }

    /// Chụp màn hình thật rồi ghi PNG vào thư mục tạm; không hiện cửa sổ, không dời chuột.
    #[test]
    #[ignore = "chụp màn hình thật; chạy tay: cargo test -p darkshot-windows -- --ignored repeat_last_saves_png_of_region --test-threads=1"]
    fn repeat_last_saves_png_of_region() {
        let root = temp_dir_for("manual");
        std::fs::create_dir_all(&root).unwrap();
        let cfg = Config::defaults(&root, Lang::En);
        APP.with(|a| {
            *a.borrow_mut() = Some(AppState {
                config: cfg,
                config_notice: None,
                overlay_open: false,
                update: None,
            })
        });
        // (0, 0) luôn thuộc màn hình chính.
        let last = lc(rc(0, 0, 200, 120), Action::Save);
        LAST_CAPTURE.with(|l| l.set(Some(last)));
        capture_flow_with(
            HWND::default(),
            CaptureKind::RepeatLast,
            Some(Arc::new(NullNotifier)),
        );
        // Lặp lại không đổi lần chụp trước, không mở lớp phủ.
        assert_eq!(LAST_CAPTURE.with(|l| l.get()), Some(last));
        assert!(!overlay_open());
        let pngs: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "png"))
            .collect();
        assert_eq!(pngs.len(), 1, "{pngs:?}");
        let img = darkshot_core::thumb::decode_png_file(&pngs[0]).unwrap();
        assert_eq!((img.width, img.height), (200, 120));
        let _ = std::fs::remove_dir_all(&root);
    }
}
