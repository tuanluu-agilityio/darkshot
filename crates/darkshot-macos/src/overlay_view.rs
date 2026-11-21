//! Cửa sổ và view của lớp phủ chọn vùng (Task 105): mỗi `NSScreen` một `DarkshotOverlayWindow`
//! (panel không viền, không kích hoạt ứng dụng, nhận phím được) chứa một `DarkshotOverlayView`
//! (`isFlipped`, gốc trên trái như core). View chỉ đổi sự kiện thành lời gọi tới `overlay` và vẽ bằng Core Graphics trong không
//! gian pixel của màn hình đó; mọi trạng thái nằm ở `overlay` (mô hình thuần `overlay_input`).

use crate::annotate_draw;
use crate::lang;
use crate::overlay::{self, Frame};
use crate::overlay_input::{
    bar_button_rect, bar_tip, hint_rect, label_rect, size_label, tip_rect, Mods, MAC_BAR,
};
use darkshot_core::actions::Action;
use darkshot_core::geometry::Rect;
use darkshot_core::i18n::Msg;
use darkshot_core::overlay_layout::{scaled, HANDLE_SIZE, ICON};
use darkshot_core::theme::{
    Rgb, ACCENT, OVERLAY_BORDER, OVERLAY_HOVER, OVERLAY_ICON, OVERLAY_LABEL, OVERLAY_TIP, SURFACE,
    TEXT, WHITE,
};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSCursor, NSEvent, NSEventModifierFlags, NSFont,
    NSFontAttributeName, NSForegroundColorAttributeName, NSGraphicsContext, NSPanel, NSResponder,
    NSScreenSaverWindowLevel, NSStringDrawing, NSView, NSWindow, NSWindowAnimationBehavior,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGContext, CGInterpolationQuality};
use objc2_foundation::{
    NSAttributedStringKey, NSDictionary, NSObject, NSObjectProtocol, NSPoint, NSRect, NSString,
};

/// Độ mờ của lớp phủ đen ngoài vùng chọn.
const DIM_ALPHA: CGFloat = 0.45;
/// Cỡ chữ (điểm) của nhãn, gợi ý và viên nhắc.
const FONT_PT: CGFloat = 12.0;

define_class!(
    // SAFETY: NSPanel cho phép kế thừa; lớp không có ivar và không cài `Drop`. Chỉ dùng trên
    // luồng chính (như mọi NSWindow).
    //
    // Panel `NonactivatingPanel`: trở thành key window và nhận `keyDown:` mà KHÔNG cần ứng dụng
    // được kích hoạt (như Spotlight). Trên macOS 14+ `activate()` có thể bị từ chối (kích hoạt
    // hợp tác); khi đó một NSWindow thường không nhận phím tới khi người dùng bấm chuột.
    #[unsafe(super(NSPanel, NSWindow, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "DarkshotOverlayWindow"]
    pub struct OverlayWindow;

    unsafe impl NSObjectProtocol for OverlayWindow {}

    impl OverlayWindow {
        /// Cửa sổ không viền mặc định không nhận phím; lớp phủ phải nhận `Esc` ngay khi hiện.
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key_window(&self) -> bool {
            true
        }

        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main_window(&self) -> bool {
            true
        }
    }
);

define_class!(
    // SAFETY: NSView cho phép kế thừa; ivar là chỉ số màn hình (`usize`, không cần hủy đặc
    // biệt). Chỉ dùng trên luồng chính.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "DarkshotOverlayView"]
    #[ivars = usize]
    pub struct OverlayView;

    unsafe impl NSObjectProtocol for OverlayView {}

    impl OverlayView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        /// Cú nhấn đầu tiên trên màn hình chưa có tiêu điểm vẫn là một lần nhấn chuột.
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            if let Some(frame) = overlay::frame(*self.ivars()) {
                draw(&frame);
            }
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            overlay::mouse_down(*self.ivars(), self.global_point(event));
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            overlay::mouse_dragged(*self.ivars(), self.global_point(event));
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            overlay::mouse_up(*self.ivars(), self.global_point(event));
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            overlay::mouse_moved(*self.ivars(), self.global_point(event));
        }

        /// Chuột phải không làm gì (không chuyển tiếp để tránh menu ngữ cảnh).
        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, _event: &NSEvent) {}

        /// Mọi phím bị nuốt (không chuyển tiếp: tránh tiếng bíp của hệ thống).
        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            let (code, chars, mods) = key_of(event);
            overlay::key_down(code, chars.as_deref(), mods);
        }

        /// `⌘S`/`⌘C` có thể tới đây trước `keyDown:`; chỉ nhận phím mình dùng.
        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> bool {
            let (code, chars, mods) = key_of(event);
            overlay::key_down(code, chars.as_deref(), mods)
        }

        /// `Esc` qua hệ thống phím của AppKit: hủy.
        #[unsafe(method(cancelOperation:))]
        fn cancel_operation(&self, _sender: Option<&AnyObject>) {
            overlay::cancel();
        }

        #[unsafe(method(resetCursorRects))]
        fn reset_cursor_rects(&self) {
            self.addCursorRect_cursor(self.bounds(), &NSCursor::crosshairCursor());
        }
    }
);

