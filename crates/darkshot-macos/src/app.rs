//! Vỏ ứng dụng macOS: `NSApplication` chính sách `accessory` (không biểu tượng Dock), vòng lặp
//! chính, một bản chạy cho mỗi hồ sơ, cổng chụp, nối cấu hình.
//!
//! Luồng: mọi trạng thái ứng dụng chỉ sống trên luồng chính (`thread_local`). Việc lâu (chụp,
//! mã hóa PNG, ghi tệp) chạy trên luồng nền; kết quả về luồng chính qua hàng đợi chính
//! (`DispatchQueue::main`). Không gọi AppKit từ luồng nền.
//!
//! "Chụp" mở lớp phủ chọn vùng (`overlay`, Task 105) trên ảnh của mọi màn hình; cổng chụp chỉ
//! về rảnh khi lớp phủ đóng (mọi đường đóng đều gọi `on_overlay_closed`).
//!
//! Cửa sổ Tùy chọn (`options`, Task 107) mở từ menu, khi khởi động lần đầu hay cấu hình lỗi
//! (`app_state::needs_settings_on_start`) và khi tải lên không được vì thiếu R2
//! (`OpenReason::NeedsR2`). Lưu: `config::save` (khóa vào Keychain) rồi cập nhật cấu hình đang chạy;
//! đóng cửa sổ (mọi đường) đăng ký lại phím tắt.
//!
//! Tải lên R2 (`upload`, Task 108): nút Tải lên của lớp phủ và phím `UploadFull` cùng đi qua
//! `start_upload`; lượt vào sổ `PendingUploads` trước khi chạy, và `quit` lưu các lượt còn dở vào
//! thư mục lưu (không mạng) trong hạn chót `EXIT_BUDGET`.
//!
//! Phím tắt toàn cục (`hotkey`, Task 106) đi qua cùng cổng chụp với menu (`GateEvent::Hotkey`):
//! chụp vùng mở lớp phủ; lưu/tải lên toàn màn hình lấy màn hình chứa con trỏ rồi chạy hành động
//! trên luồng nền; cổng về rảnh ngay khi đã có ảnh.

use crate::capture::{self, CaptureError, DisplayShot};
use crate::clipboard::PasteboardClipboard;
use crate::coords::{cocoa_to_top_left, RectF};
use crate::keymap::{
    classify_failed, mac_default_hotkeys, mac_kind, mac_taken_notice, mac_windows_keys_notice,
    HotkeyFailures,
};
use crate::notify::{self, MacNotifier};
use crate::options::{self, Env};
use crate::options_model::persist;
use crate::overlay::{self, Outcome};
use crate::secret::{self, KeychainStore};
use crate::tray::Tray;
use crate::upload;
use crate::{hotkey, lang, paths, timeutil};
use darkshot_core::actions::{run_action, Action, Ctx, NoticeKind, Notifier};
use darkshot_core::app_flow::{OpenReason, EXIT_BUDGET};
use darkshot_core::app_state::{self, AppState, ConfigNotice};
use darkshot_core::capture_gate::{
    gate_step, next_kind, CaptureGate, CaptureKind, GateAction, GateEvent,
};
use darkshot_core::config::{Config, HotkeyRole, Hotkeys};
use darkshot_core::geometry::Rect;
use darkshot_core::i18n::{t, Lang, Msg};
use darkshot_core::image::Image;
use darkshot_core::tray_logic::MenuCommand;
use dispatch2::{DispatchQueue, DispatchTime};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSScreen};
use std::cell::{Cell, RefCell};
use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Chờ menu thanh trạng thái tắt hẳn rồi mới chụp (như `MENU_CAPTURE_DELAY_MS` của Windows).
const MENU_CAPTURE_DELAY: Duration = Duration::from_millis(250);
/// Tham số dòng lệnh: in tên service Keychain của hồ sơ hiện tại rồi thoát (cho `demo-macos.sh`).
pub const ARG_KEYCHAIN_SERVICE: &str = "--keychain-service";
/// Tham số dòng lệnh: hiện lớp phủ trên ảnh tổng hợp (không chụp, không cần quyền, không đọc cấu
/// hình), in kết quả rồi thoát. Cho test `#[ignore]` chạy tay `tests/overlay_gui.rs`.
pub const ARG_OVERLAY_SELFTEST: &str = "--overlay-selftest";
/// Tham số dòng lệnh: hiện cửa sổ Tùy chọn với cấu hình mẫu (không đọc cấu hình thật, không
/// Keychain, không ghi tệp), in kết quả rồi thoát. Cho test `#[ignore]` `tests/options_gui.rs`.
pub const ARG_OPTIONS_SELFTEST: &str = "--options-selftest";

