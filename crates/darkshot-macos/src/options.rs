//! Cửa sổ Tùy chọn macOS (Task 107): vòng đời, đọc/ghi biểu mẫu, OK/Hủy, bắt phím, chọn thư mục,
//! kiểm tra kết nối R2. Giao diện ở `options_view`, ô bắt phím ở `hotkey_field`, luật ở
//! `options_model` và `settings_form` của core.
//!
//! Luồng: mọi thứ chạy trên luồng chính; chỉ `R2Uploader::test_connection` chạy trên luồng nền và
//! trả kết quả bằng `DispatchQueue::main`. Phiên (`SESSION`) là `Rc<Inner>`: hàm của AppKit gọi
//! vào đây lấy bản sao `Rc` rồi thả mượn `thread_local` ngay, nên gọi lại đệ quy không gây
//! `BorrowMutError`. Cửa sổ chỉ có một; mở lần hai chỉ đưa nó ra trước.
//!
//! Khóa bí mật: chỉ nằm trong `NSSecureTextField` và trong `Form` (Debug đã che) cho tới khi
//! `app::save_options` đưa nó vào Keychain qua `config::save`. Ô được xóa khi đóng. Hủy hay đóng
//! cửa sổ không ghi gì. Không câu lỗi, tên điều khiển hay log nào chứa khóa.
//!
//! Trong lúc Tùy chọn mở, phím tắt toàn cục bị gỡ (như Windows): nếu không, bấm lại tổ hợp đang
//! đăng ký để đặt lại thì Carbon bắt mất, ô bắt phím không nhận được. `on_close` đăng ký lại.

use crate::keymap::HotkeyFailures;
use crate::lang;
use crate::options_model::{
    error_target, expiry_from_index, form_from_config, hotkey_text, initial_tab_index,
    lang_from_index, row_status, status_msg, visible_roles, CaptureStep, RowStatus,
};
use crate::options_view::{self, Controls, OptionsTarget};
use block2::RcBlock;
use darkshot_core::app_flow::{merge_reason, OpenReason};
use darkshot_core::config::{clamp_jpeg_quality, Config, HotkeyRole, Hotkeys, UploadFormat};
use darkshot_core::i18n::{self, t, Msg, OK_LABEL};
use darkshot_core::settings_form::{apply_form, quality_enabled, Field, Form, FormError, Tab};
use darkshot_core::upload::R2Uploader;
use dispatch2::{DispatchQueue, DispatchTime};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{sel, MainThreadMarker};
use objc2_app_kit::{
    NSAlert, NSApplication, NSButton, NSColor, NSControlStateValueOff, NSControlStateValueOn,
    NSModalResponse, NSModalResponseOK, NSOpenPanel, NSTextField, NSView,
    NSWindowWillCloseNotification,
};
use objc2_foundation::{NSNotificationCenter, NSString, NSURL};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Hạn chót của một lần kiểm tra kết nối (dự phòng nếu luồng nền không báo về).
const TEST_FAILSAFE: Duration = Duration::from_secs(60);

/// Mọi thứ cửa sổ cần từ ứng dụng. Hàm (không phải closure) để không giữ trạng thái ẩn.
pub struct Env {
    /// Cấu hình đang chạy (cấu hình mẫu khi chạy thử).
    pub config: Config,
    /// Kết quả đăng ký phím tắt lần gần nhất, để hiện trạng thái từng dòng.
    pub failures: HotkeyFailures,
    pub reason: OpenReason,
    /// Ghi cấu hình và áp dụng vào ứng dụng đang chạy; lỗi là câu cho người dùng (không có khóa).
    pub on_save: fn(&Config) -> Result<(), String>,
    /// Gọi đúng một lần khi cửa sổ đóng (mọi đường); `saved` đúng khi đã ghi thành công.
    pub on_close: fn(saved: bool),
}

