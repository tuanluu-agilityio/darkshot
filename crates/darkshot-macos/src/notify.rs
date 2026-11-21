//! Thông báo hệ thống qua `UNUserNotificationCenter`; triển khai `actions::Notifier`.
//!
//! `UNUserNotificationCenter` chỉ dùng được trong gói `.app` (có bundle id): ngoài gói,
//! `currentNotificationCenter` ném ngoại lệ Objective-C, nên ta kiểm bundle id trước và chỉ ghi
//! một dòng ra `stderr`. Bị từ chối quyền cũng chỉ ghi `stderr`, không crash. Trung tâm thông báo
//! an toàn khi gọi từ mọi luồng, nên `notify` gửi ngay trên luồng gọi (không chạm AppKit).
//!
//! Xin quyền là bất đồng bộ: thông báo phát sinh trước khi có kết quả (ví dụ thông báo lúc khởi
//! động) được xếp hàng trong bộ nhớ (`NoticeGate`) và chỉ gửi khi đã được cấp quyền; bị từ chối
//! thì ghi `stderr`.
//!
//! Chỗ lọc duy nhất: thông báo thành công theo tùy chọn (`actions::notice_visible`), như lớp
//! Windows; lỗi và thông tin luôn hiện.

use block2::RcBlock;
use darkshot_core::actions::{notice_visible, NoticeKind, Notifier};
use darkshot_core::tray_logic::{enqueue_notice, Notice};
use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::{Bool, ProtocolObject};
use objc2::{define_class, msg_send, AnyThread};
use objc2_foundation::{NSBundle, NSError, NSObject, NSObjectProtocol, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
    UNNotificationPresentationOptions, UNNotificationRequest, UNUserNotificationCenter,
    UNUserNotificationCenterDelegate,
};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

/// Hiện thông báo thành công (`Config.show_success_notices`).
static SHOW_SUCCESS: AtomicBool = AtomicBool::new(true);
/// Đã ghi dòng "không gửi được" (lỗi của `addNotificationRequest` sau khi đã có quyền) ra
/// `stderr` (chỉ ghi một lần).
static DENIED_LOGGED: AtomicBool = AtomicBool::new(false);
/// Số thứ tự cho mã thông báo.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub fn set_show_success(v: bool) {
    SHOW_SUCCESS.store(v, Ordering::Relaxed);
}

/// Một dòng `stderr` cho thông báo không hiện được (xuống dòng trong nội dung đổi thành ` / `).
pub fn stderr_line(reason: &str, title: &str, body: &str) -> String {
    let one = |s: &str| {
        s.split(['\r', '\n'])
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join(" / ")
    };
    if body.trim().is_empty() {
        format!("darkshot: {reason}: {}", one(title))
    } else {
        format!("darkshot: {reason}: {} - {}", one(title), one(body))
    }
}

/// Mã của thông báo thứ `n`.
pub fn request_id(n: u64) -> String {
    format!("darkshot-notice-{n}")
}

/// Có chạy trong gói `.app` (có bundle id) không; tính một lần.
fn in_bundle() -> bool {
    static IN_BUNDLE: OnceLock<bool> = OnceLock::new();
    *IN_BUNDLE
        .get_or_init(|| autoreleasepool(|_| NSBundle::mainBundle().bundleIdentifier().is_some()))
}

/// Kết quả xin quyền gửi thông báo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthState {
    /// Chưa có kết quả (đang hỏi, hoặc chưa hỏi).
    Pending,
    Granted,
    Denied,
}

/// Việc làm với một thông báo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// Bị ẩn theo tùy chọn: bỏ.
    Drop,
    /// Ngoài gói `.app` hoặc bị từ chối quyền: ghi `stderr`.
    Stderr,
    /// Chưa có kết quả xin quyền: xếp hàng, gửi khi được cấp quyền.
    Queue,
    /// Gửi ngay tới trung tâm thông báo.
    Post,
}

/// Thông báo có được đưa tới trung tâm thông báo của hệ thống không (ngay hoặc sau khi xếp
/// hàng): chỉ khi hiện theo tùy chọn và đang chạy trong gói `.app`.
pub fn reaches_system(kind: NoticeKind, show_success: bool, in_bundle: bool) -> bool {
    notice_visible(kind, show_success) && in_bundle
}

/// Quyết định cho một thông báo.
pub fn route(kind: NoticeKind, show_success: bool, in_bundle: bool, auth: AuthState) -> Route {
    if !notice_visible(kind, show_success) {
        return Route::Drop;
    }
    if !reaches_system(kind, show_success, in_bundle) {
        return Route::Stderr;
    }
    match auth {
        AuthState::Pending => Route::Queue,
        AuthState::Granted => Route::Post,
        AuthState::Denied => Route::Stderr,
    }
}

/// Trạng thái quyền và hàng chờ (tối đa `tray_logic::MAX_QUEUED_NOTICES`, bỏ cái cũ nhất).
#[derive(Debug)]
pub struct NoticeGate {
    auth: AuthState,
    queue: VecDeque<Notice>,
}

