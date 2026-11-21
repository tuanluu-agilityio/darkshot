//! Thông báo hệ thống qua `UNUserNotificationCenter`; triển khai `actions::Notifier`.
//!
//! `UNUserNotificationCenter` chỉ dùng được trong gói `.app` (có bundle id): ngoài gói,
//! `currentNotificationCenter` ném ngoại lệ Objective-C, nên ta kiểm bundle id trước và chỉ ghi
//! một dòng ra `stderr`. Bị từ chối quyền cũng chỉ ghi `stderr`, không crash. Trung tâm thông báo
//! an toàn khi gọi từ mọi luồng, nên `notify` gửi ngay trên luồng gọi (không chạm AppKit).
//!
//! Chỗ lọc duy nhất: thông báo thành công theo tùy chọn (`actions::notice_visible`), như lớp
//! Windows; lỗi và thông tin luôn hiện.

use block2::RcBlock;
use darkshot_core::actions::{notice_visible, NoticeKind, Notifier};
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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;

/// Hiện thông báo thành công (`Config.show_success_notices`).
static SHOW_SUCCESS: AtomicBool = AtomicBool::new(true);
/// Đã ghi dòng "bị từ chối" ra `stderr` (chỉ ghi một lần).
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

#[derive(Clone, Copy, Debug, Default)]
pub struct MacNotifier;

impl Notifier for MacNotifier {
    fn notify(&self, kind: NoticeKind, title: &str, body: &str) {
        if !notice_visible(kind, SHOW_SUCCESS.load(Ordering::Relaxed)) {
            return;
        }
        if !in_bundle() {
            eprintln!("{}", stderr_line("not in an app bundle", title, body));
            return;
        }
        post(title, body);
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
            if !granted.as_bool() {
                eprintln!(
                    "darkshot: notifications are not allowed (System Settings > Notifications)"
                );
            }
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
    fn hidden_success_notices_never_reach_the_system() {
        // Ngoài gói `.app` (cargo test): `notify` chỉ ghi stderr, không chạm trung tâm thông báo.
        assert!(!in_bundle());
        set_show_success(false);
        MacNotifier.notify(NoticeKind::Success, "t", "b");
        MacNotifier.notify(NoticeKind::Error, "t", "b");
        set_show_success(true);
    }
}