impl OverlayView {
    fn new(mtm: MainThreadMarker, frame: NSRect, index: usize) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(index);
        // SAFETY: `initWithFrame:` là bộ khởi tạo chỉ định của NSView, gọi trên đối tượng vừa
        // cấp phát.
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }

    /// Vị trí sự kiện trong toạ độ toàn cục Cocoa (điểm, gốc dưới trái màn hình chính).
    fn global_point(&self, event: &NSEvent) -> (f64, f64) {
        let loc = event.locationInWindow();
        let origin = self
            .window()
            .map_or(NSPoint::new(0.0, 0.0), |w| w.frame().origin);
        (origin.x + loc.x, origin.y + loc.y)
    }
}

/// Mã phím, chữ (bỏ phím bổ trợ) và phím bổ trợ của một sự kiện phím.
fn key_of(event: &NSEvent) -> (u16, Option<String>, Mods) {
    let f = event.modifierFlags();
    let mods = Mods {
        cmd: f.contains(NSEventModifierFlags::Command),
        ctrl: f.contains(NSEventModifierFlags::Control),
        opt: f.contains(NSEventModifierFlags::Option),
        shift: f.contains(NSEventModifierFlags::Shift),
    };
    let chars = event.charactersIgnoringModifiers().map(|s| s.to_string());
    (event.keyCode(), chars, mods)
}

/// Dựng cửa sổ phủ đúng khung Cocoa `frame` của một màn hình (kể cả vùng tai thỏ) và view của nó.
/// Chưa hiện cửa sổ.
pub fn make_window(
    mtm: MainThreadMarker,
    frame: NSRect,
    index: usize,
) -> (Retained<OverlayWindow>, Retained<OverlayView>) {
    let this = OverlayWindow::alloc(mtm).set_ivars(());
    // SAFETY: bộ khởi tạo chỉ định của NSWindow (NSPanel kế thừa) trên đối tượng vừa cấp phát;
    // tham số hợp lệ. `NonactivatingPanel` phải có ngay lúc khởi tạo (đổi sau không có hiệu lực
    // đầy đủ với window server).
    let window: Retained<OverlayWindow> = unsafe {
        msg_send![
            super(this),
            initWithContentRect: frame,
            styleMask: NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
            backing: NSBackingStoreType::Buffered,
            defer: false
        ]
    };
    // SAFETY: Rust giữ `Retained` của cửa sổ; `close` không được tự giải phóng nó lần nữa.
    unsafe { window.setReleasedWhenClosed(false) };
    // Trên thanh menu và mọi cửa sổ thường (mức bảo vệ màn hình, như spec mục 4).
    window.setLevel(NSScreenSaverWindowLevel);
    window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    window.setOpaque(false);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
    window.setHasShadow(false);
    window.setAnimationBehavior(NSWindowAnimationBehavior::None);
    window.setAcceptsMouseMovedEvents(true);
    // Panel nhận key cả khi chỉ bấm chuột hay khi hiện (không chỉ khi cần ô nhập), và không tự ẩn
    // khi ứng dụng không active (ứng dụng có thể không bao giờ active).
    window.setBecomesKeyOnlyIfNeeded(false);
    window.setHidesOnDeactivate(false);
    let bounds = NSRect::new(NSPoint::new(0.0, 0.0), frame.size);
    let view = OverlayView::new(mtm, bounds, index);
    window.setContentView(Some(&view));
    window.makeFirstResponder(Some(&view));
    (window, view)
}

fn cg_rect(r: Rect) -> CGRect {
    CGRect::new(
        CGPoint::new(f64::from(r.x), f64::from(r.y)),
        CGSize::new(f64::from(r.w), f64::from(r.h)),
    )
}

fn rgb(c: Rgb) -> (CGFloat, CGFloat, CGFloat) {
    (
        f64::from(c.0) / 255.0,
        f64::from(c.1) / 255.0,
        f64::from(c.2) / 255.0,
    )
}

fn set_fill(c: Option<&CGContext>, color: Rgb, alpha: CGFloat) {
    let (r, g, b) = rgb(color);
    CGContext::set_rgb_fill_color(c, r, g, b, alpha);
}

fn set_stroke(c: Option<&CGContext>, color: Rgb, alpha: CGFloat) {
    let (r, g, b) = rgb(color);
    CGContext::set_rgb_stroke_color(c, r, g, b, alpha);
}