impl NoticeGate {
    pub const fn new() -> Self {
        NoticeGate {
            auth: AuthState::Pending,
            queue: VecDeque::new(),
        }
    }

    #[cfg(test)]
    pub fn auth(&self) -> AuthState {
        self.auth
    }

    /// Quyết định cho một thông báo; `Queue` thì thông báo đã nằm trong hàng.
    pub fn submit(&mut self, n: Notice, show_success: bool, in_bundle: bool) -> Route {
        let r = route(n.kind, show_success, in_bundle, self.auth);
        if r == Route::Queue {
            enqueue_notice(&mut self.queue, n);
        }
        r
    }

    /// Có kết quả xin quyền: ghi lại và trả các thông báo đang chờ theo thứ tự (bên gọi gửi
    /// chúng nếu `granted`, ghi `stderr` nếu không). Gọi lần nữa thì hàng đã rỗng.
    pub fn decide(&mut self, granted: bool) -> Vec<Notice> {
        self.auth = if granted {
            AuthState::Granted
        } else {
            AuthState::Denied
        };
        self.queue.drain(..).collect()
    }
}

impl Default for NoticeGate {
    fn default() -> Self {
        Self::new()
    }
}

static GATE: Mutex<NoticeGate> = Mutex::new(NoticeGate::new());

fn with_gate<R>(f: impl FnOnce(&mut NoticeGate) -> R) -> R {
    let mut g = GATE.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut g)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MacNotifier;

impl Notifier for MacNotifier {
    fn notify(&self, kind: NoticeKind, title: &str, body: &str) {
        let n = Notice {
            kind,
            title: title.to_owned(),
            body: body.to_owned(),
        };
        let show = SHOW_SUCCESS.load(Ordering::Relaxed);
        let bundle = in_bundle();
        // Gửi ngoài khóa: `post` gọi vào hệ thống.
        match with_gate(|g| g.submit(n, show, bundle)) {
            Route::Drop | Route::Queue => {}
            Route::Post => post(title, body),
            Route::Stderr if bundle => {
                eprintln!("{}", stderr_line("notifications not allowed", title, body))
            }
            Route::Stderr => eprintln!("{}", stderr_line("not in an app bundle", title, body)),
        }
    }
}

/// Kết quả xin quyền (từ khối hoàn tất, luồng bất kỳ): xả hàng chờ.
fn on_authorization(granted: bool) {
    let pending = with_gate(|g| g.decide(granted));
    if !granted {
        eprintln!("darkshot: notifications are not allowed (System Settings > Notifications)");
    }
    for n in pending {
        if granted {
            post(&n.title, &n.body);
        } else {
            eprintln!(
                "{}",
                stderr_line("notifications not allowed", &n.title, &n.body)
            );
        }
    }
}

fn post(title: &str, body: &str) {
    autoreleasepool(|_| {
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));
        let id = NSString::from_str(&request_id(NEXT_ID.fetch_add(1, Ordering::Relaxed)));
        let request =
            UNNotificationRequest::requestWithIdentifier_content_trigger(&id, &content, None);
        let (title, body) = (title.to_owned(), body.to_owned());
        let done = RcBlock::new(move |err: *mut NSError| {
            if !err.is_null() && !DENIED_LOGGED.swap(true, Ordering::Relaxed) {
                eprintln!(
                    "{}",
                    stderr_line("notification not delivered", &title, &body)
                );
            }
        });
        UNUserNotificationCenter::currentNotificationCenter()
            .addNotificationRequest_withCompletionHandler(&request, Some(&done));
    });
}

define_class!(
    // SAFETY: NSObject không có yêu cầu riêng khi kế thừa; lớp không cài `Drop`.
    #[unsafe(super(NSObject))]
    #[name = "DarkshotNotificationDelegate"]
    #[ivars = ()]
    struct NotificationDelegate;

    unsafe impl NSObjectProtocol for NotificationDelegate {}

    unsafe impl UNUserNotificationCenterDelegate for NotificationDelegate {
        /// Hiện thông báo cả khi darkshot đang là ứng dụng phía trước (ví dụ ngay sau khi chọn
        /// mục menu).
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion_handler: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion_handler
                .call((UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List,));
        }
    }
);

impl NotificationDelegate {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(());
        // SAFETY: `init` của NSObject trên đối tượng vừa cấp phát.
        unsafe { msg_send![super(this), init] }
    }
}

thread_local! {
    /// Giữ delegate sống (trung tâm thông báo chỉ giữ tham chiếu yếu).
    static DELEGATE: RefCell<Option<Retained<NotificationDelegate>>> = const { RefCell::new(None) };
}