struct Inner {
    controls: Controls,
    target: Retained<OptionsTarget>,
    /// Biểu mẫu gốc: giữ giá trị các trường không có điều khiển trên Mac.
    base_form: Form,
    current: Config,
    roles: Vec<HotkeyRole>,
    /// Phím tắt đang sửa (bật/tắt đọc từ hộp tích lúc OK).
    hotkeys: RefCell<Hotkeys>,
    failures: RefCell<HotkeyFailures>,
    reason: Cell<OpenReason>,
    /// Dòng đang bắt phím.
    capture_row: Cell<Option<usize>>,
    /// Thế hệ của lần kiểm tra kết nối đang chờ (0 = không có).
    test_generation: Cell<u64>,
    alert: RefCell<Option<Retained<NSAlert>>>,
    panel: RefCell<Option<Retained<NSOpenPanel>>>,
    on_save: fn(&Config) -> Result<(), String>,
    on_close: fn(bool),
}

thread_local! {
    static SESSION: RefCell<Option<Rc<Inner>>> = const { RefCell::new(None) };
    /// Phiên đã đóng, chờ hủy ở lượt kế của hàng đợi chính (ta có thể đang ở trong phương thức
    /// của chính các đối tượng đó).
    static RETIRED: RefCell<Vec<Rc<Inner>>> = const { RefCell::new(Vec::new()) };
}

static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

fn ns(s: &str) -> Retained<NSString> {
    NSString::from_str(s)
}

fn inner() -> Option<Rc<Inner>> {
    SESSION.with(|s| s.try_borrow().ok().and_then(|b| b.clone()))
}

fn checked(b: &NSButton) -> bool {
    b.state() == NSControlStateValueOn
}

/// Mở cửa sổ Tùy chọn, hoặc đưa cửa sổ đang mở ra trước (và chuyển sang tab R2 nếu yêu cầu mới
/// là `NeedsR2`, theo `merge_reason`).
pub fn open(mtm: MainThreadMarker, env: Env) {
    if let Some(i) = inner() {
        let merged = merge_reason(i.reason.get(), env.reason);
        if merged != i.reason.get() {
            i.reason.set(merged);
            i.controls
                .tab_view
                .selectTabViewItemAtIndex(initial_tab_index(merged) as isize);
        }
        show(mtm, &i);
        return;
    }
    // Gỡ phím tắt toàn cục trong lúc cửa sổ mở (xem chú thích đầu tệp).
    crate::hotkey::unregister_all();
    let target = OptionsTarget::new(mtm);
    let controls = options_view::build(mtm, &env.config, &target, initial_tab_index(env.reason));
    let roles = visible_roles();
    let inner = Rc::new(Inner {
        base_form: form_from_config(&env.config),
        hotkeys: RefCell::new(env.config.hotkeys),
        failures: RefCell::new(env.failures),
        current: env.config,
        controls,
        target,
        roles,
        reason: Cell::new(env.reason),
        capture_row: Cell::new(None),
        test_generation: Cell::new(0),
        alert: RefCell::new(None),
        panel: RefCell::new(None),
        on_save: env.on_save,
        on_close: env.on_close,
    });
    // Đóng bằng nút đỏ của cửa sổ = Hủy.
    let observer: &AnyObject = &inner.target;
    // SAFETY: đích có bộ chọn `windowClosing:` (nhận một `NSNotification`); tên thông báo là hằng
    // của AppKit; đích được gỡ trong `finish` trước khi bị hủy.
    unsafe {
        let window: &AnyObject = &inner.controls.window;
        NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            observer,
            sel!(windowClosing:),
            Some(NSWindowWillCloseNotification),
            Some(window),
        );
    }
    for row in 0..inner.roles.len() {
        refresh_status(&inner, row);
    }
    SESSION.with(|s| *s.borrow_mut() = Some(inner.clone()));
    show(mtm, &inner);
}

fn show(mtm: MainThreadMarker, i: &Inner) {
    let app = NSApplication::sharedApplication(mtm);
    // Lớp phủ chọn vùng ẩn ứng dụng khi đóng; bỏ ẩn để cửa sổ hiện ra.
    app.unhideWithoutActivation();
    i.controls.window.center();
    i.controls.window.makeKeyAndOrderFront(None);
    app.activate();
}