/// Số lượt lưu/chép đang chạy trên luồng nền.
static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static STATE: RefCell<Option<AppState>> = const { RefCell::new(None) };
    static GATE: Cell<CaptureGate> = const { Cell::new(CaptureGate::Idle) };
    /// Loại của lượt chụp mà cổng đã nhận gần nhất (`next_kind`).
    static KIND: Cell<CaptureKind> = const { Cell::new(CaptureKind::Region) };
    static TRAY: RefCell<Option<Tray>> = const { RefCell::new(None) };
    /// Đã gọi `CGRequestScreenCaptureAccess` trong lần chạy này (chỉ hỏi một lần).
    static PERMISSION_ASKED: Cell<bool> = const { Cell::new(false) };
    /// Kết quả đăng ký phím tắt lần gần nhất, cho cửa sổ Tùy chọn hiện trạng thái từng dòng.
    static FAILURES: RefCell<HotkeyFailures> = RefCell::new(HotkeyFailures::default());
}

/// Lỗi khi giữ khóa một bản chạy.
#[derive(Debug, PartialEq, Eq)]
pub enum LockError {
    AlreadyRunning,
    Io(String),
}

/// Khóa `flock` trên một tệp: chỉ một tiến trình giữ được cho mỗi tệp. Hệ điều hành nhả khóa khi
/// tiến trình kết thúc (kể cả khi bị giết).
#[derive(Debug)]
pub struct InstanceLock {
    _file: File,
}

impl InstanceLock {
    /// Giữ khóa trên `path` (tạo thư mục và tệp nếu chưa có).
    pub fn acquire(path: &Path) -> Result<InstanceLock, LockError> {
        use std::os::fd::AsRawFd;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| LockError::Io(e.to_string()))?;
        }
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|e| LockError::Io(e.to_string()))?;
        // SAFETY: `fd` là mô tả tệp hợp lệ của `file`, sống tới hết hàm; `flock` không giữ con trỏ.
        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if rc == 0 {
            return Ok(InstanceLock { _file: file });
        }
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::EWOULDBLOCK) {
            Err(LockError::AlreadyRunning)
        } else {
            Err(LockError::Io(err.to_string()))
        }
    }
}

/// Đường dẫn tệp khóa một bản chạy trong thư mục dữ liệu (theo hồ sơ cách ly nếu có).
pub fn instance_lock_path(appdata: &Path) -> std::path::PathBuf {
    appdata.join("darkshot").join("instance.lock")
}

/// Thông báo lúc khởi động: cấu hình hỏng/mất khóa, hoặc chưa cấu hình kho R2. Cửa sổ Tùy chọn
/// cũng tự mở ở các trường hợp này (`open_options`); thông báo nói rõ lý do.
pub fn startup_notice(
    lang: Lang,
    notice: Option<ConfigNotice>,
    r2_complete: bool,
) -> Option<(&'static str, String)> {
    let title = t(lang, Msg::MacSetupTitle);
    match notice {
        Some(n) => Some((title, n.message(lang))),
        None if !r2_complete => Some((title, t(lang, Msg::UploadNotConfigured).to_string())),
        None => None,
    }
}

/// Phím tắt dùng lúc khởi động. Chưa có tệp cấu hình, hoặc tệp hỏng/không đọc được (core đã thay
/// bằng `Config::defaults`, dùng PrintScreen): bộ mặc định Mac (quyết định 139). Tệp đọc được
/// (kể cả khi mất khóa R2): đúng phím người dùng đã lưu. Không ghi tệp.
pub fn startup_hotkeys(
    config_file_exists: bool,
    notice: Option<ConfigNotice>,
    loaded: Hotkeys,
) -> Hotkeys {
    match notice {
        _ if !config_file_exists => mac_default_hotkeys(),
        Some(ConfigNotice::Corrupt | ConfigNotice::Unreadable) => mac_default_hotkeys(),
        None | Some(ConfigNotice::SecretLost) => loaded,
    }
}

/// Hành động của một lượt chụp toàn màn hình; `None` với loại không phải toàn màn hình.
pub fn full_screen_action(kind: CaptureKind) -> Option<Action> {
    match kind {
        CaptureKind::SaveFull => Some(Action::Save),
        // Cùng đường tải lên với nút Tải lên của lớp phủ (`act_in_background`, Task 108).
        CaptureKind::UploadFull => Some(Action::Upload),
        CaptureKind::Region | CaptureKind::RepeatLast => None,
    }
}

/// Màn hình cho lượt chụp toàn màn hình: màn hình chứa con trỏ (ngoài mọi màn hình thì màn hình
/// gần nhất, như `pick_display`); không đọc được con trỏ thì màn hình chính, rồi màn hình đầu.
pub fn full_screen_index(
    displays: &[(u32, RectF)],
    cursor: Option<(f64, f64)>,
    main_id: u32,
) -> Option<usize> {
    capture::pick_display(displays, cursor, main_id)
        .or_else(|| capture::pick_display(displays, None, main_id))
}

