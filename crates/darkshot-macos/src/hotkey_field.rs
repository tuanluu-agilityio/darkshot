//! Ô bắt phím của cửa sổ Tùy chọn (Task 107): một `NSView` nhận tiêu điểm và đọc `keyDown:` /
//! `keyUp:` thô khi đang bắt phím. Mã phím macOS đi qua `options_model::capture_step` (đổi bằng
//! `keymap::mac_to_vk` rồi máy trạng thái `CaptureState` của core); ô chỉ vẽ chữ và báo kết quả
//! cho `options` (`hotkey_step`, `field_clicked`, `field_resigned`).
//!
//! Chữ gợi ý ("Nhấn tổ hợp phím mới…", "cần kèm Control…") hiện ở dòng trạng thái dưới ô, không
//! trong ô: ô hẹp nên chữ dài sẽ bị cắt.

use crate::options;
use crate::options_model::{capture_step, CaptureStep, KeyInput};
use darkshot_core::hotkey_keys::CaptureState;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSColor, NSEvent, NSEventModifierFlags, NSFont, NSFontAttributeName,
    NSForegroundColorAttributeName, NSRectFill, NSResponder, NSStringDrawing, NSView,
};
use objc2_foundation::{
    NSAttributedStringKey, NSDictionary, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize,
    NSString,
};
use std::cell::{Cell, RefCell};

/// Dữ liệu riêng của một ô.
pub struct FieldIvars {
    /// Chỉ số dòng phím tắt (theo `options_model::visible_roles`).
    row: usize,
    /// Tên tổ hợp đang hiện.
    text: RefCell<String>,
    capturing: Cell<bool>,
    state: RefCell<CaptureState>,
}

define_class!(
    // SAFETY: NSView cho phép kế thừa; ivar chỉ gồm kiểu Rust thuần (không cần hủy đặc biệt).
    // Chỉ dùng trên luồng chính.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "DarkshotHotkeyField"]
    #[ivars = FieldIvars]
    pub struct HotkeyField;

    unsafe impl NSObjectProtocol for HotkeyField {}

    impl HotkeyField {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        /// Phải nhận được tiêu điểm thì mới nhận `keyDown:`.
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        /// Bấm vào ô cũng bắt đầu bắt phím (như nút "Đổi…").
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, _event: &NSEvent) {
            options::field_clicked(self.ivars().row);
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            self.draw();
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            if !self.ivars().capturing.get() {
                // Không bắt phím: phím đi tiếp như thường (Tab chuyển ô).
                // SAFETY: gọi `keyDown:` của lớp cha với đúng sự kiện nhận được.
                let _: () = unsafe { msg_send![super(self), keyDown: event] };
                return;
            }
            self.feed(event, true);
        }

        #[unsafe(method(keyUp:))]
        fn key_up(&self, event: &NSEvent) {
            if self.ivars().capturing.get() {
                self.feed(event, false);
            } else {
                // SAFETY: như `keyDown:`.
                let _: () = unsafe { msg_send![super(self), keyUp: event] };
            }
        }

        /// Khi đang bắt phím, nuốt cả phím tắt kiểu Command và phím `Esc` (nút Hủy có phím `Esc`)
        /// để chúng thành tổ hợp mới thay vì chạy nút.
        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> bool {
            let capturing = self.ivars().capturing.get();
            if capturing {
                self.feed(event, true);
            }
            capturing
        }

        #[unsafe(method(resignFirstResponder))]
        fn resign_first_responder(&self) -> bool {
            // SAFETY: gọi `resignFirstResponder` của lớp cha.
            let ok: bool = unsafe { msg_send![super(self), resignFirstResponder] };
            if ok && self.ivars().capturing.get() {
                options::field_resigned(self.ivars().row);
            }
            ok
        }
    }
);

impl HotkeyField {
    pub fn new(mtm: MainThreadMarker, frame: NSRect, row: usize) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(FieldIvars {
            row,
            text: RefCell::new(String::new()),
            capturing: Cell::new(false),
            state: RefCell::new(CaptureState::new()),
        });
        // SAFETY: `initWithFrame:` là bộ khởi tạo chỉ định của NSView, gọi trên đối tượng vừa
        // cấp phát.
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }

    /// Đổi chữ hiện trong ô.
    pub fn set_text(&self, text: &str) {
        *self.ivars().text.borrow_mut() = text.to_string();
        self.setNeedsDisplay(true);
    }

    /// Vào hoặc ra chế độ bắt phím (đặt lại máy trạng thái).
    pub fn set_capturing(&self, on: bool) {
        self.ivars().capturing.set(on);
        *self.ivars().state.borrow_mut() = CaptureState::new();
        self.setNeedsDisplay(true);
    }

    fn feed(&self, event: &NSEvent, down: bool) {
        let f = event.modifierFlags();
        let input = KeyInput {
            code: event.keyCode(),
            down,
            ctrl: f.contains(NSEventModifierFlags::Control),
            alt: f.contains(NSEventModifierFlags::Option),
            shift: f.contains(NSEventModifierFlags::Shift),
            cmd: f.contains(NSEventModifierFlags::Command),
        };
        // Không giữ mượn `state` khi báo cho `options` (nó có thể gọi lại ô này).
        let step: CaptureStep = capture_step(&mut self.ivars().state.borrow_mut(), input);
        if step != CaptureStep::Wait {
            options::hotkey_step(self.ivars().row, step);
        }
    }

    fn draw(&self) {
        let b = self.bounds();
        let capturing = self.ivars().capturing.get();
        let border = if capturing {
            NSColor::controlAccentColor()
        } else {
            NSColor::separatorColor()
        };
        border.setFill();
        NSRectFill(b);
        let inner = NSRect::new(
            NSPoint::new(b.origin.x + 1.0, b.origin.y + 1.0),
            NSSize::new(
                (b.size.width - 2.0).max(0.0),
                (b.size.height - 2.0).max(0.0),
            ),
        );
        NSColor::textBackgroundColor().setFill();
        NSRectFill(inner);
        let text = if capturing {
            "…".to_string()
        } else {
            self.ivars().text.borrow().clone()
        };
        let font = NSFont::systemFontOfSize(13.0);
        let color = NSColor::labelColor();
        let (font_obj, color_obj): (&AnyObject, &AnyObject) = (&font, &color);
        // SAFETY: các khóa là hằng của AppKit sống suốt tiến trình.
        let keys: [&NSAttributedStringKey; 2] =
            unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
        let attrs: Retained<NSDictionary<NSAttributedStringKey, AnyObject>> =
            NSDictionary::from_slices(&keys, &[font_obj, color_obj]);
        let s = NSString::from_str(&text);
        // SAFETY: `attrs` chỉ chứa phông (`NSFont`) và màu (`NSColor`) đúng kiểu của khóa.
        let size = unsafe { s.sizeWithAttributes(Some(&attrs)) };
        let at = NSPoint::new(
            b.origin.x + 8.0,
            b.origin.y + ((b.size.height - size.height) / 2.0).max(0.0),
        );
        // SAFETY: như trên; vẽ vào ngữ cảnh hiện hành của `drawRect:` (view lật).
        unsafe { s.drawAtPoint_withAttributes(at, Some(&attrs)) };
    }
}
