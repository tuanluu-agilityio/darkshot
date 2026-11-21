//! Cửa sổ và điều khiển của Tùy chọn macOS (Task 107): `NSWindow` giao diện luôn tối với
//! `NSTabView` bốn tab (`Tab::ALL`), điều khiển AppKit dựng bằng khung tính từ hằng (không Auto
//! Layout; cỡ cố định, chữ kiểm bằng `options_model::layout`). Không giữ trạng thái: mọi hành
//! động đi qua `DarkshotOptionsTarget` tới `options`.
//!
//! Ô khóa bí mật là `NSSecureTextField`. Không điều khiển nào mang tên hay mô tả chứa giá trị.

use crate::hotkey_field::HotkeyField;
use crate::lang;
use crate::options;
use crate::options_model::{
    expiry_index, form_from_config, hotkey_text, lang_index, layout::*, visible_roles,
};
use darkshot_core::config::{Config, Expiry, UploadFormat, JPEG_QUALITY_MAX, JPEG_QUALITY_MIN};
use darkshot_core::i18n::{self, Lang, Msg, OK_LABEL};
use darkshot_core::settings_form::{quality_enabled, Tab};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceNameDarkAqua, NSBackingStoreType,
    NSButton, NSColor, NSControl, NSControlStateValueOff, NSControlStateValueOn, NSFont,
    NSPopUpButton, NSResponder, NSSecureTextField, NSSlider, NSTabView,
    NSTabViewItem, NSTextField, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{
    NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};

fn ns(s: &str) -> Retained<NSString> {
    NSString::from_str(s)
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

define_class!(
    // SAFETY: NSView cho phép kế thừa; lớp không có ivar, không cài `Drop`. Chỉ dùng trên luồng
    // chính. Gốc toạ độ ở trên trái để bố cục đọc từ trên xuống như Windows.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "DarkshotFlippedView"]
    struct FlippedView;

    unsafe impl NSObjectProtocol for FlippedView {}

    impl FlippedView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }
    }
);

impl FlippedView {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: `initWithFrame:` là bộ khởi tạo chỉ định của NSView, gọi trên đối tượng vừa
        // cấp phát.
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }
}

define_class!(
    // SAFETY: NSObject không có yêu cầu riêng khi kế thừa; lớp không có ivar, không cài `Drop`.
    // Chỉ dùng trên luồng chính (đích hành động của điều khiển, bộ nhận thông báo).
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "DarkshotOptionsTarget"]
    pub struct OptionsTarget;

    unsafe impl NSObjectProtocol for OptionsTarget {}

    impl OptionsTarget {
        #[unsafe(method(okPressed:))]
        fn ok_pressed(&self, _sender: Option<&AnyObject>) {
            options::on_ok();
        }

        #[unsafe(method(cancelPressed:))]
        fn cancel_pressed(&self, _sender: Option<&AnyObject>) {
            options::on_cancel();
        }

        #[unsafe(method(choosePressed:))]
        fn choose_pressed(&self, _sender: Option<&AnyObject>) {
            options::on_choose_folder();
        }

        #[unsafe(method(changeHotkey:))]
        fn change_hotkey(&self, sender: Option<&NSControl>) {
            if let Some(row) = sender.and_then(|s| usize::try_from(s.tag()).ok()) {
                options::on_change_hotkey(row);
            }
        }

        #[unsafe(method(rowToggled:))]
        fn row_toggled(&self, sender: Option<&NSControl>) {
            if let Some(row) = sender.and_then(|s| usize::try_from(s.tag()).ok()) {
                options::on_row_toggled(row);
            }
        }

        #[unsafe(method(formatChosen:))]
        fn format_chosen(&self, sender: Option<&NSControl>) {
            if let Some(tag) = sender.map(|s| s.tag()) {
                options::on_format_chosen(tag == 1);
            }
        }

        #[unsafe(method(qualityChanged:))]
        fn quality_changed(&self, _sender: Option<&AnyObject>) {
            options::on_quality_changed();
        }

        #[unsafe(method(testPressed:))]
        fn test_pressed(&self, _sender: Option<&AnyObject>) {
            options::on_test();
        }

        /// Người dùng đóng cửa sổ bằng nút đỏ: như Hủy.
        #[unsafe(method(windowClosing:))]
        fn window_closing(&self, _note: &NSNotification) {
            options::on_window_closed();
        }
    }
);