/// Gọi một lần lúc khởi động, trên luồng chính: đặt delegate và xin quyền gửi thông báo (hệ thống
/// chỉ hỏi người dùng ở lần đầu). Ngoài gói `.app` thì chỉ ghi `stderr`.
pub fn init() {
    if !in_bundle() {
        eprintln!("darkshot: notifications are unavailable outside an app bundle");
        return;
    }
    autoreleasepool(|_| {
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let delegate = NotificationDelegate::new();
        center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        DELEGATE.with(|d| *d.borrow_mut() = Some(delegate));
        let done = RcBlock::new(|granted: Bool, _err: *mut NSError| {
            on_authorization(granted.as_bool());
        });
        center.requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
            &done,
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stderr_line_is_one_line() {
        assert_eq!(
            stderr_line("not in an app bundle", "Image saved", "/tmp/a.png"),
            "darkshot: not in an app bundle: Image saved - /tmp/a.png"
        );
        let l = stderr_line("x", "Upload failed", "reason\nSaved to /tmp/a.png\r\n");
        assert!(!l.contains('\n') && !l.contains('\r'), "{l}");
        assert_eq!(
            l,
            "darkshot: x: Upload failed - reason / Saved to /tmp/a.png"
        );
        assert_eq!(stderr_line("x", "T", " "), "darkshot: x: T");
    }

    #[test]
    fn request_ids_are_distinct() {
        assert_eq!(request_id(1), "darkshot-notice-1");
        assert_ne!(request_id(1), request_id(2));
    }

    #[test]
    fn reaches_system_combines_visibility_and_bundle() {
        use NoticeKind::*;
        // Thành công bị ẩn: không bao giờ tới hệ thống.
        assert!(!reaches_system(Success, false, true));
        assert!(!reaches_system(Success, false, false));
        assert!(reaches_system(Success, true, true));
        // Lỗi và thông tin luôn hiện khi trong gói.
        for k in [Error, Info] {
            assert!(reaches_system(k, false, true), "{k:?}");
            assert!(reaches_system(k, true, true), "{k:?}");
        }
        // Ngoài gói: không bao giờ.
        for k in [Success, Info, Error] {
            for show in [true, false] {
                assert!(!reaches_system(k, show, false), "{k:?} {show}");
            }
        }
        // `cargo test` chạy ngoài gói `.app`.
        assert!(!in_bundle());
    }

    #[test]
    fn route_waits_for_authorization() {
        use AuthState::*;
        use NoticeKind::*;
        assert_eq!(route(Info, true, true, Pending), Route::Queue);
        assert_eq!(route(Info, true, true, Granted), Route::Post);
        assert_eq!(route(Info, true, true, Denied), Route::Stderr);
        assert_eq!(route(Error, false, false, Granted), Route::Stderr);
        for a in [Pending, Granted, Denied] {
            assert_eq!(route(Success, false, true, a), Route::Drop, "{a:?}");
            assert_eq!(route(Success, false, false, a), Route::Drop, "{a:?}");
            assert_eq!(route(Error, true, false, a), Route::Stderr, "{a:?}");
        }
    }

    fn n(kind: NoticeKind, title: &str) -> Notice {
        Notice {
            kind,
            title: title.into(),
            body: String::new(),
        }
    }

    #[test]
    fn startup_notice_is_queued_then_posted_when_granted() {
        let mut g = NoticeGate::new();
        assert_eq!(g.auth(), AuthState::Pending);
        // Thông báo lúc khởi động đến trước kết quả xin quyền: xếp hàng, không gửi.
        assert_eq!(
            g.submit(n(NoticeKind::Info, "setup"), true, true),
            Route::Queue
        );
        assert_eq!(
            g.submit(n(NoticeKind::Success, "hidden"), false, true),
            Route::Drop
        );
        assert_eq!(
            g.submit(n(NoticeKind::Error, "e"), true, true),
            Route::Queue
        );
        let pending = g.decide(true);
        assert_eq!(
            pending,
            vec![n(NoticeKind::Info, "setup"), n(NoticeKind::Error, "e")]
        );
        assert_eq!(g.auth(), AuthState::Granted);
        // Sau khi có quyền: gửi ngay; hàng đã rỗng.
        assert_eq!(
            g.submit(n(NoticeKind::Info, "later"), true, true),
            Route::Post
        );
        assert!(g.decide(true).is_empty());
    }

    #[test]
    fn denied_authorization_drains_queue_to_stderr() {
        let mut g = NoticeGate::new();
        g.submit(n(NoticeKind::Info, "setup"), true, true);
        assert_eq!(g.decide(false), vec![n(NoticeKind::Info, "setup")]);
        assert_eq!(g.auth(), AuthState::Denied);
        assert_eq!(
            g.submit(n(NoticeKind::Error, "e"), true, true),
            Route::Stderr
        );
    }

    #[test]
    fn queue_before_authorization_is_bounded() {
        let mut g = NoticeGate::new();
        for i in 0..20 {
            g.submit(n(NoticeKind::Info, &i.to_string()), true, true);
        }
        let pending = g.decide(true);
        assert_eq!(pending.len(), darkshot_core::tray_logic::MAX_QUEUED_NOTICES);
        assert_eq!(pending.last().map(|p| p.title.as_str()), Some("19"));
    }
}