/// Đăng ký lại phím tắt theo `keys` (lúc khởi động và khi cửa sổ Tùy chọn đóng) và báo **một**
/// thông báo nêu các phím bị chiếm hoặc chưa hỗ trợ trên Mac. Trả về các dòng không đăng ký được.
/// Chỉ gọi trên luồng chính.
pub fn apply_hotkeys(keys: &Hotkeys) -> Vec<HotkeyRole> {
    let failed = hotkey::apply(keys);
    FAILURES.with(|f| *f.borrow_mut() = classify_failed(keys, &failed));
    if let Some((title, body)) = mac_taken_notice(lang::current(), keys, &failed) {
        MacNotifier.notify(NoticeKind::Error, title, &body);
    }
    failed
}

/// Điểm vào. Trả mã thoát của tiến trình.
pub fn run(args: &[String]) -> i32 {
    let profile = paths::profile_override();
    if args.iter().any(|a| a == ARG_KEYCHAIN_SERVICE) {
        // Chỉ in tên service (không chạm Keychain, không hiện giao diện).
        println!("{}", secret::service_for_profile(profile.as_deref()));
        return 0;
    }
    if args.iter().any(|a| a == ARG_OVERLAY_SELFTEST) {
        return overlay_selftest();
    }
    if args.iter().any(|a| a == ARG_OPTIONS_SELFTEST) {
        return options_selftest();
    }
    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("darkshot: must start on the main thread");
        return 1;
    };
    // Một bản chạy cho mỗi hồ sơ (bản thật và bản demo cách ly chạy song song được).
    let _lock = match InstanceLock::acquire(&instance_lock_path(&paths::appdata_dir())) {
        Ok(l) => Some(l),
        Err(LockError::AlreadyRunning) => {
            eprintln!("darkshot: already running");
            return 0;
        }
        Err(LockError::Io(e)) => {
            eprintln!("darkshot: single-instance lock unavailable ({e})");
            None
        }
    };

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    notify::init();

    let config_path = paths::config_path();
    let config_file_exists = config_path.exists();
    let mut state = app_state::startup(
        &config_path,
        &KeychainStore::for_profile(profile.as_deref()),
        &paths::default_save_dir(),
        lang::os_default(),
    );
    state.config.hotkeys = startup_hotkeys(
        config_file_exists,
        state.config_notice,
        state.config.hotkeys,
    );
    let hotkeys = state.config.hotkeys;
    lang::set(state.config.language);
    notify::set_show_success(state.config.show_success_notices);
    let first_notice = startup_notice(
        state.config.language,
        state.config_notice,
        state.config.r2.is_complete(),
    );
    STATE.with(|s| *s.borrow_mut() = Some(state));
    TRAY.with(|t| *t.borrow_mut() = Some(Tray::new(mtm, on_command)));
    if let Some((title, body)) = first_notice {
        MacNotifier.notify(NoticeKind::Info, title, &body);
    }
    hotkey::init(on_hotkey);
    apply_hotkeys(&hotkeys);
    // Cấu hình mang từ Windows dùng PrintScreen/ScrollLock/Pause: trên Mac là F13–F15, nhiều bàn
    // phím không có. Nhắc một lần lúc khởi động (cửa sổ Tùy chọn cũng ghi rõ ở dòng phím tắt).
    if let Some((title, body)) = mac_windows_keys_notice(lang::current(), &hotkeys) {
        MacNotifier.notify(NoticeKind::Info, title, &body);
    }
    // Lần chạy đầu hoặc cấu hình hỏng/mất khóa: mở thẳng tab R2 khi vòng lặp chính bắt đầu chạy.
    let needs_settings = STATE.with(|s| {
        s.borrow()
            .as_ref()
            .is_some_and(|st| app_state::needs_settings_on_start(st, config_file_exists))
    });
    if needs_settings {
        DispatchQueue::main().exec_async(|| open_options(OpenReason::NeedsR2));
    }

    app.run();
    0
}

/// Ảnh tổng hợp cho `--overlay-selftest`: dải màu theo hai trục (thấy ngay ảnh có bị lật hay
/// lệch không), đục.
pub fn selftest_image(w: u32, h: u32) -> Image {
    let mut bgra = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h {
        for x in 0..w {
            let b = (u64::from(x) * 255 / u64::from(w.max(1))) as u8;
            let g = (u64::from(y) * 255 / u64::from(h.max(1))) as u8;
            bgra.extend_from_slice(&[b, g, 96, 255]);
        }
    }
    Image {
        width: w,
        height: h,
        bgra,
    }
}