impl OptionsTarget {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: `init` của NSObject trên đối tượng vừa cấp phát.
        unsafe { msg_send![super(this), init] }
    }
}

/// Mọi điều khiển cần đọc hay ghi sau khi dựng.
pub struct Controls {
    pub window: Retained<NSWindow>,
    pub tab_view: Retained<NSTabView>,
    pub save_dir: Retained<NSTextField>,
    pub language: Retained<NSPopUpButton>,
    pub notices: Retained<NSButton>,
    pub keep_selection: Retained<NSButton>,
    pub include_cursor: Retained<NSButton>,
    /// Ba mảng dưới đây cùng chỉ số với `options_model::visible_roles()`.
    pub hk_check: Vec<Retained<NSButton>>,
    pub hk_field: Vec<Retained<HotkeyField>>,
    pub hk_change: Vec<Retained<NSButton>>,
    pub hk_status: Vec<Retained<NSTextField>>,
    pub png: Retained<NSButton>,
    pub jpeg: Retained<NSButton>,
    pub quality_label: Retained<NSTextField>,
    pub quality: Retained<NSSlider>,
    pub quality_value: Retained<NSTextField>,
    pub account: Retained<NSTextField>,
    pub key_id: Retained<NSTextField>,
    /// Ô mật khẩu: không hiện ký tự, không hiện lại khóa đã lưu.
    pub secret: Retained<NSSecureTextField>,
    pub bucket: Retained<NSTextField>,
    pub url: Retained<NSTextField>,
    pub expiry: Retained<NSPopUpButton>,
    pub test_button: Retained<NSButton>,
    pub test_result: Retained<NSTextField>,
}

struct Builder<'a> {
    mtm: MainThreadMarker,
    target: &'a AnyObject,
}