/// Thuộc tính chữ: phông hệ thống cỡ `px` pixel (vẽ trong không gian pixel), màu `TEXT`.
fn text_attrs(px: CGFloat) -> Retained<NSDictionary<NSAttributedStringKey, AnyObject>> {
    let font = NSFont::systemFontOfSize(px);
    let (r, g, b) = rgb(TEXT);
    let color = NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, 1.0);
    let font: &AnyObject = &font;
    let color: &AnyObject = &color;
    // SAFETY: các khóa là hằng của AppKit sống suốt tiến trình.
    let keys = unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
    NSDictionary::from_slices(&keys, &[font, color])
}

fn text_size(s: &NSString, attrs: &NSDictionary<NSAttributedStringKey, AnyObject>) -> (i32, i32) {
    // SAFETY: `attrs` chỉ chứa phông (`NSFont`) và màu (`NSColor`) đúng kiểu của khóa.
    let size = unsafe { s.sizeWithAttributes(Some(attrs)) };
    (size.width.ceil() as i32, size.height.ceil() as i32)
}

/// Ô nền có viền rồi chữ ở giữa.
fn bubble(
    c: Option<&CGContext>,
    r: Rect,
    fill: Rgb,
    s: &NSString,
    attrs: &NSDictionary<NSAttributedStringKey, AnyObject>,
    size: (i32, i32),
) {
    set_fill(c, fill, 0.92);
    CGContext::fill_rect(c, cg_rect(r));
    set_stroke(c, OVERLAY_BORDER, 1.0);
    CGContext::stroke_rect_with_width(c, cg_rect(r), 1.0);
    let at = CGPoint::new(
        f64::from(r.x) + f64::from(r.w - size.0) / 2.0,
        f64::from(r.y) + f64::from(r.h - size.1) / 2.0,
    );
    // SAFETY: như `text_size`; vẽ vào ngữ cảnh hiện hành của `drawRect:` (view lật, gốc trên trái).
    unsafe { s.drawAtPoint_withAttributes(at, Some(attrs)) };
}

/// Đường gãy trong lưới 24×24 của biểu tượng, gốc `(ox, oy)`, hệ số `k` (pixel mỗi ô lưới).
fn polyline(c: Option<&CGContext>, pts: &[(f64, f64)], ox: f64, oy: f64, k: f64) {
    let Some((first, rest)) = pts.split_first() else {
        return;
    };
    CGContext::begin_path(c);
    CGContext::move_to_point(c, ox + first.0 * k, oy + first.1 * k);
    for p in rest {
        CGContext::add_line_to_point(c, ox + p.0 * k, oy + p.1 * k);
    }
    CGContext::stroke_path(c);
}

/// Biểu tượng của một nút, vẽ bằng nét trong ô `cell`.
fn icon(c: Option<&CGContext>, cell: Rect, dpi: u32, action: Action, color: Rgb) {
    let size = f64::from(scaled(ICON, dpi));
    let k = size / 24.0;
    let ox = f64::from(cell.x) + (f64::from(cell.w) - size) / 2.0;
    let oy = f64::from(cell.y) + (f64::from(cell.h) - size) / 2.0;
    set_stroke(c, color, 1.0);
    CGContext::set_line_width(c, 1.8 * k);
    let p = |pts: &[(f64, f64)]| polyline(c, pts, ox, oy, k);
    match action {
        Action::Upload => {
            // Mũi tên lên trên khay.
            p(&[(12.0, 15.0), (12.0, 3.5)]);
            p(&[(7.0, 8.5), (12.0, 3.5), (17.0, 8.5)]);
            p(&[(4.0, 15.0), (4.0, 20.0), (20.0, 20.0), (20.0, 15.0)]);
        }
        Action::Save => {
            p(&[(12.0, 3.5), (12.0, 15.0)]);
            p(&[(7.0, 10.0), (12.0, 15.0), (17.0, 10.0)]);
            p(&[(4.0, 15.0), (4.0, 20.0), (20.0, 20.0), (20.0, 15.0)]);
        }
        Action::Copy => {
            p(&[
                (9.0, 8.0),
                (9.0, 4.0),
                (20.0, 4.0),
                (20.0, 15.0),
                (16.0, 15.0),
            ]);
            p(&[
                (4.0, 9.0),
                (15.0, 9.0),
                (15.0, 20.0),
                (4.0, 20.0),
                (4.0, 9.0),
            ]);
        }
        _ => {
            p(&[(6.0, 6.0), (18.0, 18.0)]);
            p(&[(18.0, 6.0), (6.0, 18.0)]);
        }
    }
}