/// `--overlay-selftest`: mỗi `NSScreen` một ảnh tổng hợp đúng cỡ pixel, mở lớp phủ, chờ nó đóng.
fn overlay_selftest() -> i32 {
    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("darkshot: must start on the main thread");
        return 1;
    };
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    lang::set(lang::os_default());
    let screens = NSScreen::screens(mtm).to_vec();
    let Some(primary_h) = screens.first().map(|s| s.frame().size.height) else {
        eprintln!("darkshot: no screen");
        return 1;
    };
    let mut shots = Vec::with_capacity(screens.len());
    for (i, s) in screens.iter().enumerate() {
        let f = s.frame();
        let frame = RectF {
            x: f.origin.x,
            y: f.origin.y,
            w: f.size.width,
            h: f.size.height,
        };
        let frame_pt = cocoa_to_top_left(frame, primary_h);
        let scale = s.backingScaleFactor();
        let Some((w, h)) = capture::pixel_size(frame_pt, scale) else {
            eprintln!("darkshot: bad screen geometry");
            return 1;
        };
        shots.push(DisplayShot {
            image: selftest_image(w, h),
            frame_pt,
            scale,
            display_id: i as u32,
        });
    }
    if let Err(e) = overlay::open(mtm, shots, Some(0), selftest_closed) {
        eprintln!("darkshot: cannot open the overlay ({e:?})");
        return 1;
    }
    app.run();
    0
}

/// Cấu hình mẫu cho `--options-selftest`: không đọc tệp hay Keychain, không có khóa.
pub fn selftest_config() -> Config {
    let mut c = Config::defaults(&std::env::temp_dir(), lang::os_default());
    c.hotkeys = mac_default_hotkeys();
    c.r2.account_id = "sample-account".into();
    c.r2.access_key_id = "sample-key-id".into();
    c.r2.bucket = "sample-bucket".into();
    c.r2.public_base_url = "https://example.invalid".into();
    c
}

/// `--options-selftest`: mở cửa sổ Tùy chọn với cấu hình mẫu; OK/Hủy in kết quả rồi thoát. Lưu
/// không ghi gì (không tệp, không Keychain).
fn options_selftest() -> i32 {
    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("darkshot: must start on the main thread");
        return 1;
    };
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let config = selftest_config();
    lang::set(config.language);
    options::open(
        mtm,
        Env {
            config,
            failures: HotkeyFailures::default(),
            reason: OpenReason::User,
            on_save: |_| Ok(()),
            on_close: |saved| {
                println!("options: {}", if saved { "saved" } else { "cancelled" });
                std::process::exit(0);
            },
        },
    );
    app.run();
    0
}

fn selftest_closed(outcome: Outcome) {
    match outcome {
        Outcome::Cancelled => println!("overlay: cancelled"),
        Outcome::Act { action, rect, .. } => {
            println!("overlay: {action:?} {}x{}", rect.w, rect.h)
        }
    }
    std::process::exit(0);
}

fn on_command(cmd: MenuCommand) {
    match cmd {
        MenuCommand::Capture => request(GateEvent::MenuCapture, CaptureKind::Region),
        MenuCommand::Options => open_options(OpenReason::User),
        MenuCommand::Quit => quit(),
        // Lệnh chưa làm trên Mac không có trong menu (`tray::implemented`).
        _ => {}
    }
}

/// Mở cửa sổ Tùy chọn (hoặc đưa cửa sổ đang mở ra trước). Luồng chính.
fn open_options(reason: OpenReason) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(config) = STATE.with(|s| s.borrow().as_ref().map(|a| a.config.clone())) else {
        return;
    };
    options::open(
        mtm,
        Env {
            config,
            failures: FAILURES.with(|f| f.borrow().clone()),
            reason,
            on_save: save_options,
            on_close: options_closed,
        },
    );
}

/// Lưu từ cửa sổ Tùy chọn: ghi tệp (khóa mới vào Keychain qua `config::save`), rồi cập nhật cấu
/// hình đang chạy, ngôn ngữ, cờ thông báo và menu. Lỗi trả về là câu cho người dùng (không có
/// khóa); khi lỗi, cấu hình cũ vẫn dùng. Phím tắt đăng ký lại ở `options_closed`.
fn save_options(cfg: &Config) -> Result<(), String> {
    let profile = paths::profile_override();
    persist(
        &paths::config_path(),
        cfg,
        &KeychainStore::for_profile(profile.as_deref()),
    )?;
    let old_lang = lang::current();
    STATE.with(|s| {
        if let Some(st) = s.borrow_mut().as_mut() {
            st.config = cfg.clone();
            // Tệp giờ khớp cấu hình trong bộ nhớ.
            st.config_notice = None;
        }
    });
    lang::set(cfg.language);
    notify::set_show_success(cfg.show_success_notices);
    if cfg.language != old_lang {
        if let Some(mtm) = MainThreadMarker::new() {
            TRAY.with(|t| {
                if let Some(tray) = t.borrow().as_ref() {
                    tray.rebuild(mtm);
                }
            });
        }
    }
    Ok(())
}

