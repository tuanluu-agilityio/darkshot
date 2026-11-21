//! Cửa sổ và view của lớp phủ chọn vùng (Task 105): mỗi `NSScreen` một `DarkshotOverlayWindow`
//! (panel không viền, không kích hoạt ứng dụng, nhận phím được) chứa một `DarkshotOverlayView`
//! (`isFlipped`, gốc trên trái như core). View chỉ đổi sự kiện thành lời gọi tới `overlay` và vẽ bằng Core Graphics trong không
//! gian pixel của màn hình đó; mọi trạng thái nằm ở `overlay` (mô hình thuần `overlay_input`).

use crate::annotate_draw;
use crate::annotate_style::{
    chip_focus, layout_ex as style_layout_ex, opacity_percent, StyleCell, StyleKind, StyleLayout,
    StyleTarget,
};
use crate::annotate_ui::{
    flyout_rect_for, flyout_tip_rect, palette_cell_rect, side_tip_rect, Annot, Flyout, Layer,
    Layout, TextCmd,
};
use crate::coords::px_to_pt;
use crate::lang;
use crate::overlay::{self, Frame};
use crate::overlay_input::{
    bar_button_rect, bar_layout, bar_tip, hint_rect, label_rect, size_label, tip_rect, Mods,
};
use darkshot_core::actions::Action;
use darkshot_core::annotate::{Align, HighlightMode, SizeLevel, Tool, PALETTE};
use darkshot_core::color_picker::size_dot;
use darkshot_core::edit::{frame, handles};
use darkshot_core::geometry::Rect;
use darkshot_core::i18n::Msg;
use darkshot_core::overlay_layout::{
    inflate, scaled, shape_cell_rect, shape_tip, side_item_rect, side_layout_for, size_cell_rect,
    size_tip, BarItem, SideItem, GAP, HANDLE_SIZE, ICON, MARK, SHAPE_TOOLS, SIDE_ITEMS, SWATCH,
    TEXT_DASH, TEXT_DASH_GAP,
};
use darkshot_core::theme::{
    Rgb, ACCENT, OVERLAY_BORDER, OVERLAY_HOVER, OVERLAY_ICON, OVERLAY_LABEL, OVERLAY_TIP, SURFACE,
    TEXT, TEXT_DISABLED, WHITE,
};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSCursor, NSEvent, NSEventModifierFlags, NSFont,
    NSFontAttributeName, NSForegroundColorAttributeName, NSGraphicsContext, NSPanel, NSResponder,
    NSScreenSaverWindowLevel, NSStringDrawing, NSTextInputClient, NSView, NSWindow,
    NSWindowAnimationBehavior, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGContext, CGInterpolationQuality, CGLineCap, CGLineJoin};
use objc2_foundation::{
    NSArray, NSAttributedString, NSAttributedStringKey, NSDictionary, NSObject, NSObjectProtocol,
    NSPoint, NSRange, NSRect, NSSize, NSString,
};

/// Độ mờ của lớp phủ đen ngoài vùng chọn.
const DIM_ALPHA: CGFloat = 0.45;
/// Cỡ chữ (điểm) của nhãn, gợi ý và viên nhắc.
const FONT_PT: CGFloat = 12.0;
/// `NSNotFound` (= `NSIntegerMax`) cho `NSRange` của bộ gõ.
const NOT_FOUND: usize = isize::MAX as usize;

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
            overlay::with_frame(*self.ivars(), draw);
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            overlay::mouse_down(*self.ivars(), self.global_point(event), shift_of(event));
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            overlay::mouse_dragged(*self.ivars(), self.global_point(event), shift_of(event));
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            overlay::mouse_up(*self.ivars(), self.global_point(event), shift_of(event));
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            overlay::mouse_moved(*self.ivars(), self.global_point(event));
        }

        /// Shift nhấn/nhả: nét hay hình đang kéo cập nhật ràng buộc (45°, vuông) ngay.
        #[unsafe(method(flagsChanged:))]
        fn flags_changed(&self, event: &NSEvent) {
            overlay::flags_changed(shift_of(event));
        }

        /// Chuột phải không làm gì (không chuyển tiếp để tránh menu ngữ cảnh).
        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, _event: &NSEvent) {}

        /// Mọi phím bị nuốt (không chuyển tiếp: tránh tiếng bíp của hệ thống). Khi đang gõ chữ,
        /// phím không phải ⌘ đi qua bộ gõ (`interpretKeyEvents:`) để tiếng Việt có dấu, dead key và
        /// emoji hoạt động; kết quả về qua `insertText:` / `setMarkedText:` / `doCommandBySelector:`.
        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            let (code, chars, mods) = key_of(event);
            let r = overlay::key_down(code, chars.as_deref(), mods, event.isARepeat());
            if r.ime && !r.done {
                self.interpretKeyEvents(&NSArray::from_slice(&[event]));
            }
        }

        /// Phím có ⌘ tới đây trước `keyDown:`: xử lý ở đây và NUỐT hẳn (trả `true`), nếu không
        /// AppKit chuyển tiếp sang `keyDown:` và ⌘Z sẽ chạy hai lần.
        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> bool {
            let (code, chars, mods) = key_of(event);
            if mods.cmd {
                overlay::key_down(code, chars.as_deref(), mods, event.isARepeat());
            }
            mods.cmd
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

    // Nhập chữ qua bộ gõ của hệ thống: `TextEdit` của core vẫn là nguồn dữ liệu duy nhất; chuỗi đang
    // soạn (marked) nằm ngay trước con trỏ trong `TextEdit` và được thay mỗi lần bộ gõ cập nhật.
    unsafe impl NSTextInputClient for OverlayView {
        #[unsafe(method(insertText:replacementRange:))]
        fn insert_text(&self, string: &AnyObject, _range: NSRange) {
            overlay::text_insert(&text_of(string));
        }

        #[unsafe(method(doCommandBySelector:))]
        fn do_command(&self, selector: Sel) {
            if let Some(c) = command_for(selector) {
                overlay::text_command(c);
            }
        }

        #[unsafe(method(setMarkedText:selectedRange:replacementRange:))]
        fn set_marked_text(&self, string: &AnyObject, _selected: NSRange, _range: NSRange) {
            overlay::text_marked(&text_of(string));
        }

        #[unsafe(method(unmarkText))]
        fn unmark_text(&self) {
            overlay::text_unmark();
        }

        #[unsafe(method(selectedRange))]
        fn selected_range(&self) -> NSRange {
            match overlay::text_state() {
                Some((_, caret)) => NSRange::new(caret, 0),
                None => NSRange::new(NOT_FOUND, 0),
            }
        }

        #[unsafe(method(markedRange))]
        fn marked_range(&self) -> NSRange {
            match overlay::text_state() {
                Some((marked, caret)) if marked > 0 => {
                    NSRange::new(caret.saturating_sub(marked), marked)
                }
                _ => NSRange::new(NOT_FOUND, 0),
            }
        }

        #[unsafe(method(hasMarkedText))]
        fn has_marked_text(&self) -> bool {
            overlay::text_state().is_some_and(|(marked, _)| marked > 0)
        }

        #[unsafe(method_id(attributedSubstringForProposedRange:actualRange:))]
        fn attributed_substring(
            &self,
            _range: NSRange,
            _actual: *mut NSRange,
        ) -> Option<Retained<NSAttributedString>> {
            None
        }

        #[unsafe(method_id(validAttributesForMarkedText))]
        fn valid_attributes(&self) -> Retained<NSArray<NSAttributedStringKey>> {
            NSArray::new()
        }

        /// Vị trí con trỏ (toạ độ màn hình) cho cửa sổ gợi ý của bộ gõ.
        #[unsafe(method(firstRectForCharacterRange:actualRange:))]
        fn first_rect(&self, _range: NSRange, _actual: *mut NSRange) -> NSRect {
            self.caret_screen_rect()
        }

        #[unsafe(method(characterIndexForPoint:))]
        fn character_index(&self, _point: NSPoint) -> usize {
            0
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

    /// Con trỏ của ô chữ đang gõ trong toạ độ màn hình (điểm), cho cửa sổ gợi ý của bộ gõ.
    fn caret_screen_rect(&self) -> NSRect {
        let i = *self.ivars();
        let (Some(r), Some(scale)) = (overlay::text_caret(i), overlay::scale_of(i)) else {
            return NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0));
        };
        let local = NSRect::new(
            NSPoint::new(px_to_pt(r.x, scale), px_to_pt(r.y, scale)),
            NSSize::new(px_to_pt(r.w.max(1), scale), px_to_pt(r.h.max(1), scale)),
        );
        let in_window = self.convertRect_toView(local, None);
        match self.window() {
            Some(w) => w.convertRectToScreen(in_window),
            None => in_window,
        }
    }
}