/// Đóng phiên: gỡ bộ nhận thông báo, xóa ô khóa, ẩn và đóng cửa sổ, báo `on_close`. Hàm duy nhất
/// đóng cửa sổ; gọi nhiều lần vô hại. `close_window` sai khi chính hệ thống đang đóng cửa sổ
/// (nút đỏ): không gọi `close` lồng trong thông báo `willClose`.
fn finish(saved: bool, close_window: bool) {
    let Some(i) = SESSION.with(|s| s.try_borrow_mut().ok().and_then(|mut b| b.take())) else {
        return;
    };
    // Xóa khóa khỏi ô trước khi cửa sổ biến mất.
    i.controls.secret.setStringValue(&ns(""));
    // Bỏ kết quả kiểm tra đang chờ.
    i.test_generation.set(0);
    let observer: &AnyObject = &i.target;
    // SAFETY: gỡ đúng đối tượng đã đăng ký ở `open`.
    unsafe { NSNotificationCenter::defaultCenter().removeObserver(observer) };
    if close_window {
        i.controls.window.orderOut(None);
        i.controls.window.close();
    }
    *i.alert.borrow_mut() = None;
    *i.panel.borrow_mut() = None;
    let on_close = i.on_close;
    RETIRED.with(|r| r.borrow_mut().push(i));
    DispatchQueue::main().exec_async(|| RETIRED.with(|r| r.borrow_mut().clear()));
    on_close(saved);
}

// ---------------------------------------------------------------- đọc biểu mẫu

fn read_form(i: &Inner) -> Form {
    let c = &i.controls;
    let mut f = i.base_form.clone();
    f.save_dir = c.save_dir.stringValue().to_string();
    f.language = lang_from_index(c.language.indexOfSelectedItem(), i.current.language);
    f.show_success_notices = checked(&c.notices);
    f.keep_selection = checked(&c.keep_selection);
    f.include_cursor = checked(&c.include_cursor);
    let mut hk = *i.hotkeys.borrow();
    for (row, role) in i.roles.iter().enumerate() {
        if let Some(b) = c.hk_check.get(row) {
            let mut slot = hk.get(*role);
            slot.enabled = checked(b);
            hk.set(*role, slot);
        }
    }
    f.hotkeys = hk;
    f.upload_format = if checked(&c.jpeg) {
        UploadFormat::Jpeg
    } else {
        UploadFormat::Png
    };
    f.jpeg_quality = clamp_jpeg_quality(c.quality.doubleValue().round() as i64);
    f.account_id = c.account.stringValue().to_string();
    f.access_key_id = c.key_id.stringValue().to_string();
    f.secret_access_key = c.secret.stringValue().to_string();
    f.bucket = c.bucket.stringValue().to_string();
    f.public_base_url = c.url.stringValue().to_string();
    f.expiry = expiry_from_index(c.expiry.indexOfSelectedItem());
    f
}

// ---------------------------------------------------------------- lỗi

fn field_view(i: &Inner, field: Field) -> Option<Retained<NSView>> {
    let c = &i.controls;
    let text = |t: &Retained<NSTextField>| -> Option<Retained<NSView>> {
        Some(Retained::into_super(Retained::into_super(t.clone())))
    };
    match field {
        Field::SaveDir => text(&c.save_dir),
        Field::AccountId => text(&c.account),
        Field::Bucket => text(&c.bucket),
        Field::PublicUrl => text(&c.url),
        Field::HotkeyRow(r) => {
            let row = i.roles.iter().position(|x| *x == r)?;
            c.hk_change
                .get(row)
                .map(|b| Retained::into_super(Retained::into_super(b.clone())))
        }
    }
}

/// Hộp thoại lỗi dạng tấm (không chặn vòng lặp); giữ `alert` sống tới khi đóng cửa sổ.
fn alert(i: &Inner, title: &str, body: &str) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let a = NSAlert::new(mtm);
    a.setMessageText(&ns(title));
    a.setInformativeText(&ns(body));
    a.addButtonWithTitle(&ns(OK_LABEL));
    a.beginSheetModalForWindow_completionHandler(&i.controls.window, None);
    *i.alert.borrow_mut() = Some(a);
}

