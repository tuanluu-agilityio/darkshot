//! Thoát an toàn ngoài menu (Task 109, phần A): đăng xuất, tắt máy, khởi động lại, `terminate:`
//! từ hệ thống, `SIGTERM`, `SIGINT` đều đi cùng MỘT chuỗi bước thoát với menu "Thoát", và chuỗi đó
//! chạy đúng một lần dù nhiều đường cùng kích hoạt.
//!
//! Chọn `applicationShouldTerminate:` trả `NSTerminateNow` sau khi chạy xong chuỗi bước (không
//! dùng `NSTerminateLater`, không dựa vào `applicationWillTerminate:`):
//! - Mọi đường thoát có hệ thống tham gia (đăng xuất/tắt máy qua Apple Event "quit", `terminate:`)
//!   đi qua `applicationShouldTerminate:` TRƯỚC khi tiến trình kết thúc, và ở đó còn hủy được
//!   việc kết thúc nếu cần; `applicationWillTerminate:` quá muộn để ép thứ tự và không được gọi
//!   khi tiến trình bị `SIGTERM`.
//! - Mọi bước thoát đều ĐỒNG BỘ và có hạn: ghi PNG cục bộ (không mạng) rồi chờ lượt lưu/chép
//!   tới hạn chót `EXIT_BUDGET`. Không có việc bất đồng bộ nào cần `NSTerminateLater` (trả lời
//!   muộn đòi bơm vòng lặp chính và dễ treo lúc đăng xuất); trả `NSTerminateNow` ngay sau khi
//!   xong là đơn giản và không thể quên `reply`.
//! - Chặn luồng chính tối đa cỡ `EXIT_BUDGET` (hạn chót chung của core, quyết định 124) cộng thời
//!   gian ghi PNG, y như đường menu trước đây.
//!
//! `SIGTERM`/`SIGINT`: bỏ xử lý mặc định (`SIG_IGN`) rồi nhận bằng nguồn `DispatchSource` loại
//! signal trên hàng đợi chính; trình xử lý chạy trên luồng chính trong vòng lặp bình thường, không
//! phải trong ngữ cảnh tín hiệu, nên làm việc phức tạp được. Không có trình xử lý kiểu C.
//!
//! Phần AppKit/dispatch nằm cuối tệp; phần thuần (cờ một lần, thứ tự bước) ở trên và có test.

use darkshot_core::app_flow::EXIT_BUDGET;
use dispatch2::{
    _dispatch_source_type_signal, DispatchObject, DispatchQueue, DispatchRetained, DispatchSource,
};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSApplication, NSApplicationDelegate, NSApplicationTerminateReply};
use objc2_foundation::{NSObject, NSObjectProtocol};
use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

/// Cờ "đã bắt đầu thoát": chỉ lần gọi đầu tiên thắng.
#[derive(Debug, Default)]
pub struct ExitOnce(AtomicBool);

impl ExitOnce {
    pub const fn new() -> ExitOnce {
        ExitOnce(AtomicBool::new(false))
    }

    /// `true` đúng một lần (lần gọi đầu); mọi lần sau `false`.
    pub fn begin(&self) -> bool {
        !self.0.swap(true, Ordering::SeqCst)
    }
}

/// Các bước thoát theo đúng thứ tự của `quit` trước Task 109. Cài đặt thật ở `app.rs`.
pub trait ExitSteps {
    /// Hủy lớp phủ chọn vùng đang mở (nếu có).
    fn cancel_overlay(&mut self);
    /// Gỡ mọi phím tắt toàn cục.
    fn unregister_hotkeys(&mut self);
    /// Lưu PNG các lượt tải lên còn dở (chỉ ghi tệp, KHÔNG mạng). Trả số ảnh đã lưu.
    fn save_pending(&mut self) -> usize;
    /// Chờ các lượt lưu/chép đang chạy, không quá `deadline`.
    fn wait_idle(&mut self, deadline: Instant);
    /// Bỏ biểu tượng khay.
    fn remove_tray(&mut self);
}