/// Vẽ một màn hình của lớp phủ. Gọi trong `drawRect:` (đã có ngữ cảnh đồ họa hiện hành).
fn draw(f: &Frame) {
    let Some(ns) = NSGraphicsContext::currentContext() else {
        return;
    };
    let cg = ns.CGContext();
    let c = Some(&*cg);
    let (w, h, dpi) = (f.w, f.h, f.dpi);
    let full = Rect { x: 0, y: 0, w, h };
    CGContext::save_g_state(c);
    // Từ đây đơn vị là pixel của ảnh màn hình (khớp `Selection` và bố cục của core).
    CGContext::scale_ctm(c, 1.0 / f.scale, 1.0 / f.scale);

    // Ảnh: view lật nên phải lật ngược lại khi vẽ `CGImage` để ảnh không bị lộn.
    CGContext::save_g_state(c);
    CGContext::translate_ctm(c, 0.0, f64::from(h));
    CGContext::scale_ctm(c, 1.0, -1.0);
    CGContext::set_interpolation_quality(c, CGInterpolationQuality::None);
    CGContext::draw_image(c, cg_rect(full), Some(&f.image));
    CGContext::restore_g_state(c);

    // Lớp mờ, khoét vùng chọn (chẵn-lẻ).
    set_fill(c, Rgb(0, 0, 0), DIM_ALPHA);
    CGContext::begin_path(c);
    CGContext::add_rect(c, cg_rect(full));
    if let Some(r) = f.state.rect {
        CGContext::add_rect(c, cg_rect(r));
    }
    CGContext::eo_fill_path(c);

    // Hình đánh dấu: chỉ trong vùng chọn, trên ảnh đã khoét mờ; hình sau nằm trước hình trước.
    if let (Some(r), false) = (f.state.rect, f.shapes.is_empty()) {
        annotate_draw::draw_shapes(&cg, &f.shapes, 0, 0, r);
    }

    let attrs = text_attrs(FONT_PT * f.scale);
    if let Some(r) = f.state.rect {
        // Viền 1 px (96 DPI) bên ngoài vùng chọn.
        let lw = f64::from(scaled(1, dpi));
        let outer = CGRect::new(
            CGPoint::new(f64::from(r.x) - lw / 2.0, f64::from(r.y) - lw / 2.0),
            CGSize::new(f64::from(r.w) + lw, f64::from(r.h) + lw),
        );
        set_stroke(c, ACCENT, 1.0);
        CGContext::stroke_rect_with_width(c, outer, lw);
        // Tám tay nắm.
        let hs = scaled(HANDLE_SIZE, dpi);
        let (xs, ys) = (
            [r.x, r.x + r.w / 2, r.right()],
            [r.y, r.y + r.h / 2, r.bottom()],
        );
        for (i, &y) in ys.iter().enumerate() {
            for (j, &x) in xs.iter().enumerate() {
                if i == 1 && j == 1 {
                    continue;
                }
                let hr = Rect {
                    x: x - hs / 2,
                    y: y - hs / 2,
                    w: hs,
                    h: hs,
                };
                set_fill(c, WHITE, 1.0);
                CGContext::fill_rect(c, cg_rect(hr));
                set_stroke(c, ACCENT, 1.0);
                CGContext::stroke_rect_with_width(c, cg_rect(hr), lw);
            }
        }
        // Nhãn kích thước pixel thật.
        let text = NSString::from_str(&size_label(r));
        let size = text_size(&text, &attrs);
        let lr = label_rect(r, size, dpi, full);
        bubble(c, lr, OVERLAY_LABEL, &text, &attrs, size);
    }

    if let Some(bar) = f.state.bar {
        set_fill(c, SURFACE, 1.0);
        CGContext::fill_rect(c, cg_rect(bar));
        for (i, a) in MAC_BAR.iter().enumerate() {
            let cell = bar_button_rect(bar, dpi, i);
            let hot = f.state.pressed == Some(*a) || f.state.hover == Some(*a);
            if hot {
                set_fill(c, OVERLAY_HOVER, 1.0);
                CGContext::fill_rect(c, cg_rect(cell));
            }
            icon(c, cell, dpi, *a, if hot { ACCENT } else { OVERLAY_ICON });
        }
        set_stroke(c, OVERLAY_BORDER, 1.0);
        CGContext::stroke_rect_with_width(c, cg_rect(bar), f64::from(scaled(1, dpi)));
        if let Some(a) = f.state.hover {
            let text = NSString::from_str(lang::tr(bar_tip(a)));
            let size = text_size(&text, &attrs);
            bubble(
                c,
                tip_rect(bar, size, dpi, full),
                OVERLAY_TIP,
                &text,
                &attrs,
                size,
            );
        }
    }

    if f.state.hint {
        let text = NSString::from_str(lang::tr(Msg::OverlayHintDrag));
        let size = text_size(&text, &attrs);
        bubble(
            c,
            hint_rect(full, size, dpi),
            OVERLAY_TIP,
            &text,
            &attrs,
            size,
        );
    }
    CGContext::restore_g_state(c);
}