/// Cửa sổ Tùy chọn đã đóng (lưu hay hủy): đăng ký lại phím tắt của cấu hình đang chạy (cửa sổ đã
/// gỡ chúng lúc mở) và báo phím bị chiếm hay chưa hỗ trợ.
fn options_closed(_saved: bool) {
    let keys = STATE.with(|s| s.borrow().as_ref().map(|a| a.config.hotkeys));
    if let Some(keys) = keys {
        apply_hotkeys(&keys);
    }
}

/// Phím tắt toàn cục đã bấm (luồng chính, trong bộ xử lý sự kiện Carbon). `RepeatLast` không
/// bao giờ được đăng ký trên Mac (`keymap::mac_kind`, quyết định 135); nếu có cũng bỏ qua.
fn on_hotkey(role: HotkeyRole) {
    if let Some(kind) = mac_kind(role) {
        request(GateEvent::Hotkey, kind);
    }
}

/// Lối vào duy nhất của cổng chụp (trên luồng chính). `kind` là loại chụp đang yêu cầu; chỉ khi
/// cổng nhận yêu cầu thì loại của lượt chụp mới đổi (`next_kind`).
fn request(ev: GateEvent, kind: CaptureKind) {
    let (next, act) = gate_step(GATE.with(|g| g.get()), ev);
    GATE.with(|g| g.set(next));
    KIND.with(|k| k.set(next_kind(k.get(), kind, act)));
    match act {
        GateAction::StartTimer => {
            let when = DispatchTime::NOW.time(MENU_CAPTURE_DELAY.as_nanos() as i64);
            // Khối chạy trên luồng chính khi tới giờ.
            if DispatchQueue::main()
                .after(when, || request(GateEvent::TimerFired, CaptureKind::Region))
                .is_err()
            {
                // Không hẹn được giờ: chụp ngay thay vì để cổng kẹt ở PendingMenu.
                request(GateEvent::TimerFired, CaptureKind::Region);
            }
        }
        GateAction::CaptureNow => start_capture(),
        // Cổng không mở Tùy chọn: mở từ menu, khi khởi động và khi thiếu R2 (`open_options`).
        GateAction::OpenSettings
        | GateAction::FocusSettings
        | GateAction::None
        | GateAction::Ignore => {}
    }
}

/// Lượt chụp đã xong (tương ứng lớp phủ đóng): cổng về rảnh.
fn finish_capture() {
    let (next, _) = gate_step(GATE.with(|g| g.get()), GateEvent::OverlayClosed);
    GATE.with(|g| g.set(next));
}

fn notify_error(title: &str, body: &str) {
    MacNotifier.notify(NoticeKind::Error, title, body);
}

fn start_capture() {
    if !capture::screen_capture_allowed() {
        // Hỏi quyền một lần mỗi lần chạy; mọi lần sau chỉ nhắc cách bật trong Cài đặt.
        if !PERMISSION_ASKED.with(|a| a.replace(true)) {
            capture::request_screen_capture_access();
        }
        let (title, body) =
            capture::capture_error_notice(lang::current(), &CaptureError::PermissionDenied);
        notify_error(title, body);
        finish_capture();
        return;
    }
    let include_cursor = STATE.with(|s| s.borrow().as_ref().map(|a| a.config.include_cursor));
    let include_cursor = include_cursor.unwrap_or(false);
    // Vị trí con trỏ lúc cổng cho chụp (không phải lúc chụp xong) chọn màn hình nhận phím trước.
    let cursor = capture::cursor_location();
    let main_id = capture::main_display_id();
    let full = full_screen_action(KIND.with(|k| k.get()));
    let spawned = std::thread::Builder::new()
        .name("darkshot-capture".into())
        .spawn(move || {
            let result = capture::capture_displays(include_cursor);
            DispatchQueue::main().exec_async(move || match full {
                Some(action) => on_full_captured(result, cursor, main_id, action),
                None => on_captured(result, cursor, main_id),
            });
        });
    if let Err(e) = spawned {
        eprintln!("darkshot: cannot start the capture thread ({e})");
        let (title, body) =
            capture::capture_error_notice(lang::current(), &CaptureError::Failed(e.to_string()));
        notify_error(title, body);
        finish_capture();
    }
}