/// Chạy chuỗi bước thoát nếu chưa chạy. Trả `true` nếu lần này đã chạy, `false` nếu đã có lần
/// khác chạy trước (không làm gì, đặc biệt không lưu ảnh lần hai).
pub fn run_exit_once(once: &ExitOnce, steps: &mut impl ExitSteps) -> bool {
    if !once.begin() {
        return false;
    }
    // Hạn chót tính từ lúc bắt đầu: bước lưu ảnh chạy trước và không bị cắt (ghi PNG cục bộ là
    // cách duy nhất để không mất ảnh); các bước sau chỉ được dùng phần còn lại.
    let deadline = Instant::now() + EXIT_BUDGET;
    steps.cancel_overlay();
    steps.unregister_hotkeys();
    let saved = steps.save_pending();
    if saved > 0 {
        eprintln!("darkshot: saved {saved} unfinished upload(s) to the save folder");
    }
    steps.wait_idle(deadline);
    steps.remove_tray();
    true
}

// ---------------------------------------------------------------------------------------------
// AppKit / dispatch (chỉ luồng chính)
// ---------------------------------------------------------------------------------------------

/// Hàm thoát dùng chung do `app.rs` cung cấp: chạy chuỗi bước (một lần) rồi yêu cầu `terminate:`.
static ON_SIGNAL: OnceLock<fn()> = OnceLock::new();

define_class!(
    // SAFETY: NSObject không có yêu cầu riêng khi kế thừa; lớp không cài `Drop`. Chỉ dùng trên
    // luồng chính (delegate của NSApplication).
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "DarkshotAppDelegate"]
    #[ivars = fn()]
    struct AppDelegate;

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl NSApplicationDelegate for AppDelegate {
        #[unsafe(method(applicationShouldTerminate:))]
        fn application_should_terminate(
            &self,
            _sender: &NSApplication,
        ) -> NSApplicationTerminateReply {
            // Chạy chuỗi bước thoát (một lần; đã chạy rồi thì không làm gì) rồi cho phép thoát.
            (self.ivars())();
            NSApplicationTerminateReply::TerminateNow
        }
    }
);

impl AppDelegate {
    fn new(mtm: MainThreadMarker, run_exit: fn()) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(run_exit);
        // SAFETY: `init` của NSObject trên đối tượng vừa cấp phát.
        unsafe { msg_send![super(this), init] }
    }
}

thread_local! {
    /// `NSApplication.delegate` chỉ giữ yếu: giữ delegate sống suốt tiến trình.
    static DELEGATE: RefCell<Option<Retained<AppDelegate>>> = const { RefCell::new(None) };
    /// Nguồn tín hiệu phải sống để còn nhận sự kiện.
    static SOURCES: RefCell<Vec<DispatchRetained<DispatchSource>>> = const { RefCell::new(Vec::new()) };
}

/// Trình xử lý nguồn tín hiệu (chạy trên hàng đợi chính, tức luồng chính).
extern "C" fn on_signal_event(_ctx: *mut c_void) {
    if let Some(f) = ON_SIGNAL.get() {
        f();
    }
}