/// Lỗi của `apply_form`: chuyển tới tab chứa ô sai, đưa tiêu điểm vào ô đó, hiện câu lỗi. Giá trị
/// ở mọi tab giữ nguyên và cửa sổ không đóng.
fn show_form_error(i: &Inner, e: &FormError) {
    let (tab, field) = error_target(e);
    i.controls
        .tab_view
        .selectTabViewItemAtIndex(tab.index() as isize);
    if let Some(v) = field_view(i, field) {
        i.controls.window.makeFirstResponder(Some(&v));
    }
    alert(i, lang::tr(Msg::OptionsTitle), &e.message);
}

// ---------------------------------------------------------------- hành động

/// Nút OK.
pub fn on_ok() {
    let Some(i) = inner() else { return };
    let form = read_form(&i);
    let cfg = match apply_form(&i.current, &form, lang::current()) {
        Ok(c) => c,
        Err(e) => {
            show_form_error(&i, &e);
            return;
        }
    };
    match (i.on_save)(&cfg) {
        Ok(()) => finish(true, true),
        Err(why) => {
            // Không ghi được: giữ cửa sổ mở, cấu hình cũ vẫn dùng.
            let lang = lang::current();
            alert(
                &i,
                t(lang, Msg::OptionsSaveFailedTitle),
                &i18n::options_save_failed_body(lang, &why),
            );
        }
    }
}

/// Nút Hủy: bỏ thay đổi, không ghi tệp.
pub fn on_cancel() {
    finish(false, true);
}

/// Cửa sổ vừa bị đóng bằng nút đỏ (hệ thống đóng, ta không gọi `close` nữa).
pub fn on_window_closed() {
    finish(false, false);
}

/// Nút "Chọn…": hộp chọn thư mục dạng tấm, không chặn vòng lặp.
pub fn on_choose_folder() {
    let Some(i) = inner() else { return };
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    if i.panel.borrow().is_some() {
        return;
    }
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseDirectories(true);
    panel.setCanChooseFiles(false);
    panel.setAllowsMultipleSelection(false);
    panel.setCanCreateDirectories(true);
    panel.setMessage(Some(&ns(lang::tr(Msg::PickFolderTitle))));
    let current = i.controls.save_dir.stringValue().to_string();
    let current = current.trim();
    if !current.is_empty() && std::path::Path::new(current).is_dir() {
        panel.setDirectoryURL(Some(&NSURL::fileURLWithPath(&ns(current))));
    }
    // Hàm hoàn tất chạy trên luồng chính khi tấm đóng; không giữ gì ngoài việc gọi `folder_done`.
    let handler = RcBlock::new(|response: NSModalResponse| folder_done(response));
    panel.beginSheetModalForWindow_completionHandler(&i.controls.window, &handler);
    *i.panel.borrow_mut() = Some(panel);
}

fn folder_done(response: NSModalResponse) {
    let Some(i) = inner() else { return };
    let panel = i.panel.borrow_mut().take();
    if response != NSModalResponseOK {
        return;
    }
    let path = panel
        .and_then(|p| p.URL())
        .and_then(|u| u.path())
        .map(|p| p.to_string());
    if let Some(p) = path {
        i.controls.save_dir.setStringValue(&ns(&p));
    }
}

/// Chọn PNG hoặc JPEG.
pub fn on_format_chosen(jpeg: bool) {
    let Some(i) = inner() else { return };
    let c = &i.controls;
    c.png.setState(if jpeg {
        NSControlStateValueOff
    } else {
        NSControlStateValueOn
    });
    c.jpeg.setState(if jpeg {
        NSControlStateValueOn
    } else {
        NSControlStateValueOff
    });
    let fmt = if jpeg {
        UploadFormat::Jpeg
    } else {
        UploadFormat::Png
    };
    let on = quality_enabled(fmt);
    c.quality_label.setEnabled(on);
    c.quality.setEnabled(on);
    c.quality_value.setEnabled(on);
}

