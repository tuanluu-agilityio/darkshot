//! Vỏ ứng dụng macOS: `NSApplication` chính sách `accessory` (không biểu tượng Dock), vòng lặp
//! chính, một bản chạy cho mỗi hồ sơ, cổng chụp, nối cấu hình.
//!
//! Luồng: mọi trạng thái ứng dụng chỉ sống trên luồng chính (`thread_local`). Việc lâu (chụp,
//! mã hóa PNG, ghi tệp) chạy trên luồng nền; kết quả về luồng chính qua hàng đợi chính
//! (`DispatchQueue::main`). Không gọi AppKit từ luồng nền.
//!
//! Đường chụp tạm của Task 104 (Task 105 thay bằng lớp phủ chọn vùng): "Chụp" lưu cả màn hình
//! chứa con trỏ ra thư mục lưu.

use crate::capture::{self, CaptureError, DisplayShot};
use crate::clipboard::PasteboardClipboard;
use crate::coords::RectF;
use crate::notify::{self, MacNotifier};
use crate::secret::{self, KeychainStore};
use crate::tray::Tray;
use crate::{lang, paths, timeutil};
use darkshot_core::actions::{run_action, Action, Ctx, NoticeKind, Notifier};
use darkshot_core::app_state::{self, AppState, ConfigNotice};
use darkshot_core::capture_gate::{gate_step, CaptureGate, GateAction, GateEvent};
use darkshot_core::i18n::{t, Lang, Msg};
use darkshot_core::image::Image;
use darkshot_core::tray_logic::MenuCommand;
use dispatch2::{DispatchQueue, DispatchTime};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
use std::cell::{Cell, RefCell};
use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Chờ menu thanh trạng thái tắt hẳn rồi mới chụp (như `MENU_CAPTURE_DELAY_MS` của Windows).
const MENU_CAPTURE_DELAY: Duration = Duration::from_millis(250);
/// Thời gian tối đa chờ các lượt lưu đang chạy khi thoát.
const QUIT_BUDGET: Duration = Duration::from_secs(3);
/// Tham số dòng lệnh: in tên service Keychain của hồ sơ hiện tại rồi thoát (cho `demo-macos.sh`).
pub const ARG_KEYCHAIN_SERVICE: &str = "--keychain-service";

/// Số lượt lưu đang chạy trên luồng nền.
static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static STATE: RefCell<Option<AppState>> = const { RefCell::new(None) };
    static GATE: Cell<CaptureGate> = const { Cell::new(CaptureGate::Idle) };
    static TRAY: RefCell<Option<Tray>> = const { RefCell::new(None) };
    /// Đã gọi `CGRequestScreenCaptureAccess` trong lần chạy này (chỉ hỏi một lần).
    static PERMISSION_ASKED: Cell<bool> = const { Cell::new(false) };
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

/// Thông báo lúc khởi động: cấu hình hỏng/mất khóa, hoặc chưa cấu hình kho R2. Chưa có cửa sổ
/// Tùy chọn trên Mac nên chỉ thông báo.
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

/// Điểm vào. Trả mã thoát của tiến trình.
pub fn run(args: &[String]) -> i32 {
    let profile = paths::profile_override();
    if args.iter().any(|a| a == ARG_KEYCHAIN_SERVICE) {
        // Chỉ in tên service (không chạm Keychain, không hiện giao diện).
        println!("{}", secret::service_for_profile(profile.as_deref()));
        return 0;
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
    let state = app_state::startup(
        &config_path,
        &KeychainStore::for_profile(profile.as_deref()),
        &paths::default_save_dir(),
        lang::os_default(),
    );
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

    app.run();
    0
}

fn on_command(cmd: MenuCommand) {
    match cmd {
        MenuCommand::Capture => request(GateEvent::MenuCapture),
        MenuCommand::Quit => quit(),
        // Lệnh chưa làm trên Mac không có trong menu (`tray::implemented`).
        _ => {}
    }
}

/// Lối vào duy nhất của cổng chụp (trên luồng chính).
fn request(ev: GateEvent) {
    let (next, act) = gate_step(GATE.with(|g| g.get()), ev);
    GATE.with(|g| g.set(next));
    match act {
        GateAction::StartTimer => {
            let when = DispatchTime::NOW.time(MENU_CAPTURE_DELAY.as_nanos() as i64);
            // Khối chạy trên luồng chính khi tới giờ.
            if DispatchQueue::main()
                .after(when, || request(GateEvent::TimerFired))
                .is_err()
            {
                // Không hẹn được giờ: chụp ngay thay vì để cổng kẹt ở PendingMenu.
                request(GateEvent::TimerFired);
            }
        }
        GateAction::CaptureNow => start_capture(),
        // Chưa có cửa sổ Tùy chọn trên Mac.
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
    // Vị trí con trỏ lúc cổng cho chụp (không phải lúc chụp xong) chọn màn hình sẽ lưu.
    let cursor = capture::cursor_location();
    let main_id = capture::main_display_id();
    let spawned = std::thread::Builder::new()
        .name("darkshot-capture".into())
        .spawn(move || {
            let result = capture::capture_displays(include_cursor);
            DispatchQueue::main().exec_async(move || on_captured(result, cursor, main_id));
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
    let shot =
        capture::pick_display(&displays, cursor, main_id).and_then(|i| shots.into_iter().nth(i));
    finish_capture();
    if let Some(shot) = shot {
        save_in_background(shot.image);
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

/// Lưu PNG của ảnh trên luồng nền (`actions::run_action`, `Action::Save`); kết quả báo bằng thông
/// báo của chính `run_action`.
fn save_in_background(image: Image) {
    let Some((save_dir, expiry, upload_format, jpeg_quality)) = STATE.with(|s| {
        s.borrow().as_ref().map(|a| {
            (
                a.config.save_dir.clone(),
                a.config.expiry,
                a.config.upload_format,
                a.config.jpeg_quality,
            )
        })
    }) else {
        return;
    };
    let lang = lang::current();
    IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
    let spawned = std::thread::Builder::new()
        .name("darkshot-save".into())
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
            let _ = run_action(&ctx, Action::Save, &image);
            IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
        });
    if let Err(e) = spawned {
        IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
        eprintln!("darkshot: cannot start the save thread ({e})");
        notify_error(
            t(lang, Msg::SaveFailedTitle),
            t(lang, Msg::StepsWorkerFailed),
        );
    }
}

/// Thoát sạch: chờ các lượt lưu đang chạy (tối đa `QUIT_BUDGET`), bỏ biểu tượng, rồi kết thúc.
fn quit() {
    let deadline = Instant::now() + QUIT_BUDGET;
    while IN_FLIGHT.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
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