/// Cài delegate (`applicationShouldTerminate:` → `run_exit`) và nhận `SIGTERM`/`SIGINT` qua
/// `DispatchSource` (→ `on_signal`). `run_exit` phải chạy chuỗi bước thoát một lần; `on_signal`
/// chạy nó rồi gọi `terminate:`. Chỉ gọi một lần, trên luồng chính, trước `app.run()`.
pub fn install(mtm: MainThreadMarker, app: &NSApplication, run_exit: fn(), on_signal: fn()) {
    let delegate = AppDelegate::new(mtm, run_exit);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    DELEGATE.with(|d| *d.borrow_mut() = Some(delegate));

    let _ = ON_SIGNAL.set(on_signal);
    for sig in [libc::SIGTERM, libc::SIGINT] {
        // SAFETY: `signal` với `SIG_IGN` chỉ đổi cách xử lý mặc định của tín hiệu (bắt buộc để
        // nguồn `DispatchSource` nhận được tín hiệu); không giữ con trỏ nào.
        unsafe {
            libc::signal(sig, libc::SIG_IGN);
        }
        // SAFETY: loại nguồn là hằng của libdispatch (`_dispatch_source_type_signal`), `handle`
        // là số tín hiệu hợp lệ, `mask` 0 theo tài liệu, hàng đợi chính sống suốt tiến trình.
        let source = unsafe {
            DispatchSource::new(
                std::ptr::addr_of!(_dispatch_source_type_signal).cast_mut(),
                sig as usize,
                0,
                Some(DispatchQueue::main()),
            )
        };
        source.set_event_handler_f(on_signal_event);
        source.activate();
        SOURCES.with(|s| s.borrow_mut().push(source));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::upload;
    use darkshot_core::app_state::PendingUploads;
    use darkshot_core::i18n::Lang;
    use darkshot_core::image::Image;
    use darkshot_core::naming::LocalTime;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicU32;
    use std::sync::Arc;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// Thư mục tạm riêng của một test; dọn khi hết phạm vi.
    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> TempDir {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let d = std::env::temp_dir().join(format!("darkshot_sd109_{}_{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            TempDir(d)
        }
        fn files(&self) -> usize {
            std::fs::read_dir(&self.0)
                .map(|r| r.filter_map(Result::ok).count())
                .unwrap_or(0)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn now() -> LocalTime {
        LocalTime {
            year: 2026,
            month: 11,
            day: 21,
            hour: 10,
            minute: 0,
            second: 0,
        }
    }

    fn img() -> Arc<Image> {
        Arc::new(Image {
            width: 2,
            height: 2,
            bgra: vec![200; 16],
        })
    }

    /// Bước giả ghi lại thứ tự; bước lưu dùng `save_on_exit` thật trên sổ thật.
    struct Rec {
        log: Vec<&'static str>,
        reg: Arc<PendingUploads>,
        deadline_seen: Option<Instant>,
    }
    impl ExitSteps for Rec {
        fn cancel_overlay(&mut self) {
            self.log.push("overlay");
        }
        fn unregister_hotkeys(&mut self) {
            self.log.push("hotkeys");
        }
        fn save_pending(&mut self) -> usize {
            self.log.push("save");
            upload::save_on_exit(&self.reg, now(), Lang::En)
        }
        fn wait_idle(&mut self, deadline: Instant) {
            self.log.push("wait");
            self.deadline_seen = Some(deadline);
        }
        fn remove_tray(&mut self) {
            self.log.push("tray");
        }
    }

    fn rec(reg: &Arc<PendingUploads>) -> Rec {
        Rec {
            log: Vec::new(),
            reg: reg.clone(),
            deadline_seen: None,
        }
    }

    #[test]
    fn flag_wins_only_once() {
        let once = ExitOnce::new();
        assert!(once.begin());
        assert!(!once.begin());
        assert!(!once.begin());
    }

    #[test]
    fn steps_run_in_the_quit_order() {
        let reg = Arc::new(PendingUploads::new());
        let mut r = rec(&reg);
        assert!(run_exit_once(&ExitOnce::new(), &mut r));
        assert_eq!(r.log, ["overlay", "hotkeys", "save", "wait", "tray"]);
    }

    #[test]
    fn wait_deadline_comes_from_exit_budget() {
        let reg = Arc::new(PendingUploads::new());
        let mut r = rec(&reg);
        let before = Instant::now();
        run_exit_once(&ExitOnce::new(), &mut r);
        let d = r.deadline_seen.unwrap();
        assert!(d >= before + EXIT_BUDGET);
        assert!(d <= Instant::now() + EXIT_BUDGET);
    }

    #[test]
    fn second_trigger_does_nothing() {
        let once = ExitOnce::new();
        let reg = Arc::new(PendingUploads::new());
        let mut first = rec(&reg);
        let mut second = rec(&reg);
        assert!(run_exit_once(&once, &mut first));
        assert!(!run_exit_once(&once, &mut second));
        assert!(second.log.is_empty(), "đường thứ hai không chạy bước nào");
    }

    #[test]
    fn pending_uploads_are_saved_exactly_once_across_triggers() {
        let dir = TempDir::new();
        let reg = Arc::new(PendingUploads::new());
        reg.add(img(), dir.0.clone());
        reg.add(img(), dir.0.clone());
        let once = ExitOnce::new();
        // Menu, delegate và tín hiệu cùng kích hoạt: chỉ lần đầu lưu.
        for _ in 0..3 {
            let mut r = rec(&reg);
            run_exit_once(&once, &mut r);
        }
        assert_eq!(dir.files(), 2, "mỗi ảnh dở đúng một tệp, không bản thứ hai");
        assert_eq!(upload::save_on_exit(&reg, now(), Lang::En), 0);
    }

    #[test]
    fn nothing_pending_writes_nothing() {
        let dir = TempDir::new();
        let reg = Arc::new(PendingUploads::new());
        let mut r = rec(&reg);
        run_exit_once(&ExitOnce::new(), &mut r);
        assert_eq!(dir.files(), 0);
    }
}