/// Thanh trượt chất lượng đổi: cập nhật số bên cạnh.
pub fn on_quality_changed() {
    let Some(i) = inner() else { return };
    let q = clamp_jpeg_quality(i.controls.quality.doubleValue().round() as i64);
    i.controls.quality_value.setStringValue(&ns(&q.to_string()));
}

// ---------------------------------------------------------------- phím tắt

fn refresh_status(i: &Inner, row: usize) {
    let Some(role) = i.roles.get(row).copied() else {
        return;
    };
    let Some(label) = i.controls.hk_status.get(row) else {
        return;
    };
    if i.capture_row.get() == Some(row) {
        label.setStringValue(&ns(lang::tr(Msg::HotkeyCapturePrompt)));
        label.setTextColor(Some(&NSColor::secondaryLabelColor()));
        return;
    }
    // Trạng thái theo hộp tích hiện tại và phím đang sửa.
    let mut keys = *i.hotkeys.borrow();
    if let Some(b) = i.controls.hk_check.get(row) {
        let mut slot = keys.get(role);
        slot.enabled = checked(b);
        keys.set(role, slot);
    }
    let status = row_status(&keys, role, &i.failures.borrow());
    match status_msg(status) {
        Some(m) => {
            label.setStringValue(&ns(lang::tr(m)));
            let color = if status == RowStatus::Ok {
                NSColor::secondaryLabelColor()
            } else {
                NSColor::systemRedColor()
            };
            label.setTextColor(Some(&color));
        }
        None => label.setStringValue(&ns("")),
    }
}

/// Hộp tích của một dòng phím tắt đổi.
pub fn on_row_toggled(row: usize) {
    let Some(i) = inner() else { return };
    refresh_status(&i, row);
}

/// Nút "Đổi…" hoặc bấm vào ô: bắt đầu bắt phím cho dòng `row`.
pub fn on_change_hotkey(row: usize) {
    let Some(i) = inner() else { return };
    begin_capture(&i, row);
}

pub fn field_clicked(row: usize) {
    on_change_hotkey(row);
}

fn begin_capture(i: &Inner, row: usize) {
    let Some(field) = i.controls.hk_field.get(row) else {
        return;
    };
    if let Some(prev) = i.capture_row.replace(Some(row)) {
        if prev != row {
            if let Some(f) = i.controls.hk_field.get(prev) {
                f.set_capturing(false);
            }
            refresh_status(i, prev);
        }
    }
    field.set_capturing(true);
    i.controls.window.makeFirstResponder(Some(field));
    refresh_status(i, row);
}

fn end_capture(i: &Inner, row: usize) {
    if i.capture_row.get() == Some(row) {
        i.capture_row.set(None);
    }
    if let Some(f) = i.controls.hk_field.get(row) {
        f.set_capturing(false);
    }
    refresh_status(i, row);
}

/// Ô mất tiêu điểm khi đang bắt phím: dừng bắt, giữ phím cũ.
pub fn field_resigned(row: usize) {
    let Some(i) = inner() else { return };
    end_capture(&i, row);
}

/// Kết quả một lần bấm trong ô bắt phím.
pub fn hotkey_step(row: usize, step: CaptureStep) {
    let Some(i) = inner() else { return };
    let Some(role) = i.roles.get(row).copied() else {
        return;
    };
    match step {
        CaptureStep::Wait => {}
        CaptureStep::Cancel => end_capture(&i, row),
        CaptureStep::NeedModifier | CaptureStep::Unsupported => {
            let m = if step == CaptureStep::NeedModifier {
                Msg::MacHotkeyNeedModifier
            } else {
                Msg::MacHotkeyUnsupportedKey
            };
            if let Some(label) = i.controls.hk_status.get(row) {
                label.setStringValue(&ns(lang::tr(m)));
                label.setTextColor(Some(&NSColor::systemRedColor()));
            }
        }
        CaptureStep::Got(hk) => {
            let slot = {
                let mut keys = i.hotkeys.borrow_mut();
                let mut slot = keys.get(role);
                slot.key = hk;
                keys.set(role, slot);
                slot
            };
            // Phím mới: kết quả đăng ký của phím cũ không còn đúng.
            {
                let mut f = i.failures.borrow_mut();
                f.taken.retain(|r| *r != role);
                f.unsupported.retain(|r| *r != role);
            }
            if let Some(field) = i.controls.hk_field.get(row) {
                field.set_text(&hotkey_text(&slot));
            }
            end_capture(&i, row);
        }
    }
}