/// Kết quả chụp, trên luồng chính.
fn on_captured(
    result: Result<Vec<DisplayShot>, CaptureError>,
    cursor: Option<(f64, f64)>,
    main_id: u32,
) {
    let shots = match result {
        Ok(s) if !s.is_empty() => s,
        Ok(_) => {
            report_capture_error(&CaptureError::NoDisplay);
            return;
        }
        Err(e) => {
            report_capture_error(&e);
            return;
        }
    };
    let displays: Vec<(u32, RectF)> = shots.iter().map(|s| (s.display_id, s.frame_pt)).collect();
    // Màn hình chứa con trỏ lúc yêu cầu chụp nhận phím trước.
    let focus = capture::pick_display(&displays, cursor, main_id);
    let Some(mtm) = MainThreadMarker::new() else {
        finish_capture();
        return;
    };
    if let Err(e) = overlay::open(mtm, shots, focus, on_overlay_closed) {
        report_capture_error(&CaptureError::Failed(format!("overlay: {e:?}")));
    }
}

/// Kết quả chụp của lượt toàn màn hình, trên luồng chính: lấy màn hình chứa con trỏ lúc yêu cầu,
/// cổng về rảnh, rồi chạy `action` trên luồng nền.
fn on_full_captured(
    result: Result<Vec<DisplayShot>, CaptureError>,
    cursor: Option<(f64, f64)>,
    main_id: u32,
    action: Action,
) {
    let mut shots = match result {
        Ok(s) if !s.is_empty() => s,
        Ok(_) => {
            report_capture_error(&CaptureError::NoDisplay);
            return;
        }
        Err(e) => {
            report_capture_error(&e);
            return;
        }
    };
    let displays: Vec<(u32, RectF)> = shots.iter().map(|s| (s.display_id, s.frame_pt)).collect();
    let Some(i) = full_screen_index(&displays, cursor, main_id).filter(|i| *i < shots.len()) else {
        report_capture_error(&CaptureError::NoDisplay);
        return;
    };
    let image = shots.swap_remove(i).image;
    drop(shots);
    finish_capture();
    let rect = Rect {
        x: 0,
        y: 0,
        w: i32::try_from(image.width).unwrap_or(i32::MAX),
        h: i32::try_from(image.height).unwrap_or(i32::MAX),
    };
    act_in_background(action, image, rect);
}

/// Lớp phủ đã đóng (mọi đường): cổng về rảnh, rồi làm hành động trên luồng nền.
fn on_overlay_closed(outcome: Outcome) {
    finish_capture();
    if let Outcome::Act {
        action,
        image,
        rect,
    } = outcome
    {
        act_in_background(action, image, rect);
    }
}

fn report_capture_error(e: &CaptureError) {
    if let CaptureError::Failed(why) = e {
        eprintln!("darkshot: capture failed ({why})");
    }
    let (title, body) = capture::capture_error_notice(lang::current(), e);
    notify_error(title, body);
    finish_capture();
}

/// Làm `action` trên vùng `rect` (pixel) của `image`, không chặn luồng chính. Lưu/chép: cắt rồi
/// `actions::run_action` trên luồng `darkshot-action`. Tải lên: `start_upload`.
fn act_in_background(action: Action, image: Image, rect: Rect) {
    let Some(config) = STATE.with(|s| s.borrow().as_ref().map(|a| a.config.clone())) else {
        // Không có cấu hình: không thể biết thư mục lưu; báo thay vì im lặng làm mất ảnh.
        notify_error(
            t(lang::current(), Msg::SaveFailedTitle),
            t(lang::current(), Msg::StepsWorkerFailed),
        );
        return;
    };
    let lang = lang::current();
    if action == Action::Upload {
        start_upload(&config, image, rect, lang);
        return;
    }
    let (save_dir, expiry, upload_format, jpeg_quality) = (
        config.save_dir.clone(),
        config.expiry,
        config.upload_format,
        config.jpeg_quality,
    );
    IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
    let spawned = std::thread::Builder::new()
        .name("darkshot-action".into())
        .spawn(move || {
            let ctx = Ctx {
                save_dir: &save_dir,
                expiry,
                now: timeutil::local_now(),
                uploader: None,
                clipboard: &PasteboardClipboard,
                notifier: &MacNotifier,
                lang,
                upload_format,
                jpeg_quality,
            };
            let cropped = image.crop(rect);
            let _ = run_action(&ctx, action, &cropped);
            IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
        });
    if let Err(e) = spawned {
        IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
        eprintln!("darkshot: cannot start the action thread ({e})");
        notify_error(
            t(
                lang,
                if action == Action::Copy {
                    Msg::CopyFailedTitle
                } else {
                    Msg::SaveFailedTitle
                },
            ),
            t(lang, Msg::StepsWorkerFailed),
        );
    }
}

