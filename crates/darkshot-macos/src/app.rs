//! Vỏ ứng dụng macOS: `NSApplication` chính sách `accessory` (không biểu tượng Dock), vòng lặp
//! chính, một bản chạy cho mỗi hồ sơ, cổng chụp, nối cấu hình.
//!
//! Luồng: mọi trạng thái ứng dụng chỉ sống trên luồng chính (`thread_local`). Việc lâu (chụp,
//! mã hóa PNG, ghi tệp) chạy trên luồng nền; kết quả về luồng chính qua hàng đợi chính
//! (`DispatchQueue::main`). Không gọi AppKit từ luồng nền.
//!
//! "Chụp" mở lớp phủ chọn vùng (`overlay`, Task 105) trên ảnh của mọi màn hình; cổng chụp chỉ
//! về rảnh khi lớp phủ đóng (mọi đường đóng đều gọi `on_overlay_closed`).

use crate::capture::{self, CaptureError, DisplayShot};
use crate::clipboard::PasteboardClipboard;
use crate::coords::{cocoa_to_top_left, RectF};
use crate::notify::{self, MacNotifier};
use crate::overlay::{self, Outcome};
use crate::secret::{self, KeychainStore};
use crate::tray::Tray;
use crate::{lang, paths, timeutil};
use darkshot_core::actions::{run_action, Action, Ctx, NoticeKind, Notifier};
use darkshot_core::app_state::{self, AppState, ConfigNotice};
use darkshot_core::capture_gate::{gate_step, CaptureGate, GateAction, GateEvent};
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
use std::time::{Duration, Instant};

/// Chờ menu thanh trạng thái tắt hẳn rồi mới chụp (như `MENU_CAPTURE_DELAY_MS` của Windows).
const MENU_CAPTURE_DELAY: Duration = Duration::from_millis(250);
/// Thời gian tối đa chờ các lượt lưu/chép đang chạy khi thoát.
const QUIT_BUDGET: Duration = Duration::from_secs(3);
/// Tham số dòng lệnh: in tên service Keychain của hồ sơ hiện tại rồi thoát (cho `demo-macos.sh`).
pub const ARG_KEYCHAIN_SERVICE: &str = "--keychain-service";
/// Tham số dòng lệnh: hiện lớp phủ trên ảnh tổng hợp (không chụp, không cần quyền, không đọc cấu
/// hình), in kết quả rồi thoát. Cho test `#[ignore]` chạy tay `tests/overlay_gui.rs`.
pub const ARG_OVERLAY_SELFTEST: &str = "--overlay-selftest";

/// Số lượt lưu/chép đang chạy trên luồng nền.
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
    if args.iter().any(|a| a == ARG_OVERLAY_SELFTEST) {
        return overlay_selftest();
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
    // Vị trí con trỏ lúc cổng cho chụp (không phải lúc chụp xong) chọn màn hình nhận phím trước.
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

/// Cắt vùng `rect` (pixel) của `image` rồi chạy `actions::run_action` trên luồng nền (mã hóa PNG,
/// ghi tệp, clipboard không chạy trên luồng chính); kết quả báo bằng thông báo của `run_action`.
fn act_in_background(action: Action, image: Image, rect: Rect) {
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

/// Thoát sạch: chờ các lượt lưu đang chạy (tối đa `QUIT_BUDGET`), bỏ biểu tượng, rồi kết thúc.
fn quit() {
    overlay::cancel();
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