// ---------------------------------------------------------------- kiểm tra kết nối R2

fn set_test_result(i: &Inner, text: &str, color: Retained<NSColor>) {
    i.controls.test_result.setStringValue(&ns(text));
    i.controls.test_result.setTextColor(Some(&color));
}

/// Nút "Kiểm tra kết nối": đọc biểu mẫu qua `apply_form` (cùng luật như OK), rồi chạy
/// `R2Uploader::test_connection` trên luồng nền.
pub fn on_test() {
    let Some(i) = inner() else { return };
    let form = read_form(&i);
    let cfg = match apply_form(&i.current, &form, lang::current()) {
        Ok(c) => c,
        Err(e) => {
            show_form_error(&i, &e);
            return;
        }
    };
    if !cfg.r2.is_complete() {
        set_test_result(&i, lang::tr(Msg::TestFillAll), NSColor::systemRedColor());
        return;
    }
    if i.test_generation.get() != 0 {
        return;
    }
    let generation = NEXT_GENERATION.fetch_add(1, Ordering::SeqCst);
    let r2 = cfg.r2;
    let spawned = std::thread::Builder::new()
        .name("darkshot-test-connection".into())
        .spawn(move || {
            let (ok, msg) = match R2Uploader::new(&r2).test_connection() {
                Ok(()) => (true, lang::tr(Msg::TestSuccess).to_string()),
                Err(e) => (false, e.message(lang::current())),
            };
            // Kết quả về luồng chính; nếu cửa sổ đã đóng thì `test_done` bỏ qua.
            DispatchQueue::main().exec_async(move || test_done(generation, ok, &msg));
        });
    if spawned.is_err() {
        set_test_result(
            &i,
            lang::tr(Msg::TestThreadFailed),
            NSColor::systemRedColor(),
        );
        return;
    }
    i.test_generation.set(generation);
    i.controls.test_button.setEnabled(false);
    set_test_result(
        &i,
        lang::tr(Msg::TestRunning),
        NSColor::secondaryLabelColor(),
    );
    // Dự phòng: luồng nền không bao giờ báo về thì mở nút lại và báo hết giờ.
    let when = DispatchTime::NOW.time(TEST_FAILSAFE.as_nanos() as i64);
    let _ = DispatchQueue::main().after(when, move || test_timeout(generation));
}

fn test_done(generation: u64, ok: bool, msg: &str) {
    let Some(i) = inner() else { return };
    if i.test_generation.get() != generation {
        return;
    }
    i.test_generation.set(0);
    i.controls.test_button.setEnabled(true);
    let color = if ok {
        NSColor::systemGreenColor()
    } else {
        NSColor::systemRedColor()
    };
    set_test_result(&i, msg, color);
}

fn test_timeout(generation: u64) {
    if inner().is_some_and(|i| i.test_generation.get() == generation) {
        test_done(generation, false, lang::tr(Msg::TestTimeout));
    }
}

/// Tab đang chọn (cho kiểm thử chạy tay).
#[allow(dead_code)]
pub fn selected_tab() -> Option<Tab> {
    let i = inner()?;
    let item = i.controls.tab_view.selectedTabViewItem()?;
    let idx = i.controls.tab_view.indexOfTabViewItem(&item);
    usize::try_from(idx)
        .ok()
        .and_then(|n| Tab::ALL.get(n).copied())
}