impl Builder<'_> {
    fn label(&self, parent: &NSView, text: &str, x: f64, y: f64, w: f64) -> Retained<NSTextField> {
        let l = NSTextField::labelWithString(&ns(text), self.mtm);
        l.setFrame(rect(x, y, w, 17.0));
        parent.addSubview(&l);
        l
    }

    /// Ghi chú nhiều dòng, chữ nhỏ màu phụ.
    fn note(
        &self,
        parent: &NSView,
        text: &str,
        (x, y, w, h): (f64, f64, f64, f64),
    ) -> Retained<NSTextField> {
        let l = NSTextField::wrappingLabelWithString(&ns(text), self.mtm);
        l.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        l.setTextColor(Some(&NSColor::secondaryLabelColor()));
        l.setFrame(rect(x, y, w, h));
        parent.addSubview(&l);
        l
    }

    fn check(
        &self,
        parent: &NSView,
        text: &str,
        (x, y, w): (f64, f64, f64),
        on: bool,
        action: objc2::runtime::Sel,
        tag: isize,
    ) -> Retained<NSButton> {
        // SAFETY: đích là `OptionsTarget` sống cùng cửa sổ (do `options` giữ); `action` là bộ chọn
        // có trên đích (nhận một đối số).
        let b = unsafe {
            NSButton::checkboxWithTitle_target_action(
                &ns(text),
                Some(self.target),
                Some(action),
                self.mtm,
            )
        };
        b.setFrame(rect(x, y, w, 18.0));
        b.setState(if on {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        b.setTag(tag);
        parent.addSubview(&b);
        b
    }

    fn radio(
        &self,
        parent: &NSView,
        text: &str,
        (x, y, w): (f64, f64, f64),
        on: bool,
        tag: isize,
    ) -> Retained<NSButton> {
        // SAFETY: như `check`.
        let b = unsafe {
            NSButton::radioButtonWithTitle_target_action(
                &ns(text),
                Some(self.target),
                Some(sel!(formatChosen:)),
                self.mtm,
            )
        };
        b.setFrame(rect(x, y, w, 18.0));
        b.setState(if on {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        b.setTag(tag);
        parent.addSubview(&b);
        b
    }

    fn button(
        &self,
        text: &str,
        (x, y, w): (f64, f64, f64),
        action: objc2::runtime::Sel,
    ) -> Retained<NSButton> {
        // SAFETY: như `check`.
        let b = unsafe {
            NSButton::buttonWithTitle_target_action(
                &ns(text),
                Some(self.target),
                Some(action),
                self.mtm,
            )
        };
        b.setFrame(rect(x, y, w, 26.0));
        b
    }

    fn field(&self, parent: &NSView, value: &str, (x, y, w): (f64, f64, f64)) -> Retained<NSTextField> {
        let f = NSTextField::textFieldWithString(&ns(value), self.mtm);
        f.setFrame(rect(x, y, w, 22.0));
        parent.addSubview(&f);
        f
    }

    fn popup(
        &self,
        parent: &NSView,
        titles: &[&str],
        selected: usize,
        (x, y, w): (f64, f64, f64),
    ) -> Retained<NSPopUpButton> {
        let p = NSPopUpButton::initWithFrame_pullsDown(
            self.mtm.alloc(),
            rect(x, y, w, 26.0),
            false,
        );
        for t in titles {
            p.addItemWithTitle(&ns(t));
        }
        p.selectItemAtIndex(selected as isize);
        parent.addSubview(&p);
        p
    }
}

/// Cột đặt ô sau cột nhãn.
const FIELD_X: f64 = PAD + LABEL_W + 8.0;
/// Bề rộng ô nhập của tab R2.
const FIELD_W: f64 = CONTENT_W - PAD - FIELD_X;
/// Bề rộng vùng chữ của một hàng đầy đủ.
const FULL_W: f64 = CONTENT_W - 2.0 * PAD;

/// Dựng cửa sổ (chưa hiện) từ cấu hình `cfg`; `failures` quyết định trạng thái ban đầu của các
/// dòng phím tắt (do `options` đặt sau).
pub fn build(
    mtm: MainThreadMarker,
    cfg: &Config,
    target: &OptionsTarget,
    selected_tab: usize,
) -> Controls {
    let form = form_from_config(cfg);
    let target_obj: &AnyObject = target;
    let b = Builder {
        mtm,
        target: target_obj,
    };
    let l = |m: Msg| lang::tr(m);

    let window = {
        // SAFETY: bộ khởi tạo chỉ định của NSWindow trên đối tượng vừa cấp phát; tham số hợp lệ.
        let w: Retained<NSWindow> = unsafe {
            msg_send![
                NSWindow::alloc(mtm),
                initWithContentRect: rect(0.0, 0.0, WINDOW_W, WINDOW_H),
                styleMask: NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
                backing: NSBackingStoreType::Buffered,
                defer: false
            ]
        };
        // SAFETY: Rust giữ `Retained` của cửa sổ; `close` không được tự giải phóng nó lần nữa.
        unsafe { w.setReleasedWhenClosed(false) };
        w.setTitle(&ns(l(Msg::OptionsTitle)));
        // Giao diện luôn tối, không theo cài đặt hệ thống.
        // SAFETY: `NSAppearanceNameDarkAqua` là hằng của AppKit sống suốt tiến trình.
        if let Some(dark) = NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua }) {
            w.setAppearance(Some(&dark));
        }
        w
    };
    let content = FlippedView::new(mtm, rect(0.0, 0.0, WINDOW_W, WINDOW_H));
    window.setContentView(Some(&content));

    let tab_view = NSTabView::initWithFrame(
        mtm.alloc(),
        rect(10.0, 10.0, WINDOW_W - 20.0, WINDOW_H - 62.0),
    );
    // Một khung nội dung cho mỗi tab, theo thứ tự `Tab::ALL`.
    let pane = |tab: Tab| -> Retained<FlippedView> {
        let v = FlippedView::new(mtm, rect(0.0, 0.0, CONTENT_W, WINDOW_H - 110.0));
        let item = NSTabViewItem::new();
        item.setLabel(&ns(l(tab.label())));
        item.setView(Some(&v));
        tab_view.addTabViewItem(&item);
        v
    };

    // ---- Tab Thông thường ----
    let g = pane(Tab::General);
    b.label(&g, l(Msg::LabelSaveDir), PAD, 16.0, FULL_W);
    let save_dir = b.field(&g, &form.save_dir, (PAD, 38.0, FULL_W - CHANGE_BTN_W - 8.0));
    let choose = b.button(l(Msg::BtnChoose), (PAD + FULL_W - CHANGE_BTN_W, 36.0, CHANGE_BTN_W), sel!(choosePressed:));
    g.addSubview(&choose);
    b.label(&g, l(Msg::LabelLanguage), PAD, 84.0, LABEL_W);
    let names: Vec<&str> = Lang::ALL.iter().map(|x| x.native_name()).collect();
    let language = b.popup(&g, &names, lang_index(form.language), (FIELD_X, 80.0, 200.0));
    let notices = b.check(&g, l(Msg::ChkShowSuccessNotices), (PAD, 128.0, FULL_W), form.show_success_notices, sel!(rowToggled:), -1);
    let keep_selection = b.check(&g, l(Msg::ChkKeepSelection), (PAD, 158.0, FULL_W), form.keep_selection, sel!(rowToggled:), -1);
    let include_cursor = b.check(&g, l(Msg::ChkIncludeCursor), (PAD, 188.0, FULL_W), form.include_cursor, sel!(rowToggled:), -1);
    // Hộp tích chung không cần hành động; bỏ đích để không gọi `rowToggled:` với tag -1.
    for c in [&notices, &keep_selection, &include_cursor] {
        // SAFETY: bỏ đích và hành động của nút; không có tham chiếu nào bị giữ.
        unsafe {
            c.setTarget(None);
            c.setAction(None);
        }
    }

    // ---- Tab Phím tắt ----
    let h = pane(Tab::Hotkeys);
    let roles = visible_roles();
    let (mut hk_check, mut hk_field, mut hk_change, mut hk_status) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (i, role) in roles.iter().enumerate() {
        let y = 14.0 + 86.0 * i as f64;
        let slot = form.hotkeys.get(*role);
        let tag = i as isize;
        hk_check.push(b.check(&h, l(role.label()), (PAD, y, CHECK_W), slot.enabled, sel!(rowToggled:), tag));
        let field = HotkeyField::new(mtm, rect(PAD + 20.0, y + 24.0, KEY_FIELD_W, 24.0), i);
        field.set_text(&hotkey_text(&slot));
        h.addSubview(&field);
        hk_field.push(field);
        let change = b.button(l(Msg::BtnChange), (PAD + 20.0 + KEY_FIELD_W + 8.0, y + 23.0, CHANGE_BTN_W), sel!(changeHotkey:));
        change.setTag(tag);
        h.addSubview(&change);
        hk_change.push(change);
        hk_status.push(b.note(&h, "", (PAD + 20.0, y + 52.0, FULL_W - 20.0, 30.0)));
    }
    let note_y = 14.0 + 86.0 * roles.len() as f64 + 4.0;
    b.note(&h, l(Msg::MacHotkeyNote), (PAD, note_y, FULL_W, 54.0));

    // ---- Tab Định dạng ----
    let f = pane(Tab::Format);
    b.label(&f, l(Msg::LabelUploadFormat), PAD, 16.0, FULL_W);
    let is_jpeg = form.upload_format == UploadFormat::Jpeg;
    let png = b.radio(&f, l(UploadFormat::Png.label()), (PAD + 12.0, 42.0, FULL_W - 12.0), !is_jpeg, 0);
    let jpeg = b.radio(&f, l(UploadFormat::Jpeg.label()), (PAD + 12.0, 68.0, FULL_W - 12.0), is_jpeg, 1);
    let quality_label = b.label(&f, l(Msg::LabelJpegQuality), PAD, 114.0, LABEL_W);
    // SAFETY: đích là `OptionsTarget` sống cùng cửa sổ; bộ chọn có trên đích.
    let quality = unsafe {
        NSSlider::sliderWithValue_minValue_maxValue_target_action(
            f64::from(form.jpeg_quality),
            f64::from(JPEG_QUALITY_MIN),
            f64::from(JPEG_QUALITY_MAX),
            Some(target_obj),
            Some(sel!(qualityChanged:)),
            mtm,
        )
    };
    quality.setFrame(rect(FIELD_X, 110.0, 240.0, 26.0));
    quality.setContinuous(true);
    f.addSubview(&quality);
    let quality_value = b.label(&f, &form.jpeg_quality.to_string(), FIELD_X + 250.0, 114.0, 40.0);
    b.note(&f, l(Msg::NoteSavedAlwaysPng), (PAD, 160.0, FULL_W, 20.0));
    quality_label.setEnabled(quality_enabled(form.upload_format));
    quality.setEnabled(quality_enabled(form.upload_format));
    quality_value.setEnabled(quality_enabled(form.upload_format));

    // ---- Tab R2 ----
    let r = pane(Tab::R2);
    let mut y = 16.0;
    let mut row = |text: &str, value: &str| -> Retained<NSTextField> {
        b.label(&r, text, PAD, y + 3.0, LABEL_W);
        let fld = b.field(&r, value, (FIELD_X, y, FIELD_W));
        y += 34.0;
        fld
    };
    // "Account ID", "Access Key ID", "Secret Access Key" là tên riêng, không dịch.
    let account = row("Account ID", &form.account_id);
    let key_id = row("Access Key ID", &form.access_key_id);
    b.label(&r, "Secret Access Key", PAD, y + 3.0, LABEL_W);
    let secret = NSSecureTextField::initWithFrame(mtm.alloc(), rect(FIELD_X, y, FIELD_W, 22.0));
    r.addSubview(&secret);
    y += 24.0;
    let hint = if cfg.r2.secret_access_key.is_empty() {
        Msg::SecretNone
    } else {
        Msg::SecretSaved
    };
    b.note(&r, l(hint), (FIELD_X, y, FIELD_W, 16.0));
    y += 26.0;
    let bucket = {
        b.label(&r, l(Msg::LabelBucket), PAD, y + 3.0, LABEL_W);
        let fld = b.field(&r, &form.bucket, (FIELD_X, y, FIELD_W));
        y += 34.0;
        fld
    };
    let url = {
        b.label(&r, l(Msg::LabelPublicUrl), PAD, y + 3.0, LABEL_W);
        let fld = b.field(&r, &form.public_base_url, (FIELD_X, y, FIELD_W));
        y += 34.0;
        fld
    };
    b.label(&r, l(Msg::LabelExpiry), PAD, y + 5.0, LABEL_W);
    let expiry_titles: Vec<&str> = Expiry::ALL
        .iter()
        .map(|e| i18n::expiry_label(lang::current(), *e))
        .collect();
    let expiry = b.popup(&r, &expiry_titles, expiry_index(form.expiry), (FIELD_X, y + 1.0, 200.0));
    y += 44.0;
    let test_button = b.button(l(Msg::BtnTest), (PAD, y, 150.0), sel!(testPressed:));
    r.addSubview(&test_button);
    let test_result = b.note(&r, "", (PAD + 160.0, y + 2.0, FULL_W - 160.0, 52.0));

    // Chọn tab mở đầu rồi thêm khung tab vào cửa sổ TRƯỚC các nút (thứ tự duyệt phím tắt: ô bắt
    // phím trong tab được hỏi trước nút Hủy khi đang bắt phím).
    tab_view.selectTabViewItemAtIndex(selected_tab.min(Tab::ALL.len() - 1) as isize);
    content.addSubview(&tab_view);

    let by = WINDOW_H - 40.0;
    let ok = b.button(OK_LABEL, (WINDOW_W - 20.0 - 90.0, by, 90.0), sel!(okPressed:));
    ok.setKeyEquivalent(&ns("\r"));
    let cancel = b.button(l(Msg::BtnCancel), (WINDOW_W - 20.0 - 90.0 - 8.0 - 90.0, by, 90.0), sel!(cancelPressed:));
    cancel.setKeyEquivalent(&ns("\u{1b}"));
    content.addSubview(&cancel);
    content.addSubview(&ok);

    Controls {
        window,
        tab_view,
        save_dir,
        language,
        notices,
        keep_selection,
        include_cursor,
        hk_check,
        hk_field,
        hk_change,
        hk_status,
        png,
        jpeg,
        quality_label,
        quality,
        quality_value,
        account,
        key_id,
        secret,
        bucket,
        url,
        expiry,
        test_button,
        test_result,
    }
}