/// Shift có đang giữ trong sự kiện (chuột hoặc `flagsChanged:`; không đọc chữ của phím).
fn shift_of(event: &NSEvent) -> bool {
    event.modifierFlags().contains(NSEventModifierFlags::Shift)
}

/// Chuỗi của đối số `insertText:`/`setMarkedText:` (`NSString` hoặc `NSAttributedString`).
fn text_of(obj: &AnyObject) -> String {
    if let Some(s) = obj.downcast_ref::<NSString>() {
        s.to_string()
    } else if let Some(a) = obj.downcast_ref::<NSAttributedString>() {
        a.string().to_string()
    } else {
        String::new()
    }
}

/// Lệnh soạn chữ của một `doCommandBySelector:`; lệnh khác (Tab, Esc...) bỏ qua.
fn command_for(selector: Sel) -> Option<TextCmd> {
    let table: [(Sel, TextCmd); 17] = [
        (sel!(deleteBackward:), TextCmd::Backspace),
        (
            sel!(deleteBackwardByDecomposingPreviousCharacter:),
            TextCmd::Backspace,
        ),
        (sel!(deleteForward:), TextCmd::Delete),
        (sel!(moveLeft:), TextCmd::Left),
        (sel!(moveBackward:), TextCmd::Left),
        (sel!(moveRight:), TextCmd::Right),
        (sel!(moveForward:), TextCmd::Right),
        (sel!(moveUp:), TextCmd::Up),
        (sel!(moveDown:), TextCmd::Down),
        (sel!(moveToBeginningOfLine:), TextCmd::Home),
        (sel!(moveToLeftEndOfLine:), TextCmd::Home),
        (sel!(moveToBeginningOfParagraph:), TextCmd::Home),
        (sel!(moveToEndOfLine:), TextCmd::End),
        (sel!(moveToRightEndOfLine:), TextCmd::End),
        (sel!(moveToEndOfParagraph:), TextCmd::End),
        (sel!(insertNewline:), TextCmd::Newline),
        (sel!(insertLineBreak:), TextCmd::Newline),
    ];
    table.iter().find(|(s, _)| *s == selector).map(|(_, c)| *c)
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

/// Một nét của biểu tượng trong lưới 24×24: đường gãy, khép kín hay không.
#[derive(Clone, Debug, PartialEq)]
struct GlyphStroke {
    pts: Vec<(f64, f64)>,
    closed: bool,
}

fn line(pts: &[(f64, f64)]) -> GlyphStroke {
    GlyphStroke {
        pts: pts.to_vec(),
        closed: false,
    }
}

fn poly(pts: &[(f64, f64)]) -> GlyphStroke {
    GlyphStroke {
        pts: pts.to_vec(),
        closed: true,
    }
}

/// Điểm của cung tâm (`cx`, `cy`) bán kính `r`, từ góc `start` quét `sweep` độ; quy ước như GDI+
/// của bản Windows (góc tăng theo chiều kim đồng hồ trên màn hình có Y hướng xuống).
fn arc_pts(cx: f64, cy: f64, r: f64, start: f64, sweep: f64) -> Vec<(f64, f64)> {
    let n = ((sweep.abs() / 15.0).ceil() as usize).max(2);
    (0..=n)
        .map(|i| {
            let a = (start + sweep * i as f64 / n as f64).to_radians();
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

/// Elip khép kín tâm (`cx`, `cy`), nửa trục `rx`, `ry`.
fn oval(cx: f64, cy: f64, rx: f64, ry: f64) -> GlyphStroke {
    let mut pts = arc_pts(cx, cy, 1.0, 0.0, 360.0);
    pts.pop();
    for p in &mut pts {
        p.0 = cx + (p.0 - cx) * rx;
        p.1 = cy + (p.1 - cy) * ry;
    }
    GlyphStroke { pts, closed: true }
}

/// Biểu tượng cần vẽ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Glyph {
    Tool(Tool),
    Undo,
    Redo,
    Act(Action),
}

/// Các nét của biểu tượng `g` trong lưới 24×24.
fn glyph_strokes(g: Glyph) -> Vec<GlyphStroke> {
    match g {
        Glyph::Act(Action::Upload) => vec![
            line(&[(12.0, 15.0), (12.0, 3.5)]),
            line(&[(7.0, 8.5), (12.0, 3.5), (17.0, 8.5)]),
            line(&[(4.0, 15.0), (4.0, 20.0), (20.0, 20.0), (20.0, 15.0)]),
        ],
        Glyph::Act(Action::Save) => vec![
            line(&[(12.0, 3.5), (12.0, 15.0)]),
            line(&[(7.0, 10.0), (12.0, 15.0), (17.0, 10.0)]),
            line(&[(4.0, 15.0), (4.0, 20.0), (20.0, 20.0), (20.0, 15.0)]),
        ],
        Glyph::Act(Action::Copy) => vec![
            line(&[
                (9.0, 8.0),
                (9.0, 4.0),
                (20.0, 4.0),
                (20.0, 15.0),
                (16.0, 15.0),
            ]),
            line(&[
                (4.0, 9.0),
                (15.0, 9.0),
                (15.0, 20.0),
                (4.0, 20.0),
                (4.0, 9.0),
            ]),
        ],
        Glyph::Act(_) => vec![
            line(&[(6.0, 6.0), (18.0, 18.0)]),
            line(&[(18.0, 6.0), (6.0, 18.0)]),
        ],
        Glyph::Undo => {
            // Mũi tên quay lại: đầu nhọn bên trái, thân vòng nửa tròn bên phải.
            let mut body = vec![(4.0, 9.0), (14.5, 9.0)];
            body.extend(arc_pts(14.5, 14.5, 5.5, 270.0, 180.0));
            body.push((9.0, 20.0));
            vec![line(&[(8.0, 5.0), (4.0, 9.0), (8.0, 13.0)]), line(&body)]
        }
        Glyph::Redo => {
            let mut body = vec![(20.0, 9.0), (9.5, 9.0)];
            body.extend(arc_pts(9.5, 14.5, 5.5, 270.0, -180.0));
            body.push((15.0, 20.0));
            vec![line(&[(16.0, 5.0), (20.0, 9.0), (16.0, 13.0)]), line(&body)]
        }
        Glyph::Tool(Tool::Select) => vec![poly(&[
            (6.0, 3.5),
            (6.0, 18.5),
            (10.0, 14.5),
            (12.8, 20.5),
            (15.2, 19.4),
            (12.4, 13.5),
            (18.0, 13.5),
        ])],
        Glyph::Tool(Tool::Pen) => vec![
            poly(&[
                (4.0, 20.0),
                (5.0, 15.5),
                (15.5, 5.0),
                (19.0, 8.5),
                (8.5, 19.0),
            ]),
            line(&[(13.0, 7.5), (16.5, 11.0)]),
        ],
        Glyph::Tool(Tool::Line) => vec![line(&[(5.0, 19.0), (19.0, 5.0)])],
        Glyph::Tool(Tool::Arrow) => vec![
            line(&[(5.0, 19.0), (18.5, 5.5)]),
            line(&[(10.0, 5.0), (19.0, 5.0), (19.0, 14.0)]),
        ],
        Glyph::Tool(Tool::Rect) => {
            vec![poly(&[(4.0, 6.0), (20.0, 6.0), (20.0, 18.0), (4.0, 18.0)])]
        }
        Glyph::Tool(Tool::Ellipse) => vec![oval(12.0, 12.0, 8.5, 6.0)],
        Glyph::Tool(Tool::Marker) => vec![
            poly(&[(9.0, 13.0), (15.0, 7.0), (19.0, 11.0), (13.0, 17.0)]),
            poly(&[(9.0, 13.0), (6.0, 18.0), (8.0, 20.0), (13.0, 17.0)]),
            line(&[(12.0, 21.0), (20.0, 21.0)]),
        ],
        Glyph::Tool(Tool::Blur) => vec![
            poly(&[(4.0, 6.0), (20.0, 6.0), (20.0, 18.0), (4.0, 18.0)]),
            line(&[(4.0, 14.0), (12.0, 6.0)]),
            line(&[(8.0, 18.0), (20.0, 6.0)]),
            line(&[(16.0, 18.0), (20.0, 14.0)]),
        ],
        Glyph::Tool(Tool::Text) => vec![
            line(&[(5.0, 7.0), (5.0, 5.0), (19.0, 5.0), (19.0, 7.0)]),
            line(&[(12.0, 5.0), (12.0, 19.0)]),
            line(&[(9.0, 19.0), (15.0, 19.0)]),
        ],
        Glyph::Tool(Tool::Step) => vec![
            oval(12.0, 12.0, 8.5, 8.5),
            line(&[(10.0, 9.5), (12.5, 7.5), (12.5, 16.5)]),
            line(&[(10.0, 16.5), (15.0, 16.5)]),
        ],
    }
}

/// Vẽ biểu tượng `g` bằng nét trong ô `cell`.
fn glyph(c: Option<&CGContext>, cell: Rect, dpi: u32, g: Glyph, color: Rgb) {
    let size = f64::from(scaled(ICON, dpi));
    let k = size / 24.0;
    let ox = f64::from(cell.x) + (f64::from(cell.w) - size) / 2.0;
    let oy = f64::from(cell.y) + (f64::from(cell.h) - size) / 2.0;
    set_stroke(c, color, 1.0);
    CGContext::set_line_width(c, 1.8 * k);
    CGContext::set_line_cap(c, CGLineCap::Round);
    CGContext::set_line_join(c, CGLineJoin::Round);
    for st in glyph_strokes(g) {
        let Some((first, rest)) = st.pts.split_first() else {
            continue;
        };
        CGContext::begin_path(c);
        CGContext::move_to_point(c, ox + first.0 * k, oy + first.1 * k);
        for p in rest {
            CGContext::add_line_to_point(c, ox + p.0 * k, oy + p.1 * k);
        }
        if st.closed {
            CGContext::close_path(c);
        }
        CGContext::stroke_path(c);
    }
}

/// Chấm tô của nút Cỡ/ô mức cỡ, đường kính `diam` (pixel), tâm giữa ô `cell`.
fn dot(c: Option<&CGContext>, cell: Rect, diam: f64, color: Rgb) {
    set_fill(c, color, 1.0);
    let (cx, cy) = (
        f64::from(cell.x) + f64::from(cell.w) / 2.0,
        f64::from(cell.y) + f64::from(cell.h) / 2.0,
    );
    CGContext::fill_ellipse_in_rect(
        c,
        CGRect::new(
            CGPoint::new(cx - diam / 2.0, cy - diam / 2.0),
            CGSize::new(diam, diam),
        ),
    );
}

/// Tam giác đặc nhỏ ở góc dưới phải ô của nút Hình (nút có bảng bật ra).
fn corner_triangle(c: Option<&CGContext>, cell: Rect, dpi: u32, color: Rgb) {
    let k = f64::from(scaled(ICON, dpi)) / 24.0;
    let inset = f64::from(scaled(4, dpi));
    let (x1, y1) = (
        f64::from(cell.right()) - inset,
        f64::from(cell.bottom()) - inset,
    );
    let t = 5.0 * k;
    set_fill(c, color, 1.0);
    CGContext::begin_path(c);
    CGContext::move_to_point(c, x1, y1 - t);
    CGContext::add_line_to_point(c, x1, y1);
    CGContext::add_line_to_point(c, x1 - t, y1);
    CGContext::close_path(c);
    CGContext::fill_path(c);
}

/// Viền `t` pixel bên trong ô `r`.
fn inner_frame(c: Option<&CGContext>, r: Rect, t: i32, color: Rgb) {
    set_fill(c, color, 1.0);
    for e in [
        Rect { h: t, ..r },
        Rect {
            y: r.bottom() - t,
            h: t,
            ..r
        },
        Rect { w: t, ..r },
        Rect {
            x: r.right() - t,
            w: t,
            ..r
        },
    ] {
        CGContext::fill_rect(c, cg_rect(e));
    }
}

fn fill(c: Option<&CGContext>, r: Rect, color: Rgb) {
    set_fill(c, color, 1.0);
    CGContext::fill_rect(c, cg_rect(r));
}

/// Nền và viền 1 px của một bảng/thanh.
fn panel(c: Option<&CGContext>, r: Rect, dpi: u32) {
    fill(c, r, SURFACE);
    set_stroke(c, OVERLAY_BORDER, 1.0);
    CGContext::stroke_rect_with_width(c, cg_rect(r), f64::from(scaled(1, dpi)));
}

/// Khung nét đứt quanh ô chữ (chỉ trên màn hình, không bao giờ vào ảnh).
fn dashed_frame(c: Option<&CGContext>, r: Rect, dpi: u32) {
    let (dash, gap, t) = (
        scaled(TEXT_DASH, dpi),
        scaled(TEXT_DASH_GAP, dpi),
        scaled(1, dpi),
    );
    set_fill(c, ACCENT, 1.0);
    let mut x = r.x;
    while x < r.right() {
        let w = dash.min(r.right() - x);
        for y in [r.y, r.bottom() - t] {
            CGContext::fill_rect(c, cg_rect(Rect { x, y, w, h: t }));
        }
        x += dash + gap;
    }
    let mut y = r.y;
    while y < r.bottom() {
        let h = dash.min(r.bottom() - y);
        for x in [r.x, r.right() - t] {
            CGContext::fill_rect(c, cg_rect(Rect { x, y, w: t, h }));
        }
        y += dash + gap;
    }
}

/// Tay nắm vuông cạnh `hs` tâm (`cx`, `cy`), trắng viền `ACCENT`.
fn handle_square(c: Option<&CGContext>, cx: i32, cy: i32, hs: i32, lw: f64) {
    let hr = Rect {
        x: cx - hs / 2,
        y: cy - hs / 2,
        w: hs,
        h: hs,
    };
    set_fill(c, WHITE, 1.0);
    CGContext::fill_rect(c, cg_rect(hr));
    set_stroke(c, ACCENT, 1.0);
    CGContext::stroke_rect_with_width(c, cg_rect(hr), lw);
}

/// Vẽ hình đánh dấu, từ dưới lên trên: hình vẽ SAU nằm PHÍA TRƯỚC. Hình đang kéo vẽ thay hình đó
/// ở đúng vị trí của nó trong thứ tự; nét đang vẽ dở rồi chữ đang gõ vẽ SAU CÙNG (trên cùng).
fn draw_marks(cg: &CGContext, a: &Annot, clip: Rect) {
    for layer in a.layers() {
        match layer {
            Layer::Run(run) => {
                annotate_draw::draw_shapes(cg, run, 0, 0, clip);
            }
            Layer::Preview(s) | Layer::Stroke(s) => {
                annotate_draw::draw_shapes(cg, std::slice::from_ref(s), 0, 0, clip);
            }
            Layer::Boxed(s) | Layer::Typing(s) => {
                annotate_draw::draw_shapes(cg, std::slice::from_ref(&s), 0, 0, clip);
            }
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
    CGContext::draw_image(c, cg_rect(full), Some(f.image));
    CGContext::restore_g_state(c);

    // Lớp mờ, khoét vùng chọn (chẵn-lẻ).
    set_fill(c, Rgb(0, 0, 0), DIM_ALPHA);
    CGContext::begin_path(c);
    CGContext::add_rect(c, cg_rect(full));
    if let Some(r) = f.state.rect {
        CGContext::add_rect(c, cg_rect(r));
    }
    CGContext::eo_fill_path(c);

    // Hình đánh dấu: chỉ trong vùng chọn, trên ảnh đã khoét mờ.
    if let (Some(r), Some(a)) = (f.state.rect, f.annot) {
        draw_marks(&cg, a, r);
        // Khung và tay nắm của hình đang chọn: chỉ trên màn hình, không qua `burn`.
        if let Some((shape, b)) = a.shown_selection() {
            let lw = f64::from(scaled(1, dpi));
            if let Some(fr) = frame(shape, b) {
                set_stroke(c, ACCENT, 1.0);
                CGContext::stroke_rect_with_width(c, cg_rect(inflate(fr, -1)), lw);
            }
            let hs = scaled(HANDLE_SIZE, dpi);
            for (_, p) in handles(shape, b) {
                handle_square(c, p.x.round() as i32, p.y.round() as i32, hs, lw);
            }
        }
        // Khung nét đứt và con trỏ nháy của ô đang gõ.
        if let Some(t) = a.text() {
            dashed_frame(c, t.frame, dpi);
            if a.caret_on() {
                fill(c, t.caret, ACCENT);
            }
        }
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
                handle_square(c, x, y, hs, lw);
            }
        }
        // Nhãn kích thước pixel thật.
        let text = NSString::from_str(&size_label(r));
        let size = text_size(&text, &attrs);
        let lr = label_rect(r, size, dpi, full);
        bubble(c, lr, OVERLAY_LABEL, &text, &attrs, size);
    }

    let mut tip: Option<(Msg, Rect)> = None;
    if let Some(bar) = f.state.bar {
        draw_bar(c, f, bar, &mut tip);
    }
    if let (Some(a), Some(lay)) = (f.annot, f.env) {
        if let Some(side) = lay.side {
            draw_side(c, f, a, side, &lay);
        }
    }
    if let Some((msg, anchor)) = tip {
        let text = NSString::from_str(lang::tr(msg));
        let size = text_size(&text, &attrs);
        let r = tip_rect(anchor, size, dpi, full);
        bubble(c, r, OVERLAY_TIP, &text, &attrs, size);
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

/// Thanh ngang: Hoàn tác, Làm lại | Tải lên, Lưu, Chép, Hủy.
fn draw_bar(c: Option<&CGContext>, f: &Frame, bar: Rect, tip: &mut Option<(Msg, Rect)>) {
    let dpi = f.dpi;
    let (can_undo, can_redo) = f
        .annot
        .map_or((false, false), |a| (a.can_undo(), a.can_redo()));
    let enabled = |it: BarItem| match it {
        BarItem::Undo => can_undo,
        BarItem::Redo => can_redo,
        BarItem::Act(_) => true,
    };
    fill(c, bar, SURFACE);
    let margin = scaled(GAP, dpi);
    let mut prev: Option<BarItem> = None;
    for (item, r) in bar_layout(dpi).cells {
        let cell = Rect {
            x: bar.x + r.x,
            y: bar.y + r.y,
            ..r
        };
        match item {
            Some(it) => {
                let hot = (f.state.pressed == Some(it) || f.state.hover == Some(it)) && enabled(it);
                if hot {
                    fill(c, inflate(cell, -1), OVERLAY_HOVER);
                }
                // Vạch 1 px giữa hai nút hành động liền nhau.
                if matches!((prev, it), (Some(BarItem::Act(_)), BarItem::Act(_))) {
                    set_fill(c, OVERLAY_BORDER, 1.0);
                    CGContext::fill_rect(
                        c,
                        cg_rect(Rect {
                            x: cell.x,
                            y: cell.y + 1,
                            w: 1,
                            h: cell.h - 2,
                        }),
                    );
                }
                let color = if !enabled(it) {
                    TEXT_DISABLED
                } else if hot {
                    ACCENT
                } else {
                    OVERLAY_ICON
                };
                let g = match it {
                    BarItem::Undo => Glyph::Undo,
                    BarItem::Redo => Glyph::Redo,
                    BarItem::Act(a) => Glyph::Act(a),
                };
                glyph(c, cell, dpi, g, color);
            }
            None => fill(
                c,
                Rect {
                    x: cell.x + cell.w / 2,
                    y: cell.y + margin,
                    w: 1,
                    h: cell.h - 2 * margin,
                },
                OVERLAY_BORDER,
            ),
        }
        prev = item;
    }
    set_stroke(c, OVERLAY_BORDER, 1.0);
    CGContext::stroke_rect_with_width(c, cg_rect(bar), f64::from(scaled(1, dpi)));
    if let Some(it) = f.state.hover {
        if let Some(cell) = bar_button_rect(bar, dpi, it) {
            *tip = Some((bar_tip(it), cell));
        }
    }
}

/// Thanh dọc công cụ cùng bảng Cỡ/Hình/màu đang mở; gợi ý của mục đang trỏ (đặt cạnh thanh dọc
/// hoặc bảng) vẽ ngay ở đây.
fn draw_side(c: Option<&CGContext>, f: &Frame, a: &Annot, side: Rect, lay: &Layout) {
    let (sel, mon) = (lay.sel, lay.mon);
    let dpi = f.dpi;
    // Gợi ý gom lại và vẽ SAU CÙNG: không bao giờ bị bảng hay ô khác vẽ đè.
    let mut tips: Vec<PendingTip> = Vec::new();
    let sl = side_layout_for(mon, dpi);
    panel(c, side, dpi);
    let mark = scaled(MARK, dpi);
    let mark_inset = scaled(6, dpi);
    let icon_px = f64::from(scaled(ICON, dpi));
    let attrs = text_attrs(FONT_PT * f.scale);
    for (i, item) in SIDE_ITEMS.iter().enumerate() {
        let Some(r) = side_item_rect(side, &sl, i) else {
            continue;
        };
        let selected = a.side_selected(*item);
        let hot = selected || a.side_hover() == Some(*item);
        if hot {
            fill(c, inflate(r, -1), OVERLAY_HOVER);
        }
        if selected {
            fill(
                c,
                Rect {
                    x: r.x + 1,
                    y: r.y + mark_inset,
                    w: mark,
                    h: r.h - 2 * mark_inset,
                },
                ACCENT,
            );
        }
        let color = if hot { ACCENT } else { OVERLAY_ICON };
        match item {
            SideItem::Tool(t) => glyph(c, r, dpi, Glyph::Tool(*t), color),
            SideItem::Shapes => {
                glyph(c, r, dpi, Glyph::Tool(a.shape_tool()), color);
                corner_triangle(c, r, dpi, color);
            }
            SideItem::Size => dot(c, r, f64::from(size_dot(a.size())) * icon_px / 24.0, color),
            SideItem::Color => {
                // Ô vuông tô màu hiện tại, viền `OVERLAY_BORDER`.
                let sw = scaled(SWATCH, dpi);
                let sq = Rect {
                    x: r.x + (r.w - sw) / 2,
                    y: r.y + (r.h - sw) / 2,
                    w: sw,
                    h: sw,
                };
                fill(c, sq, a.color());
                set_stroke(c, OVERLAY_BORDER, 1.0);
                CGContext::stroke_rect_with_width(c, cg_rect(sq), f64::from(scaled(1, dpi)));
            }
        }
        if a.side_hover() == Some(*item) && a.flyout() == Flyout::None {
            let text = NSString::from_str(lang::tr(item.tip()));
            let size = text_size(&text, &attrs);
            let rect = side_tip_rect(side, r, sel, mon, dpi, size);
            tips.push(PendingTip {
                rect,
                text: lang::tr(item.tip()),
                size,
            });
        }
    }
    // Vạch ngăn 1 px giữa ô vạch ngăn, có lề hai bên.
    let margin = scaled(GAP, dpi);
    for s in &sl.seps {
        fill(
            c,
            Rect {
                x: side.x + s.x + margin,
                y: side.y + s.y + s.h / 2,
                w: s.w - 2 * margin,
                h: 1,
            },
            OVERLAY_BORDER,
        );
    }
    // Bảng thuộc tính của Hình/Chữ (khi không có bảng Cỡ/Hình/Màu mở).
    if a.style_panel_open() {
        draw_style_panel(c, f, a, lay, &attrs, &mut tips);
    }
    // Bảng đang mở.
    let Some(fly) = flyout_rect_for(a.flyout(), side, sel, mon, dpi) else {
        flush_tips(c, &attrs, tips);
        return;
    };
    panel(c, fly, dpi);
    let t = scaled(MARK, dpi);
    match a.flyout() {
        Flyout::None => {}
        Flyout::Sizes => {
            for (i, level) in SizeLevel::ALL.iter().enumerate() {
                let cell = size_cell_rect(fly, dpi, i);
                let on = *level == a.size();
                let hot = a.size_hover() == Some(*level);
                if hot {
                    fill(c, inflate(cell, -1), OVERLAY_HOVER);
                }
                if on {
                    inner_frame(c, cell, t, ACCENT);
                }
                dot(
                    c,
                    cell,
                    f64::from(size_dot(*level)) * icon_px / 24.0,
                    if on || hot { ACCENT } else { OVERLAY_ICON },
                );
                if hot {
                    queue_flyout_tip(&mut tips, &attrs, cell, fly, mon, dpi, size_tip(*level));
                }
            }
        }
        Flyout::Shapes => {
            for (i, tool) in SHAPE_TOOLS.iter().enumerate() {
                let cell = shape_cell_rect(fly, dpi, i);
                let on = a.tool() == Some(*tool);
                let hot = a.shape_hover() == Some(*tool);
                if hot {
                    fill(c, inflate(cell, -1), OVERLAY_HOVER);
                }
                if on {
                    inner_frame(c, cell, t, ACCENT);
                }
                glyph(
                    c,
                    cell,
                    dpi,
                    Glyph::Tool(*tool),
                    if on || hot { ACCENT } else { OVERLAY_ICON },
                );
                if hot {
                    queue_flyout_tip(&mut tips, &attrs, cell, fly, mon, dpi, shape_tip(*tool));
                }
            }
        }
        Flyout::Palette => {
            for (i, color) in PALETTE.iter().enumerate() {
                let cell = palette_cell_rect(fly, dpi, i);
                fill(c, cell, *color);
                if *color == a.color() {
                    // Ô màu đang dùng: viền trắng (nổi trên mọi màu) rồi viền `ACCENT` trong.
                    inner_frame(c, cell, t, WHITE);
                    inner_frame(c, inflate(cell, -t), t.max(1), ACCENT);
                } else if a.color_hover() == Some(i) {
                    inner_frame(c, cell, t, WHITE);
                }
            }
        }
    }
    flush_tips(c, &attrs, tips);
}

/// Chữ `s` trong ô `r`: giữa ô, hoặc sát trái (đệm nhỏ) khi `centered` là false.
fn text_in(
    r: Rect,
    s: &str,
    attrs: &NSDictionary<NSAttributedStringKey, AnyObject>,
    centered: bool,
    dpi: u32,
) {
    let text = NSString::from_str(s);
    let size = text_size(&text, attrs);
    let x = if centered {
        f64::from(r.x) + f64::from(r.w - size.0) / 2.0
    } else {
        f64::from(r.x + scaled(2, dpi))
    };
    let at = CGPoint::new(x, f64::from(r.y) + f64::from(r.h - size.1) / 2.0);
    // SAFETY: như `bubble`.
    unsafe { text.drawAtPoint_withAttributes(at, Some(attrs)) };
}

/// Nút của bảng thuộc tính: nền khi trỏ, viền `ACCENT` khi đang bật.
fn style_button(c: Option<&CGContext>, r: Rect, on: bool, hot: bool, dpi: u32) {
    if hot {
        fill(c, inflate(r, -1), OVERLAY_HOVER);
    }
    if on {
        inner_frame(c, r, scaled(MARK, dpi).max(1), ACCENT);
    }
}

/// Ba vạch của biểu tượng căn lề trong ô `r`.
fn align_glyph(c: Option<&CGContext>, r: Rect, a: Align, color: Rgb) {
    let (w, h) = (r.w * 5 / 10, r.h * 5 / 10);
    let (x0, y0) = (r.x + (r.w - w) / 2, r.y + (r.h - h) / 2);
    let bar_h = (h / 5).max(1);
    for (i, frac) in [10, 6, 8].iter().enumerate() {
        let bw = w * frac / 10;
        let x = match a {
            Align::Left => x0,
            Align::Center => x0 + (w - bw) / 2,
            Align::Right => x0 + w - bw,
        };
        let y = y0 + i as i32 * (h - bar_h) / 2;
        fill(
            c,
            Rect {
                x,
                y,
                w: bw,
                h: bar_h,
            },
            color,
        );
    }
}

fn style_tip(cell: StyleCell) -> Option<Msg> {
    match cell {
        StyleCell::Align(Align::Left) => Some(Msg::AlignLeft),
        StyleCell::Align(Align::Center) => Some(Msg::AlignCenter),
        StyleCell::Align(Align::Right) => Some(Msg::AlignRight),
        _ => None,
    }
}

/// Bảng thuộc tính của Hình, Chữ và Highlight: thanh tiêu đề (kéo, thu gọn), ô Nền/Viền/Không nền,
/// 48 màu, độ dày viền, Không viền, bo góc, độ trong suốt và (Chữ) căn lề.
fn draw_style_panel(
    c: Option<&CGContext>,
    f: &Frame,
    a: &Annot,
    lay: &Layout,
    attrs: &NSDictionary<NSAttributedStringKey, AnyObject>,
    tips: &mut Vec<PendingTip>,
) {
    let dpi = f.dpi;
    let Some(view) = a.style_view(dpi) else {
        return;
    };
    let Some((rect, collapsed)) = a.style_panel_in(view.kind, lay) else {
        return;
    };
    let mon = lay.mon;
    let l: StyleLayout = style_layout_ex(rect, view.kind, collapsed, dpi);
    panel(c, rect, dpi);
    let small = text_attrs(FONT_PT * f.scale * 0.9);
    let hover = a.style_hover();
    let target = a.style_target();
    let t = scaled(MARK, dpi).max(1);
    // Thanh tiêu đề: vạch nắm kéo và nút thu gọn/mở.
    let hot_head = hover == Some(StyleCell::Header);
    let hot_toggle = hover == Some(StyleCell::Collapse);
    if hot_head {
        fill(c, inflate(l.header, -1), OVERLAY_HOVER);
    }
    for i in 0..2 {
        let gy = l.header.y + l.header.h / 2 + (i * 2 - 1) * scaled(2, dpi);
        fill(
            c,
            Rect {
                x: l.header.x + scaled(8, dpi),
                y: gy,
                w: l.header.w - l.collapse.w - scaled(16, dpi),
                h: 1,
            },
            OVERLAY_BORDER,
        );
    }
    style_button(c, l.collapse, false, hot_toggle, dpi);
    text_in(
        l.collapse,
        if collapsed { "\u{25B4}" } else { "\u{25BE}" },
        &small,
        true,
        dpi,
    );
    if hot_toggle {
        queue_flyout_tip(
            tips,
            attrs,
            l.collapse,
            rect,
            mon,
            dpi,
            Msg::StylePanelToggle,
        );
    }
    if collapsed {
        return;
    }
    // Hàng ô: Nền, Viền, Không nền. Đúng một trong Nền/Không nền bật.
    let highlight = view.kind.is_highlight();
    let focus = chip_focus(view.kind, target, view.fill);
    let mut chips = Vec::new();
    if !highlight {
        if view.kind == StyleKind::Label {
            chips.push((
                l.text_chip,
                Msg::StyleTextColor,
                focus.text,
                StyleCell::Target(StyleTarget::Text),
            ));
        }
        chips.extend([
            (
                l.fill_chip,
                Msg::StyleFill,
                focus.fill,
                StyleCell::Target(StyleTarget::Fill),
            ),
            (
                l.border_chip,
                Msg::StyleBorder,
                focus.border,
                StyleCell::Target(StyleTarget::Border),
            ),
            (
                l.no_fill,
                Msg::StyleNoFill,
                focus.no_fill,
                StyleCell::NoFill,
            ),
        ]);
    }
    for (r, msg, on, cell) in chips {
        style_button(c, r, on, hover == Some(cell), dpi);
        text_in(r, lang::tr(msg), &small, true, dpi);
    }
    // Bảng Highlight: hai chế độ thay hàng Nền/Viền.
    if let (Some(modes), Some(cur)) = (l.modes, view.mode) {
        for (m, r) in HighlightMode::ALL.iter().zip(modes) {
            style_button(c, r, cur == *m, hover == Some(StyleCell::Mode(*m)), dpi);
            let msg = match m {
                HighlightMode::Freehand => Msg::HighlightFreehand,
                HighlightMode::Rect => Msg::HighlightRect,
            };
            text_in(r, lang::tr(msg), &small, true, dpi);
        }
    }
    // 48 màu: ô đang áp cho nền hoặc viền có viền trắng rồi `ACCENT`.
    let active = match target {
        _ if highlight => Some(view.stroke),
        StyleTarget::Fill => view.fill,
        StyleTarget::Border => Some(view.stroke),
        StyleTarget::Text => Some(view.text),
    };
    for (i, color) in PALETTE.iter().enumerate() {
        let Some(cell) = l.cells.get(i).copied() else {
            continue;
        };
        fill(c, cell, *color);
        if Some(*color) == active {
            inner_frame(c, cell, t, WHITE);
            inner_frame(c, inflate(cell, -t), t.max(1), ACCENT);
        } else if hover == Some(StyleCell::Color(i)) {
            inner_frame(c, cell, t, WHITE);
        }
    }
    // Các hàng tăng giảm: nhãn, [-], giá trị, [+].
    let width = format!("{}", view.width_dip.round() as i32);
    let radius = format!("{}", view.radius_px.round() as i32);
    let opacity = format!("{}%", opacity_percent(view.opacity));
    let width_msg = if view.kind == StyleKind::MarkerFree {
        Msg::HighlightSize
    } else {
        Msg::StyleWidth
    };
    let rows = [
        (
            l.width_label,
            width_msg,
            (l.width_minus, StyleCell::Width(-1)),
            l.width_value,
            width,
            (l.width_plus, StyleCell::Width(1)),
            // Highlight hình chữ nhật không có độ dày nét.
            view.kind != StyleKind::MarkerRect,
        ),
        (
            l.radius_label,
            Msg::StyleRadius,
            (l.radius_minus, StyleCell::Radius(-1)),
            l.radius_value,
            radius,
            (l.radius_plus, StyleCell::Radius(1)),
            view.kind.has_radius(),
        ),
        (
            l.opacity_label,
            Msg::StyleOpacity,
            (l.opacity_minus, StyleCell::Opacity(-1)),
            l.opacity_value,
            opacity,
            (l.opacity_plus, StyleCell::Opacity(1)),
            true,
        ),
    ];
    for (label, msg, minus, value, text, plus, shown) in rows {
        if !shown {
            continue;
        }
        text_in(label, lang::tr(msg), &small, false, dpi);
        for (r, cell) in [minus, plus] {
            style_button(c, r, false, hover == Some(cell), dpi);
            inner_frame(c, r, 1, OVERLAY_BORDER);
            let sign = if matches!(
                cell,
                StyleCell::Width(-1) | StyleCell::Opacity(-1) | StyleCell::Radius(-1)
            ) {
                "\u{2212}"
            } else {
                "+"
            };
            text_in(r, sign, attrs, true, dpi);
        }
        text_in(value, &text, attrs, true, dpi);
    }
    // Ô tích Không viền (Hình, Chữ).
    if view.kind.has_border() {
        let r = l.no_border;
        style_button(c, r, false, hover == Some(StyleCell::NoBorder), dpi);
        let side = (r.h * 2 / 3).max(8);
        let bx = Rect {
            x: r.x + scaled(2, dpi),
            y: r.y + (r.h - side) / 2,
            w: side,
            h: side,
        };
        inner_frame(
            c,
            bx,
            1,
            if view.no_border {
                ACCENT
            } else {
                OVERLAY_BORDER
            },
        );
        if view.no_border {
            // Dấu tích: hai đoạn.
            let inner = inflate(bx, -scaled(3, dpi));
            set_stroke(c, ACCENT, 1.0);
            CGContext::set_line_width(c, f64::from(scaled(2, dpi)));
            CGContext::move_to_point(c, f64::from(inner.x), f64::from(inner.y + inner.h / 2));
            CGContext::add_line_to_point(
                c,
                f64::from(inner.x + inner.w / 3),
                f64::from(inner.bottom()),
            );
            CGContext::add_line_to_point(c, f64::from(inner.right()), f64::from(inner.y));
            CGContext::stroke_path(c);
        }
        let label = Rect {
            x: bx.right() + scaled(6, dpi),
            w: r.right() - bx.right() - scaled(6, dpi),
            ..r
        };
        text_in(label, lang::tr(Msg::StyleNoBorder), &small, false, dpi);
    }
    // Căn lề (Chữ).
    if let (Some(aligns), Some(cur)) = (l.aligns, view.align) {
        for (al, r) in Align::ALL.iter().zip(aligns) {
            let cell = StyleCell::Align(*al);
            let on = cur == *al;
            let hot = hover == Some(cell);
            style_button(c, r, on, hot, dpi);
            align_glyph(c, r, *al, if on || hot { ACCENT } else { OVERLAY_ICON });
            if hot {
                if let Some(msg) = style_tip(cell) {
                    queue_flyout_tip(tips, attrs, r, rect, mon, dpi, msg);
                }
            }
        }
    }
}

/// Gợi ý chờ vẽ: vị trí đã tính, chữ và cỡ chữ.
struct PendingTip {
    rect: Rect,
    text: &'static str,
    size: (i32, i32),
}

/// Xếp gợi ý của ô `cell` trong bảng `fly` (đặt trên hoặc dưới bảng) vào hàng chờ.
fn queue_flyout_tip(
    tips: &mut Vec<PendingTip>,
    attrs: &NSDictionary<NSAttributedStringKey, AnyObject>,
    cell: Rect,
    fly: Rect,
    mon: Rect,
    dpi: u32,
    msg: Msg,
) {
    let text = lang::tr(msg);
    let size = text_size(&NSString::from_str(text), attrs);
    tips.push(PendingTip {
        rect: flyout_tip_rect(cell, fly, mon, dpi, size),
        text,
        size,
    });
}

/// Vẽ mọi gợi ý đang chờ, sau cùng để nằm trên mọi thanh, bảng và ô.
fn flush_tips(
    c: Option<&CGContext>,
    attrs: &NSDictionary<NSAttributedStringKey, AnyObject>,
    tips: Vec<PendingTip>,
) {
    for t in tips {
        bubble(
            c,
            t.rect,
            OVERLAY_TIP,
            &NSString::from_str(t.text),
            attrs,
            t.size,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mọi nét của mọi biểu tượng nằm trong lưới 24×24 (không tràn ô nút).
    #[test]
    fn glyph_strokes_stay_inside_the_24_grid() {
        let mut glyphs = vec![Glyph::Undo, Glyph::Redo];
        glyphs.extend(Tool::ALL.iter().map(|t| Glyph::Tool(*t)));
        glyphs.extend(
            [Action::Upload, Action::Save, Action::Copy, Action::Cancel]
                .iter()
                .map(|a| Glyph::Act(*a)),
        );
        for g in glyphs {
            let strokes = glyph_strokes(g);
            assert!(!strokes.is_empty(), "{g:?}");
            for st in strokes {
                assert!(st.pts.len() >= 2, "{g:?}");
                for (x, y) in st.pts {
                    assert!(
                        (0.0..=24.0).contains(&x) && (0.0..=24.0).contains(&y),
                        "{g:?}: ({x}, {y})"
                    );
                }
            }
        }
    }

    #[test]
    fn arc_runs_clockwise_on_a_y_down_screen() {
        // 270° là đỉnh (y nhỏ), quét +180° qua bên phải xuống đáy.
        let p = arc_pts(10.0, 10.0, 5.0, 270.0, 180.0);
        let first = p[0];
        let last = p[p.len() - 1];
        assert!((first.0 - 10.0).abs() < 1e-9 && (first.1 - 5.0).abs() < 1e-9);
        assert!((last.0 - 10.0).abs() < 1e-9 && (last.1 - 15.0).abs() < 1e-9);
        assert!(p.iter().any(|(x, _)| *x > 14.9));
        // Quét âm đi qua bên trái.
        let q = arc_pts(10.0, 10.0, 5.0, 270.0, -180.0);
        assert!(q.iter().any(|(x, _)| *x < 5.1));
    }

    #[test]
    fn oval_has_the_requested_half_axes() {
        let o = oval(12.0, 12.0, 8.5, 6.0);
        assert!(o.closed);
        let xs = o.pts.iter().map(|p| p.0);
        let ys = o.pts.iter().map(|p| p.1);
        assert!((xs.clone().fold(f64::MIN, f64::max) - 20.5).abs() < 1e-6);
        assert!((xs.fold(f64::MAX, f64::min) - 3.5).abs() < 1e-6);
        assert!((ys.clone().fold(f64::MIN, f64::max) - 18.0).abs() < 1e-6);
        assert!((ys.fold(f64::MAX, f64::min) - 6.0).abs() < 1e-6);
    }

    #[test]
    fn text_commands_map_from_selectors() {
        assert_eq!(command_for(sel!(deleteBackward:)), Some(TextCmd::Backspace));
        assert_eq!(command_for(sel!(deleteForward:)), Some(TextCmd::Delete));
        assert_eq!(command_for(sel!(moveLeft:)), Some(TextCmd::Left));
        assert_eq!(command_for(sel!(moveRight:)), Some(TextCmd::Right));
        assert_eq!(command_for(sel!(moveUp:)), Some(TextCmd::Up));
        assert_eq!(command_for(sel!(moveDown:)), Some(TextCmd::Down));
        assert_eq!(
            command_for(sel!(moveToBeginningOfLine:)),
            Some(TextCmd::Home)
        );
        assert_eq!(command_for(sel!(moveToEndOfLine:)), Some(TextCmd::End));
        assert_eq!(command_for(sel!(insertNewline:)), Some(TextCmd::Newline));
        // Tab, Esc và lệnh lạ không làm gì.
        assert_eq!(command_for(sel!(insertTab:)), None);
        assert_eq!(command_for(sel!(cancelOperation:)), None);
        assert_eq!(command_for(sel!(noop:)), None);
    }
}