/// Tải lên (luồng chính chỉ cắt ảnh và ghi sổ; mạng, mã hóa và ghi tệp ở luồng `darkshot-upload`).
/// Ảnh vào sổ `PendingUploads` trước khi chạy, nên thoát giữa chừng vẫn lưu được (`quit`).
/// Chưa cấu hình R2: `run_action` lưu ảnh vào máy và báo; sau đó luồng nhờ luồng chính mở Tùy chọn
/// tab R2 (`needs_settings`).
fn start_upload(config: &Config, image: Image, rect: Rect, lang: Lang) {
    let image = Arc::new(upload::image_for_upload(image, rect));
    let deps = upload::Deps {
        uploader: upload::uploader_for(config),
        clipboard: Arc::new(PasteboardClipboard),
        notifier: Arc::new(MacNotifier),
        clock: timeutil::local_now,
        lang,
        // Chạy trên luồng tải lên: AppKit chỉ chạm ở luồng chính qua hàng đợi chính.
        on_needs_settings: Box::new(|| {
            DispatchQueue::main().exec_async(|| open_options(OpenReason::NeedsR2));
        }),
    };
    let _ = upload::start(
        &upload::registry(),
        upload::UploadParams::from_config(config),
        deps,
        image,
    );
}

/// Thoát sạch, trong một hạn chót chung `EXIT_BUDGET`. Thứ tự: (1) lưu ảnh của mọi lượt tải lên
/// còn dở vào thư mục lưu (`upload::save_on_exit`: chỉ ghi tệp, KHÔNG mạng, không chờ tải lên);
/// (2) chờ các lượt lưu/chép đang chạy tới hạn chót; (3) bỏ biểu tượng và kết thúc. Việc (1) chạy
/// trước và không bị cắt bởi hạn chót: ghi PNG cục bộ là cách duy nhất để không mất ảnh.
fn quit() {
    let deadline = Instant::now() + EXIT_BUDGET;
    overlay::cancel();
    hotkey::unregister_all();
    let saved = upload::save_on_exit(&upload::registry(), timeutil::local_now(), lang::current());
    if saved > 0 {
        eprintln!("darkshot: saved {saved} unfinished upload(s) to the save folder");
    }
    upload::wait_idle(deadline, || IN_FLIGHT.load(Ordering::SeqCst) > 0);
    // Không hủy `Tray` ở đây: hàm này chạy bên trong phương thức của đích menu (`MenuTarget`),
    // hủy đích lúc đó là dùng-sau-giải-phóng. Tiến trình kết thúc ngay sau đó.
    TRAY.with(|t| {
        if let Some(tray) = t.borrow().as_ref() {
            tray.remove();
        }
    });
    if let Some(mtm) = MainThreadMarker::new() {
        NSApplication::sharedApplication(mtm).terminate(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("darkshot_mac_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn selftest_image_is_an_opaque_gradient() {
        let img = selftest_image(300, 200);
        assert!(img.is_valid());
        assert_eq!((img.width, img.height), (300, 200));
        assert!(img.bgra.chunks(4).all(|p| p[3] == 255));
        // Góc trên trái tối, góc dưới phải sáng: ảnh lật hay lệch thấy ngay.
        assert_eq!(&img.bgra[..4], &[0, 0, 96, 255]);
        let last = img.bgra.len() - 4;
        assert!(img.bgra[last] > 250 && img.bgra[last + 1] > 250);
    }

    #[test]
    fn second_instance_lock_is_refused_until_released() {
        let dir = temp_dir("lock");
        let path = instance_lock_path(&dir);
        let first = InstanceLock::acquire(&path).expect("first lock");
        assert!(path.exists());
        assert_eq!(
            InstanceLock::acquire(&path).unwrap_err(),
            LockError::AlreadyRunning
        );
        drop(first);
        let again = InstanceLock::acquire(&path).expect("lock after release");
        drop(again);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn different_profiles_lock_independently() {
        let a = temp_dir("lock_a");
        let b = temp_dir("lock_b");
        let la = InstanceLock::acquire(&instance_lock_path(&a)).expect("a");
        let lb = InstanceLock::acquire(&instance_lock_path(&b)).expect("b");
        drop((la, lb));
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    #[test]
    fn lock_path_is_under_darkshot_dir() {
        assert_eq!(
            instance_lock_path(Path::new("/x/Application Support")),
            Path::new("/x/Application Support/darkshot/instance.lock")
        );
    }

    #[test]
    fn startup_notice_rules() {
        for l in Lang::ALL {
            // Cấu hình đủ: không báo gì.
            assert_eq!(startup_notice(l, None, true), None);
            // Thiếu R2 (kể cả chưa có tệp cấu hình): nhắc cấu hình kho.
            assert_eq!(
                startup_notice(l, None, false),
                Some((
                    t(l, Msg::MacSetupTitle),
                    t(l, Msg::UploadNotConfigured).to_string()
                ))
            );
            // Cấu hình hỏng hay mất khóa: báo đúng lý do, ưu tiên hơn thiếu R2.
            for n in [
                ConfigNotice::Corrupt,
                ConfigNotice::SecretLost,
                ConfigNotice::Unreadable,
            ] {
                for r2 in [true, false] {
                    assert_eq!(
                        startup_notice(l, Some(n), r2),
                        Some((t(l, Msg::MacSetupTitle), n.message(l)))
                    );
                }
            }
        }
    }

    #[test]
    fn startup_hotkeys_use_mac_defaults_without_a_readable_file() {
        let mut saved = Hotkeys::default();
        saved.save_full.enabled = true;
        // Chưa có tệp: bộ mặc định Mac, dù core trả gì.
        assert_eq!(startup_hotkeys(false, None, saved), mac_default_hotkeys());
        // Tệp hỏng / không đọc được: core đã thay bằng mặc định Windows (PrintScreen).
        for n in [ConfigNotice::Corrupt, ConfigNotice::Unreadable] {
            assert_eq!(startup_hotkeys(true, Some(n), saved), mac_default_hotkeys());
        }
        // Tệp đọc được (kể cả khi mất khóa R2): giữ đúng phím đã lưu, kể cả PrintScreen.
        assert_eq!(startup_hotkeys(true, None, saved), saved);
        assert_eq!(
            startup_hotkeys(true, Some(ConfigNotice::SecretLost), saved),
            saved
        );
    }

    #[test]
    fn full_screen_kinds_map_to_actions() {
        assert_eq!(
            full_screen_action(CaptureKind::SaveFull),
            Some(Action::Save)
        );
        assert_eq!(
            full_screen_action(CaptureKind::UploadFull),
            Some(Action::Upload)
        );
        assert_eq!(full_screen_action(CaptureKind::Region), None);
        assert_eq!(full_screen_action(CaptureKind::RepeatLast), None);
    }

    #[test]
    fn full_screen_picks_the_cursor_display_or_main() {
        let r = |x: f64| RectF {
            x,
            y: 0.0,
            w: 100.0,
            h: 100.0,
        };
        let displays = [(7, r(0.0)), (9, r(100.0))];
        assert_eq!(
            full_screen_index(&displays, Some((150.0, 50.0)), 7),
            Some(1)
        );
        assert_eq!(full_screen_index(&displays, Some((10.0, 50.0)), 9), Some(0));
        // Con trỏ ngoài mọi màn hình: màn hình gần nhất; không đọc được con trỏ: màn hình chính.
        assert_eq!(
            full_screen_index(&displays, Some((-500.0, 0.0)), 9),
            Some(0)
        );
        assert_eq!(full_screen_index(&displays, None, 9), Some(1));
        // Không thấy màn hình chính: màn hình đầu.
        assert_eq!(full_screen_index(&displays, None, 42), Some(0));
        assert_eq!(full_screen_index(&[], Some((0.0, 0.0)), 7), None);
    }

    /// Phím tắt và menu dùng chung cổng: phím khi đang chụp (lớp phủ mở) bị bỏ qua và không đổi
    /// loại của lượt đang chạy; phím khi menu đang chờ thì chụp ngay theo loại của phím.
    #[test]
    fn hotkeys_share_the_gate_with_the_menu() {
        use CaptureGate::*;
        let (s, a) = gate_step(Idle, GateEvent::Hotkey);
        assert_eq!((s, a), (Capturing, GateAction::CaptureNow));
        let kind = next_kind(CaptureKind::Region, CaptureKind::SaveFull, a);
        assert_eq!(kind, CaptureKind::SaveFull);
        let (s2, a2) = gate_step(s, GateEvent::Hotkey);
        assert_eq!((s2, a2), (Capturing, GateAction::Ignore));
        assert_eq!(
            next_kind(kind, CaptureKind::Region, a2),
            CaptureKind::SaveFull
        );
        let (s, a) = gate_step(PendingMenu, GateEvent::Hotkey);
        assert_eq!((s, a), (Capturing, GateAction::CaptureNow));
        assert_eq!(gate_step(s, GateEvent::TimerFired).1, GateAction::Ignore);
    }

    #[test]
    fn gate_never_allows_two_captures() {
        use CaptureGate::*;
        // Menu → chờ → chụp; trong lúc chụp mọi yêu cầu khác bị bỏ qua tới khi xong.
        let (s, a) = gate_step(Idle, GateEvent::MenuCapture);
        assert_eq!((s, a), (PendingMenu, GateAction::StartTimer));
        assert_eq!(gate_step(s, GateEvent::MenuCapture).1, GateAction::Ignore);
        let (s, a) = gate_step(s, GateEvent::TimerFired);
        assert_eq!((s, a), (Capturing, GateAction::CaptureNow));
        assert_eq!(gate_step(s, GateEvent::MenuCapture).1, GateAction::Ignore);
        assert_eq!(gate_step(s, GateEvent::TimerFired).1, GateAction::Ignore);
        assert_eq!(
            gate_step(s, GateEvent::OverlayClosed),
            (Idle, GateAction::None)
        );
    }
}
