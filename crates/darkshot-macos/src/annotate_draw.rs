//! Vẽ các hình đánh dấu bằng Core Graphics và CoreText (Task 110): một hàm vẽ dùng chung cho lớp phủ
//! và cho ảnh ra (`burn`), tương đương `darkshot-windows/src/annotate_draw.rs` (GDI+).
//!
//! Quy ước: ngữ cảnh nhận vào có không gian người dùng đơn vị pixel, gốc TRÊN TRÁI, trục y hướng
//! xuống (lớp phủ vẽ trong view đã lật; `burn` tự lật). Chữ vẽ bằng ma trận chữ lật y. Xếp lớp:
//! `draw_shapes` duyệt danh sách theo chỉ số tăng dần, hình sau đè lên hình trước.

use darkshot_core::annotate::{
    align_offsets, arrow_geometry, arrow_min_head, blur_pixel_rect, outline_color, Align, Pt,
    Shape, Style, TextEdit, LABEL_PAD_EM, MARKER_ALPHA, STEP_RING, TEXT_DIP, TEXT_OUTLINE_DIP,
};
use darkshot_core::geometry::Rect;
use darkshot_core::image::Image;
use darkshot_core::theme::Rgb;
use objc2_core_foundation::{
    kCFBooleanTrue, CFAttributedString, CFDictionary, CFRetained, CFString, CFType,
    CGAffineTransform, CGFloat, CGPoint, CGRect, CGSize,
};
use objc2_core_graphics::{
    kCGColorSpaceSRGB, CGBitmapContextCreate, CGBitmapInfo, CGColorSpace, CGContext,
    CGImageAlphaInfo, CGImageByteOrderInfo, CGLineCap, CGLineJoin, CGTextDrawingMode,
};
use objc2_core_text::{
    kCTFontAttributeName, kCTForegroundColorFromContextAttributeName, CTFont, CTFontUIFontType,
    CTLine, CTLineBoundsOptions,
};
use std::ffi::c_void;

/// Tọa độ có độ lớn vượt ngưỡng này (hoặc không hữu hạn) coi như không vẽ được: Core Graphics
/// ghi cảnh báo và có thể chạy rất lâu với số khổng lồ.
const COORD_LIMIT: f32 = 1.0e7;

fn sane(v: f32) -> bool {
    v.is_finite() && v.abs() <= COORD_LIMIT
}

fn sane_pt(p: Pt) -> bool {
    sane(p.x) && sane(p.y)
}

fn f(v: f32) -> CGFloat {
    CGFloat::from(v)
}

fn cg_rect(x: f32, y: f32, w: f32, h: f32) -> CGRect {
    CGRect::new(CGPoint::new(f(x), f(y)), CGSize::new(f(w), f(h)))
}

fn set_fill(c: &CGContext, color: Rgb, alpha: f32) {
    CGContext::set_rgb_fill_color(
        Some(c),
        f64::from(color.0) / 255.0,
        f64::from(color.1) / 255.0,
        f64::from(color.2) / 255.0,
        f64::from(alpha),
    );
}

fn set_stroke(c: &CGContext, color: Rgb, alpha: f32) {
    CGContext::set_rgb_stroke_color(
        Some(c),
        f64::from(color.0) / 255.0,
        f64::from(color.1) / 255.0,
        f64::from(color.2) / 255.0,
        f64::from(alpha),
    );
}

/// Điểm chung khi mọi điểm của nét trùng nhau (nét chỉ là một chấm); `None` khi rỗng hoặc có điểm khác.
fn single_spot(points: &[Pt]) -> Option<Pt> {
    let first = *points.first()?;
    points.iter().all(|p| *p == first).then_some(first)
}

/// Đường gấp khúc qua `points` (chưa vẽ).
fn polyline(c: &CGContext, points: &[Pt]) {
    CGContext::begin_path(Some(c));
    if let Some(first) = points.first() {
        CGContext::move_to_point(Some(c), f(first.x), f(first.y));
        for p in &points[1..] {
            CGContext::add_line_to_point(Some(c), f(p.x), f(p.y));
        }
    }
}

// ---------------------------------------------------------------- chữ (CoreText)

/// Phông chữ đánh dấu (phông hệ thống đậm) cỡ `size` px cùng số đo dòng.
struct TextStyle {
    font: CFRetained<CTFont>,
    ascent: f32,
    /// Chiều cao một dòng: ascent + descent + leading.
    line_height: f32,
}

impl TextStyle {
    fn new(size: f32) -> Option<TextStyle> {
        if !size.is_finite() || size <= 0.0 || size > 10_000.0 {
            return None;
        }
        // SAFETY: tham số hợp lệ; `None` ngôn ngữ = ngôn ngữ hệ thống. Kết quả được kiểm `Option`.
        let font = unsafe {
            CTFont::new_ui_font_for_language(CTFontUIFontType::EmphasizedSystem, f(size), None)
        }?;
        // SAFETY: phông hợp lệ vừa tạo; các hàm chỉ đọc.
        let (a, d, l) = unsafe { (font.ascent(), font.descent(), font.leading()) };
        let (ascent, lh) = (a as f32, (a + d + l) as f32);
        (ascent.is_finite() && lh.is_finite() && lh > 0.0).then_some(TextStyle {
            font,
            ascent,
            line_height: lh,
        })
    }

    /// CTLine của một dòng (không chứa '\n'); màu lấy từ ngữ cảnh khi vẽ.
    fn line(&self, s: &str) -> Option<CFRetained<CTLine>> {
        let text = CFString::from_str(s);
        // SAFETY: hai khóa là hằng CoreText sống suốt tiến trình; `kCFBooleanTrue` là hằng CF.
        let (key_font, key_fg, yes) = unsafe {
            (
                kCTFontAttributeName,
                kCTForegroundColorFromContextAttributeName,
                kCFBooleanTrue?,
            )
        };
        let font: &CFType = self.font.as_ref();
        let yes: &CFType = yes.as_ref();
        let dict = CFDictionary::<CFString, CFType>::from_slices(&[key_font, key_fg], &[font, yes]);
        // SAFETY: chuỗi và từ điển hợp lệ (các khóa/giá trị là CFType); kết quả kiểm `Option`.
        let attr = unsafe { CFAttributedString::new(None, Some(&text), Some(dict.as_opaque())) }?;
        // SAFETY: chuỗi thuộc tính hợp lệ.
        Some(unsafe { CTLine::with_attributed_string(&attr) })
    }
}

/// Hộp mực (không viền) của một dòng, trong không gian y hướng lên có gốc là điểm đặt chữ ở đường
/// cơ sở: `(x, y, w, h)`; `None` khi dòng không có nét (rỗng, toàn khoảng trắng).
fn ink_box(line: &CTLine) -> Option<(f32, f32, f32, f32)> {
    // SAFETY: dòng hợp lệ; hàm chỉ đọc.
    let r = unsafe { line.bounds_with_options(CTLineBoundsOptions::UseGlyphPathBounds) };
    let (x, y, w, h) = (
        r.origin.x as f32,
        r.origin.y as f32,
        r.size.width as f32,
        r.size.height as f32,
    );
    (x.is_finite() && y.is_finite() && w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0)
        .then_some((x, y, w, h))
}

/// Bề rộng tiến (gồm khoảng trắng cuối) của `s` theo `style`.
fn advance_width(style: &TextStyle, s: &str) -> Option<f32> {
    if s.is_empty() {
        return Some(0.0);
    }
    let line = style.line(s)?;
    let mut ascent: CGFloat = 0.0;
    let mut descent: CGFloat = 0.0;
    let mut leading: CGFloat = 0.0;
    // SAFETY: dòng hợp lệ; ba con trỏ ra trỏ tới biến cục bộ.
    let w = unsafe { line.typographic_bounds(&mut ascent, &mut descent, &mut leading) };
    (w.is_finite() && w >= 0.0).then_some(w as f32)
}

/// Nét ngoài của chữ: dày 2 × TEXT_OUTLINE_DIP đổi ra px theo tỉ lệ `size / TEXT_DIP`.
fn outline_width(size: f32) -> f32 {
    2.0 * TEXT_OUTLINE_DIP * size / TEXT_DIP
}

/// Vẽ một dòng với đường cơ sở tại `(x, baseline)` (y hướng xuống): trước là nét ngoài, sau là ruột
/// chữ để ruột nằm trên nét. `ink` là màu nét ngoài (`None`: không viền).
fn draw_line(
    c: &CGContext,
    line: &CTLine,
    x: f32,
    baseline: f32,
    fill: Rgb,
    outline: Option<(Rgb, f32)>,
) {
    CGContext::save_g_state(Some(c));
    // Không gian y hướng xuống: lật ma trận chữ để chữ đứng thẳng.
    CGContext::set_text_matrix(
        Some(c),
        CGAffineTransform {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: -1.0,
            tx: 0.0,
            ty: 0.0,
        },
    );
    CGContext::set_text_position(Some(c), f(x), f(baseline));
    CGContext::set_should_smooth_fonts(Some(c), false);
    if let Some((color, width)) = outline {
        set_stroke(c, color, 1.0);
        CGContext::set_line_width(Some(c), f64::from(width));
        CGContext::set_line_join(Some(c), CGLineJoin::Round);
        CGContext::set_text_drawing_mode(Some(c), CGTextDrawingMode::Stroke);
        // SAFETY: ngữ cảnh và dòng hợp lệ; vẽ trên luồng gọi.
        unsafe { line.draw(c) };
    }
    // `CTLine::draw` dời vị trí chữ về cuối dòng: đặt lại trước lượt tô ruột.
    CGContext::set_text_position(Some(c), f(x), f(baseline));
    set_fill(c, fill, 1.0);
    CGContext::set_text_drawing_mode(Some(c), CGTextDrawingMode::Fill);
    // SAFETY: như trên.
    unsafe { line.draw(c) };
    CGContext::restore_g_state(Some(c));
}

fn draw_text(c: &CGContext, origin: Pt, text: &str, size: f32, color: Rgb) -> bool {
    if !sane_pt(origin) {
        return false;
    }
    let Some(style) = TextStyle::new(size) else {
        return false;
    };
    let outline = Some((outline_color(color), outline_width(size)));
    for (i, l) in text.split('\n').enumerate() {
        if l.is_empty() {
            continue;
        }
        let Some(line) = style.line(l) else {
            return false;
        };
        let baseline = origin.y + style.ascent + i as f32 * style.line_height;
        if !sane(baseline) {
            return false;
        }
        draw_line(c, &line, origin.x, baseline, color, outline);
    }
    true
}

/// Vòng đánh số: lòng màu `color`, viền và số màu `outline_color(color)`. Viền dày
/// `radius * STEP_RING` nằm trong hình tròn bán kính `radius`. `n == 0`: không vẽ số.
fn draw_step(c: &CGContext, center: Pt, radius: f32, n: u32, color: Rgb) -> bool {
    if !sane_pt(center) || !sane(radius) || radius <= 0.0 {
        return false;
    }
    let ring = radius * STEP_RING;
    let mid = radius - ring / 2.0;
    let ink = outline_color(color);
    let rect = cg_rect(center.x - mid, center.y - mid, 2.0 * mid, 2.0 * mid);
    set_fill(c, color, 1.0);
    CGContext::fill_ellipse_in_rect(Some(c), rect);
    set_stroke(c, ink, 1.0);
    CGContext::set_line_width(Some(c), f64::from(ring));
    CGContext::stroke_ellipse_in_rect(Some(c), rect);
    if n == 0 {
        return true;
    }
    let s = n.to_string();
    let mut size = radius * 1.1;
    let Some(mut style) = TextStyle::new(size) else {
        return false;
    };
    let Some(mut line) = style.line(&s) else {
        return false;
    };
    let Some(mut b) = ink_box(&line) else {
        return false;
    };
    let max_w = 1.5 * radius;
    if b.2 > max_w {
        let k = max_w / b.2;
        size *= k;
        let (Some(st),) = (TextStyle::new(size),) else {
            return false;
        };
        style = st;
        let Some(l) = style.line(&s) else {
            return false;
        };
        line = l;
        let Some(nb) = ink_box(&line) else {
            return false;
        };
        b = nb;
    }
    // Tâm hộp mực trùng `center`: y hướng lên nên mực trải từ baseline - (y + h) đến baseline - y.
    let x = center.x - (b.0 + b.2 / 2.0);
    let baseline = center.y + b.1 + b.3 / 2.0;
    draw_line(c, &line, x, baseline, ink, None);
    true
}

// ---------------------------------------------------------------- các hình khác

fn draw_arrow(c: &CGContext, a: Pt, b: Pt, width: f32, color: Rgb) -> bool {
    if !(sane_pt(a) && sane_pt(b) && sane(width)) || width <= 0.0 {
        return false;
    }
    let geo = arrow_geometry(a, b, width, arrow_min_head(width));
    set_fill(c, color, 1.0);
    set_stroke(c, color, 1.0);
    if geo.shaft_end != a {
        // Đầu tròn ở `a`, đầu phẳng ở phía mũi; thân lấn 1 px vào đầu mũi tên để không lộ đường nối
        // khử răng cưa giữa thân và tam giác.
        let (dx, dy) = (b.x - geo.shaft_end.x, b.y - geo.shaft_end.y);
        let depth = dx.hypot(dy);
        let k = if depth > 0.0 {
            1.0f32.min(depth / 2.0) / depth
        } else {
            0.0
        };
        let end = Pt {
            x: geo.shaft_end.x + dx * k,
            y: geo.shaft_end.y + dy * k,
        };
        CGContext::set_line_width(Some(c), f(width));
        CGContext::set_line_cap(Some(c), CGLineCap::Butt);
        polyline(c, &[a, end]);
        CGContext::stroke_path(Some(c));
        // Nắp tròn ở `a` (Core Graphics không cho hai đầu nút khác nhau).
        let r = width / 2.0;
        CGContext::fill_ellipse_in_rect(Some(c), cg_rect(a.x - r, a.y - r, width, width));
    }
    polyline(c, &geo.head);
    CGContext::close_path(Some(c));
    CGContext::fill_path(Some(c));
    true
}

fn draw_marker(c: &CGContext, points: &[Pt], width: f32, color: Rgb) -> bool {
    if points.is_empty() {
        return true;
    }
    if !(points.iter().all(|p| sane_pt(*p)) && sane(width)) || width <= 0.0 {
        return false;
    }
    let alpha = f32::from(MARKER_ALPHA) / 255.0;
    set_fill(c, color, alpha);
    // Mọi điểm trùng nhau: vẽ ô vuông như nét một điểm.
    if let Some(first) = single_spot(points) {
        let r = width / 2.0;
        CGContext::fill_rect(Some(c), cg_rect(first.x - r, first.y - r, width, width));
        return true;
    }
    // Nới đường thành viền rồi tô một lần (quy tắc khác không) nên chỗ nét tự cắt chỉ phủ một lần.
    CGContext::set_line_width(Some(c), f(width));
    CGContext::set_line_cap(Some(c), CGLineCap::Square);
    CGContext::set_line_join(Some(c), CGLineJoin::Round);
    polyline(c, points);
    CGContext::replace_path_with_stroked_path(Some(c));
    CGContext::fill_path(Some(c));
    true
}

/// Vùng che: tô đặc `blur_pixel_rect(a, b)` bằng `color`, alpha 1, không khử răng cưa.
fn draw_blur(c: &CGContext, a: Pt, b: Pt, color: Rgb) -> bool {
    if !(sane_pt(a) && sane_pt(b)) {
        return false;
    }
    let r = blur_pixel_rect(a, b);
    if r.w <= 0 || r.h <= 0 {
        return true;
    }
    CGContext::save_g_state(Some(c));
    CGContext::set_should_antialias(Some(c), false);
    set_fill(c, color, 1.0);
    CGContext::fill_rect(
        Some(c),
        cg_rect(r.x as f32, r.y as f32, r.w as f32, r.h as f32),
    );
    CGContext::restore_g_state(Some(c));
    true
}

// ---------------------------------------------------------------- Hộp có kiểu và khối chữ

/// Chạy `paint` trong một lớp trong suốt hợp nhất với độ trong suốt `opacity`: nền và viền vẽ đặc
/// rồi cả khối trộn với ảnh bên dưới một lần (chỗ viền đè nền không bị trộn hai lần).
fn with_opacity(c: &CGContext, opacity: f32, paint: impl FnOnce()) {
    CGContext::save_g_state(Some(c));
    CGContext::set_alpha(Some(c), f64::from(opacity.clamp(0.0, 1.0)));
    // SAFETY: không truyền thông tin phụ; lớp được đóng ngay sau đó trên cùng ngữ cảnh.
    unsafe { CGContext::begin_transparency_layer(Some(c), None) };
    paint();
    CGContext::end_transparency_layer(Some(c));
    CGContext::restore_g_state(Some(c));
}

/// Nền và viền của hình chữ nhật/elip `r` theo `style` (viền căn giữa trên cạnh `r`).
fn paint_box(c: &CGContext, r: CGRect, oval: bool, style: &Style) {
    if let Some(fill) = style.fill {
        set_fill(c, fill, 1.0);
        if oval {
            CGContext::fill_ellipse_in_rect(Some(c), r);
        } else {
            CGContext::fill_rect(Some(c), r);
        }
    }
    if style.stroke_width > 0.0 {
        set_stroke(c, style.stroke, 1.0);
        CGContext::set_line_width(Some(c), f(style.stroke_width));
        if oval {
            CGContext::stroke_ellipse_in_rect(Some(c), r);
        } else {
            CGContext::set_line_join(Some(c), CGLineJoin::Miter);
            CGContext::set_line_cap(Some(c), CGLineCap::Butt);
            CGContext::stroke_rect(Some(c), r);
        }
    }
}

fn draw_box(c: &CGContext, a: Pt, b: Pt, oval: bool, style: &Style) -> bool {
    let w = style.stroke_width;
    if !(sane_pt(a) && sane_pt(b) && sane(w)) || w < 0.0 || !style.opacity.is_finite() {
        return false;
    }
    if style.fill.is_none() && w <= 0.0 {
        return true;
    }
    let r = cg_rect(
        a.x.min(b.x),
        a.y.min(b.y),
        (b.x - a.x).abs(),
        (b.y - a.y).abs(),
    );
    with_opacity(c, style.opacity, || paint_box(c, r, oval, style));
    true
}

/// Số đo khối chữ: hộp (góc trên trái, rộng, cao), đệm, và từng dòng với x bắt đầu (tọa độ ảnh).
struct LabelMetrics {
    style: TextStyle,
    pad: f32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    /// (dòng, x bắt đầu chữ của dòng)
    lines: Vec<(String, f32)>,
}

fn label_pad(size: f32) -> f32 {
    LABEL_PAD_EM * size
}

fn label_metrics(origin: Pt, text: &str, size: f32, align: Align) -> Option<LabelMetrics> {
    if !sane_pt(origin) {
        return None;
    }
    let style = TextStyle::new(size)?;
    let pad = label_pad(size);
    let strs: Vec<&str> = text.split('\n').collect();
    let widths = strs
        .iter()
        .map(|l| advance_width(&style, l))
        .collect::<Option<Vec<f32>>>()?;
    let (inner_w, offs) = align_offsets(align, &widths);
    let h = strs.len() as f32 * style.line_height + 2.0 * pad;
    let lines = strs
        .iter()
        .zip(&offs)
        .map(|(l, o)| ((*l).to_string(), origin.x + pad + o))
        .collect();
    Some(LabelMetrics {
        style,
        pad,
        x: origin.x,
        y: origin.y,
        w: inner_w + 2.0 * pad,
        h,
        lines,
    })
}

#[allow(clippy::too_many_arguments)]
fn draw_label(
    c: &CGContext,
    origin: Pt,
    text: &str,
    size: f32,
    color: Rgb,
    align: Align,
    style: &Style,
) -> bool {
    if !sane(style.stroke_width) || style.stroke_width < 0.0 || !style.opacity.is_finite() {
        return false;
    }
    let Some(m) = label_metrics(origin, text, size, align) else {
        return false;
    };
    if style.fill.is_some() || style.stroke_width > 0.0 {
        // Viền nằm trọn trong hộp: thụt vào nửa độ dày.
        let inset = style.stroke_width / 2.0;
        let r = cg_rect(
            m.x + inset,
            m.y + inset,
            (m.w - style.stroke_width).max(0.0),
            (m.h - style.stroke_width).max(0.0),
        );
        with_opacity(c, style.opacity, || paint_box(c, r, false, style));
    }
    // Có nền thì chữ đủ rõ không cần nét ngoài; không nền giữ nét ngoài như chữ thường.
    let outline = style
        .fill
        .is_none()
        .then(|| (outline_color(color), outline_width(size)));
    for (i, (l, x)) in m.lines.iter().enumerate() {
        if l.is_empty() {
            continue;
        }
        let Some(line) = m.style.line(l) else {
            return false;
        };
        let baseline = m.y + m.pad + m.style.ascent + i as f32 * m.style.line_height;
        if !sane(baseline) {
            return false;
        }
        draw_line(c, &line, *x, baseline, color, outline);
    }
    true
}

fn draw_one(c: &CGContext, s: &Shape) -> bool {
    match s {
        Shape::Pen {
            points,
            width,
            color,
        } => {
            if points.is_empty() {
                return true;
            }
            if !(points.iter().all(|p| sane_pt(*p)) && sane(*width)) || *width <= 0.0 {
                return false;
            }
            if let Some(p) = single_spot(points) {
                let r = width / 2.0;
                set_fill(c, *color, 1.0);
                CGContext::fill_ellipse_in_rect(Some(c), cg_rect(p.x - r, p.y - r, *width, *width));
                return true;
            }
            set_stroke(c, *color, 1.0);
            CGContext::set_line_width(Some(c), f(*width));
            CGContext::set_line_cap(Some(c), CGLineCap::Round);
            CGContext::set_line_join(Some(c), CGLineJoin::Round);
            polyline(c, points);
            CGContext::stroke_path(Some(c));
            true
        }
        Shape::Line { a, b, width, color } => {
            if !(sane_pt(*a) && sane_pt(*b) && sane(*width)) || *width <= 0.0 {
                return false;
            }
            set_stroke(c, *color, 1.0);
            CGContext::set_line_width(Some(c), f(*width));
            CGContext::set_line_cap(Some(c), CGLineCap::Round);
            CGContext::set_line_join(Some(c), CGLineJoin::Round);
            polyline(c, &[*a, *b]);
            CGContext::stroke_path(Some(c));
            true
        }
        Shape::Arrow { a, b, width, color } => draw_arrow(c, *a, *b, *width, *color),
        Shape::Rect { a, b, width, color } => {
            if !(sane_pt(*a) && sane_pt(*b) && sane(*width)) || *width <= 0.0 {
                return false;
            }
            set_stroke(c, *color, 1.0);
            CGContext::set_line_join(Some(c), CGLineJoin::Miter);
            CGContext::set_line_cap(Some(c), CGLineCap::Butt);
            CGContext::stroke_rect_with_width(
                Some(c),
                cg_rect(
                    a.x.min(b.x),
                    a.y.min(b.y),
                    (b.x - a.x).abs(),
                    (b.y - a.y).abs(),
                ),
                f(*width),
            );
            true
        }
        Shape::Marker {
            points,
            width,
            color,
        } => draw_marker(c, points, *width, *color),
        Shape::Blur { a, b, color } => draw_blur(c, *a, *b, *color),
        Shape::Text {
            origin,
            text,
            size,
            color,
        } => draw_text(c, *origin, text, *size, *color),
        Shape::Ellipse { a, b, width, color } => {
            if !(sane_pt(*a) && sane_pt(*b) && sane(*width)) || *width <= 0.0 {
                return false;
            }
            set_stroke(c, *color, 1.0);
            CGContext::set_line_width(Some(c), f(*width));
            CGContext::stroke_ellipse_in_rect(
                Some(c),
                cg_rect(
                    a.x.min(b.x),
                    a.y.min(b.y),
                    (b.x - a.x).abs(),
                    (b.y - a.y).abs(),
                ),
            );
            true
        }
        Shape::Step {
            center,
            radius,
            n,
            color,
        } => draw_step(c, *center, *radius, *n, *color),
        Shape::Box { a, b, oval, style } => draw_box(c, *a, *b, *oval, style),
        Shape::Label {
            origin,
            text,
            size,
            color,
            align,
            style,
        } => draw_label(c, *origin, text, *size, *color, *align, style),
    }
}

/// Vẽ `shapes` lên `ctx` (không gian pixel, gốc trên trái): điểm ảnh (x, y) của ảnh chụp nằm ở
/// (x - ox, y - oy); chỉ vẽ trong `clip` (tọa độ ảnh chụp). Duyệt đúng thứ tự chỉ số tăng dần (chỉ
/// số 0 ở dưới cùng), không sắp xếp lại, không gom theo loại. `false` khi có hình không vẽ được
/// (các hình còn lại vẫn được vẽ).
pub fn draw_shapes(ctx: &CGContext, shapes: &[Shape], ox: i32, oy: i32, clip: Rect) -> bool {
    if shapes.is_empty() {
        return true;
    }
    CGContext::save_g_state(Some(ctx));
    CGContext::translate_ctm(Some(ctx), -f64::from(ox), -f64::from(oy));
    CGContext::clip_to_rect(
        Some(ctx),
        cg_rect(
            clip.x as f32,
            clip.y as f32,
            clip.w.max(0) as f32,
            clip.h.max(0) as f32,
        ),
    );
    CGContext::set_should_antialias(Some(ctx), true);
    let mut all = true;
    for s in shapes {
        // Mỗi hình bắt đầu từ trạng thái đồ họa sạch (nét, cap, join không rò sang hình sau).
        CGContext::save_g_state(Some(ctx));
        all &= draw_one(ctx, s);
        CGContext::restore_g_state(Some(ctx));
    }
    CGContext::restore_g_state(Some(ctx));
    all
}

// ---------------------------------------------------------------- đo

/// Làm tròn ra ngoài, nới 1 px khử răng cưa.
fn round_out(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
    let (fx, fy) = ((x0 - 1.0).floor() as i32, (y0 - 1.0).floor() as i32);
    let (cx, cy) = ((x1 + 1.0).ceil() as i32, (y1 + 1.0).ceil() as i32);
    Rect {
        x: fx,
        y: fy,
        w: cx.saturating_sub(fx),
        h: cy.saturating_sub(fy),
    }
}

const EMPTY: Rect = Rect {
    x: 0,
    y: 0,
    w: 0,
    h: 0,
};

/// Hộp bao (tọa độ ảnh chụp) của một hình; Text đo bằng CoreText cùng phông với lúc vẽ.
pub fn shape_bounds(s: &Shape) -> Rect {
    if let Some(r) = s.bounds() {
        return r;
    }
    if let Shape::Label {
        origin,
        text,
        size,
        align,
        style,
        ..
    } = s
    {
        return label_bounds(*origin, text, *size, *align, style);
    }
    let Shape::Text {
        origin, text, size, ..
    } = s
    else {
        // Pen/Marker không có điểm: không vẽ gì.
        return EMPTY;
    };
    text_bounds(*origin, text, *size).unwrap_or_else(|| {
        // Không đo được: ước lượng rộng tay (mỗi ký tự một em, dòng 1,5 em, cộng viền).
        let lines = text.split('\n').count().max(1) as f32;
        let cols = text
            .split('\n')
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0) as f32;
        let pad = TEXT_OUTLINE_DIP * size / TEXT_DIP;
        round_out(
            origin.x - pad,
            origin.y - pad,
            origin.x + cols * size + pad,
            origin.y + lines * size * 1.5 + pad,
        )
    })
}

/// Hộp bao của khối chữ: cả hộp (gồm đệm và viền), nới nét ngoài của chữ.
fn label_bounds(origin: Pt, text: &str, size: f32, align: Align, style: &Style) -> Rect {
    let margin = (outline_width(size) / 2.0).max(style.stroke_width.max(0.0) / 2.0);
    let (x, y, w, h) = match label_metrics(origin, text, size, align) {
        Some(m) => (m.x, m.y, m.w, m.h),
        None => {
            // Không đo được: ước lượng (mỗi ký tự 0,6 em, dòng 1,5 em).
            let lines = text.split('\n').count().max(1) as f32;
            let cols = text
                .split('\n')
                .map(|l| l.chars().count())
                .max()
                .unwrap_or(0) as f32;
            let pad = label_pad(size);
            (
                origin.x,
                origin.y,
                cols * size * 0.6 + 2.0 * pad,
                lines * size * 1.5 + 2.0 * pad,
            )
        }
    };
    round_out(x - margin, y - margin, x + w + margin, y + h + margin)
}

/// Vị trí và chiều cao con trỏ nháy của ô chữ có kiểu: x theo căn lề của dòng chứa con trỏ.
pub fn label_caret_rect(edit: &TextEdit, align: Align) -> Rect {
    let (origin, size) = (edit.origin(), edit.size());
    let (line, col) = edit.line_col();
    let text = edit.text();
    let prefix: String = text
        .split('\n')
        .nth(line)
        .unwrap_or("")
        .chars()
        .take(col)
        .collect();
    let pad = label_pad(size);
    let measured = label_metrics(origin, &text, size, align).and_then(|m| {
        let x = m.lines.get(line).map(|(_, x)| *x)?;
        let w = advance_width(&m.style, &prefix)?;
        Some((x + w, m.style.line_height))
    });
    let (cx, lh) = measured.unwrap_or((
        origin.x + pad + prefix.chars().count() as f32 * size * 0.5,
        size * 1.33,
    ));
    Rect {
        x: cx.floor() as i32,
        y: (origin.y + pad + line as f32 * lh).floor() as i32,
        w: ((size / TEXT_DIP).round() as i32).max(1),
        h: (lh.ceil() as i32).max(1),
    }
}

/// Hộp mực của mọi dòng cộng nửa nét ngoài, hợp với góc đặt chữ, làm tròn ra ngoài.
fn text_bounds(origin: Pt, text: &str, size: f32) -> Option<Rect> {
    if !sane_pt(origin) {
        return None;
    }
    let style = TextStyle::new(size)?;
    let pad = outline_width(size) / 2.0;
    let mut acc: Option<(f32, f32, f32, f32)> = None;
    for (i, l) in text.split('\n').enumerate() {
        if l.is_empty() {
            continue;
        }
        let line = style.line(l)?;
        let Some((x, y, w, h)) = ink_box(&line) else {
            continue;
        };
        let baseline = origin.y + style.ascent + i as f32 * style.line_height;
        let (x0, x1) = (origin.x + x, origin.x + x + w);
        let (y0, y1) = (baseline - (y + h), baseline - y);
        acc = Some(match acc {
            None => (x0, y0, x1, y1),
            Some((a, b, c, d)) => (a.min(x0), b.min(y0), c.max(x1), d.max(y1)),
        });
    }
    let (x0, y0, x1, y1) = acc?;
    // Gồm cả góc đầu dòng đầu để hộp luôn chứa vị trí đặt chữ.
    let r = round_out(
        x0.min(origin.x) - pad,
        y0.min(origin.y) - pad,
        x1 + pad,
        y1 + pad,
    );
    (r.x.abs() < i32::MAX / 2 && r.w >= 0 && r.h >= 0).then_some(r)
}

/// Vị trí và chiều cao con trỏ nháy (tọa độ ảnh chụp) của ô chữ thường (chưa có kiểu); ô chữ có
/// kiểu dùng `label_caret_rect`.
#[cfg(test)]
pub fn caret_rect(edit: &TextEdit) -> Rect {
    // TextEdit không cho đọc gốc và cỡ chữ: lấy qua bản sao có thêm một ký tự (to_shape luôn Some).
    let mut probe = edit.clone();
    probe.insert("x");
    let Some(Shape::Text { origin, size, .. }) = probe.to_shape() else {
        return EMPTY;
    };
    let (line, col) = edit.line_col();
    let text = edit.text();
    let prefix: String = text
        .split('\n')
        .nth(line)
        .unwrap_or("")
        .chars()
        .take(col)
        .collect();
    let measured = TextStyle::new(size).and_then(|style| {
        let w = advance_width(&style, &prefix)?;
        Some((w, style.line_height))
    });
    // Không đo được: ước lượng nửa em mỗi ký tự, dòng 1,33 em.
    let (dx, lh) = measured.unwrap_or((prefix.chars().count() as f32 * size * 0.5, size * 1.33));
    Rect {
        x: (origin.x + dx).floor() as i32,
        y: (origin.y + line as f32 * lh).floor() as i32,
        w: ((size / TEXT_DIP).round() as i32).max(1),
        h: (lh.ceil() as i32).max(1),
    }
}

// ---------------------------------------------------------------- đốt vào ảnh

/// Dựng `CGBitmapContext` (BGRA premultiplied-first little-endian, sRGB, đã lật để gốc ở trên trái)
/// trực tiếp trên bộ đệm của `img`, gọi `paint`, rồi đặt alpha 255. Ngữ cảnh được hủy trước khi trả
/// về nên không còn giữ con trỏ vào bộ đệm. `img` không đổi khi lỗi trước khi vẽ.
fn with_bitmap(img: &mut Image, paint: impl FnOnce(&CGContext) -> bool) -> Result<(), String> {
    if !img.is_valid() {
        return Err("invalid image".into());
    }
    let (w, h) = (img.width as usize, img.height as usize);
    let row = w
        .checked_mul(4)
        .ok_or_else(|| "image too large".to_string())?;
    // SAFETY: hằng của Core Graphics sống suốt tiến trình.
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))
        .ok_or_else(|| "no sRGB color space".to_string())?;
    let info = CGBitmapInfo(
        CGImageAlphaInfo::PremultipliedFirst.0 | CGImageByteOrderInfo::Order32Little.0,
    );
    let ptr = img.bgra.as_mut_ptr().cast::<c_void>();
    // SAFETY: `is_valid` bảo đảm bộ đệm có đúng w*h*4 byte; ngữ cảnh chỉ ghi trong [0, h) hàng với
    // `row` byte mỗi hàng, và được hủy (drop) trước khi `img` được dùng lại; `img` được mượn độc
    // quyền suốt thời gian đó.
    let ctx = unsafe { CGBitmapContextCreate(ptr, w, h, 8, row, Some(&space), info.0) }
        .ok_or_else(|| "could not create a bitmap context".to_string())?;
    // Lật trục: gốc trên trái, y hướng xuống.
    CGContext::translate_ctm(Some(&ctx), 0.0, h as f64);
    CGContext::scale_ctm(Some(&ctx), 1.0, -1.0);
    let ok = paint(&ctx);
    CGContext::flush(Some(&ctx));
    drop(ctx);
    for p in img.bgra.as_chunks_mut::<4>().0 {
        p[3] = 255;
    }
    if ok {
        Ok(())
    } else {
        Err("could not draw the markings".into())
    }
}

/// In `shapes` vào ảnh đã cắt từ vùng `crop` (tọa độ ảnh chụp). Ảnh giữ alpha 255. Rỗng thì không
/// làm gì. Dùng đúng `draw_shapes` nên thứ tự lớp giống trên màn hình.
pub fn burn(img: &mut Image, shapes: &[Shape], crop: Rect) -> Result<(), String> {
    if shapes.is_empty() {
        return Ok(());
    }
    with_bitmap(img, |ctx| draw_shapes(ctx, shapes, crop.x, crop.y, crop))
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkshot_core::annotate::{blur_pixel_rect, outline_color, Pt, TEXT_DIP};
    use darkshot_core::theme::Rgb;

    const RED: Rgb = Rgb(255, 0, 0);
    const BLUE: Rgb = Rgb(0, 0, 255);
    const YELLOW: Rgb = Rgb(255, 255, 0);

    fn pt(x: f32, y: f32) -> Pt {
        Pt { x, y }
    }

    fn solid(w: u32, h: u32, v: u8) -> Image {
        let mut bgra = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..w * h {
            bgra.extend_from_slice(&[v, v, v, 255]);
        }
        Image {
            width: w,
            height: h,
            bgra,
        }
    }

    fn noise(w: u32, h: u32, seed: u32) -> Image {
        let mut s = seed;
        let mut bgra = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..w * h {
            for _ in 0..3 {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                bgra.push((s >> 24) as u8);
            }
            bgra.push(255);
        }
        Image {
            width: w,
            height: h,
            bgra,
        }
    }

    /// [B, G, R, A] tại (x, y).
    fn px(img: &Image, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * img.width + x) * 4) as usize;
        [
            img.bgra[i],
            img.bgra[i + 1],
            img.bgra[i + 2],
            img.bgra[i + 3],
        ]
    }

    fn bgr(c: Rgb) -> [u8; 4] {
        [c.2, c.1, c.0, 255]
    }

    fn near(a: [u8; 4], b: [u8; 4], tol: u8) -> bool {
        a.iter().zip(b.iter()).all(|(x, y)| x.abs_diff(*y) <= tol)
    }

    fn full(img: &Image) -> Rect {
        Rect {
            x: 0,
            y: 0,
            w: img.width as i32,
            h: img.height as i32,
        }
    }

    fn all_opaque(img: &Image) -> bool {
        img.bgra.as_chunks::<4>().0.iter().all(|p| p[3] == 255)
    }

    /// Vẽ `shapes` lên ảnh nền xám `bg` cỡ w×h (crop = cả ảnh) và trả ảnh.
    fn render(w: u32, h: u32, bg: u8, shapes: &[Shape]) -> Image {
        let mut img = solid(w, h, bg);
        let crop = full(&img);
        assert_eq!(burn(&mut img, shapes, crop), Ok(()));
        assert!(all_opaque(&img));
        img
    }

    fn blur_at(x: f32, y: f32, w: f32, h: f32, c: Rgb) -> Shape {
        Shape::Blur {
            a: pt(x, y),
            b: pt(x + w, y + h),
            color: c,
        }
    }

    fn text(origin: Pt, t: &str, size: f32, color: Rgb) -> Shape {
        Shape::Text {
            origin,
            text: t.into(),
            size,
            color,
        }
    }

    #[test]
    fn empty_burn_is_identity() {
        let mut img = noise(64, 40, 7);
        let before = img.clone();
        let crop = Rect {
            x: 13,
            y: 9,
            w: 64,
            h: 40,
        };
        assert_eq!(burn(&mut img, &[], crop), Ok(()));
        assert_eq!(img, before);
    }

    #[test]
    fn rect_outline_not_filled() {
        let r = Shape::Rect {
            a: pt(50.0, 40.0),
            b: pt(10.0, 10.0),
            width: 4.0,
            color: RED,
        };
        let img = render(60, 50, 0, &[r]);
        assert_eq!(px(&img, 30, 25), [0, 0, 0, 255], "tâm là nền");
        assert_eq!(px(&img, 10, 25), bgr(RED), "cạnh trái");
        assert_eq!(px(&img, 30, 10), bgr(RED), "cạnh trên");
        assert_eq!(px(&img, 49, 25), bgr(RED), "cạnh phải");
        assert_eq!(px(&img, 30, 5), [0, 0, 0, 255], "ngoài khung");
    }

    #[test]
    fn line_and_pen() {
        let line = Shape::Line {
            a: pt(5.0, 10.0),
            b: pt(35.0, 10.0),
            width: 4.0,
            color: RED,
        };
        let img = render(40, 20, 0, &[line]);
        assert_eq!(px(&img, 20, 10), bgr(RED));
        assert_eq!(px(&img, 20, 15), [0, 0, 0, 255]);
        // Đầu tròn: nhô ra ngoài hai đầu nửa độ dày.
        assert!(px(&img, 4, 10)[2] > 128, "đầu tròn trái");

        let pen = Shape::Pen {
            points: vec![pt(5.0, 5.0), pt(5.0, 35.0), pt(35.0, 35.0)],
            width: 4.0,
            color: BLUE,
        };
        let img = render(40, 40, 0, &[pen]);
        assert!(near(px(&img, 5, 20), bgr(BLUE), 2));
        assert!(near(px(&img, 20, 35), bgr(BLUE), 2));
        assert_eq!(px(&img, 20, 20), [0, 0, 0, 255]);

        // Mọi điểm trùng: một chấm tròn; rỗng điểm: không vẽ gì.
        let dot = Shape::Pen {
            points: vec![pt(20.0, 20.0); 3],
            width: 10.0,
            color: RED,
        };
        let img = render(40, 40, 0, &[dot]);
        assert_eq!(px(&img, 20, 20), bgr(RED));
        assert_eq!(px(&img, 20, 30), [0, 0, 0, 255]);
        let none = Shape::Pen {
            points: vec![],
            width: 10.0,
            color: RED,
        };
        let marker_none = Shape::Marker {
            points: vec![],
            width: 10.0,
            color: RED,
        };
        let img = render(20, 20, 7, &[none, marker_none]);
        assert_eq!(img, solid(20, 20, 7));
    }

    #[test]
    fn arrow_head_wider_than_shaft() {
        let a = Shape::Arrow {
            a: pt(10.0, 30.0),
            b: pt(90.0, 30.0),
            width: 4.0,
            color: RED,
        };
        let img = render(100, 60, 0, &[a]);
        assert_eq!(px(&img, 40, 30), bgr(RED), "thân");
        assert_eq!(px(&img, 40, 34), [0, 0, 0, 255], "ngoài thân");
        // Đầu: dài max(4w, 12 * w / 3) = 16, rộng ±16*sin28 ≈ 7,5 ở gốc đầu.
        assert!(
            px(&img, 77, 35)[2] > 200,
            "đầu rộng hơn thân: {:?}",
            px(&img, 77, 35)
        );
        assert!(px(&img, 77, 25)[2] > 200);
        assert_eq!(px(&img, 95, 30), [0, 0, 0, 255], "sau mũi tên");
    }

    #[test]
    fn ellipse_outline() {
        let e = Shape::Ellipse {
            a: pt(10.0, 10.0),
            b: pt(70.0, 50.0),
            width: 4.0,
            color: RED,
        };
        let img = render(80, 60, 0, &[e]);
        assert!(near(px(&img, 10, 30), bgr(RED), 2), "mép trái");
        assert!(near(px(&img, 40, 10), bgr(RED), 2), "mép trên");
        assert!(near(px(&img, 69, 30), bgr(RED), 2), "mép phải");
        assert_eq!(px(&img, 40, 30), [0, 0, 0, 255], "tâm rỗng");
        assert_eq!(px(&img, 12, 12), [0, 0, 0, 255], "góc ngoài elip");
    }

    #[test]
    fn marker_is_translucent_and_single_coverage() {
        let m = Shape::Marker {
            points: vec![pt(10.0, 20.0), pt(50.0, 20.0)],
            width: 10.0,
            color: RED,
        };
        let img = render(60, 40, 100, &[m]);
        // 0,4 * 255 + 0,6 * 100 = 162; 0,6 * 100 = 60.
        let p = px(&img, 30, 20);
        assert!(near(p, [60, 60, 162, 255], 3), "{p:?}");
        assert_eq!(px(&img, 30, 30), [100, 100, 100, 255]);
        // Đầu vuông: nhô ra nửa độ dày.
        assert!(near(px(&img, 6, 20), [60, 60, 162, 255], 3));
        // Nét tự cắt chỉ phủ một lần (không đậm hơn chỗ khác).
        let x = Shape::Marker {
            points: vec![
                pt(10.0, 10.0),
                pt(50.0, 30.0),
                pt(50.0, 10.0),
                pt(10.0, 30.0),
            ],
            width: 10.0,
            color: RED,
        };
        let img = render(60, 40, 100, &[x]);
        let cross = px(&img, 30, 20);
        assert!(
            near(cross, [60, 60, 162, 255], 3),
            "giao điểm không đậm hơn: {cross:?}"
        );
    }

    #[test]
    fn blur_covers_exact_pixel_rect() {
        let b = Shape::Blur {
            a: pt(10.2, 10.7),
            b: pt(30.5, 20.1),
            color: BLUE,
        };
        let r = blur_pixel_rect(pt(10.2, 10.7), pt(30.5, 20.1));
        assert_eq!((r.x, r.y, r.w, r.h), (10, 10, 21, 11));
        let img = render(50, 40, 0, &[b]);
        for (x, y, inside) in [
            (10, 10, true),
            (30, 20, true),
            (20, 15, true),
            (9, 15, false),
            (31, 15, false),
            (20, 9, false),
            (20, 21, false),
        ] {
            let want = if inside { bgr(BLUE) } else { [0, 0, 0, 255] };
            assert_eq!(px(&img, x, y), want, "({x},{y})");
        }
    }

    #[test]
    fn step_has_disc_ring_and_number() {
        let s = Shape::Step {
            center: pt(40.0, 40.0),
            radius: 20.0,
            n: 1,
            color: RED,
        };
        let img = render(80, 80, 255, &[s]);
        assert_eq!(px(&img, 40 + 12, 40), bgr(RED), "lòng màu");
        let ink = outline_color(RED);
        assert!(
            near(px(&img, 40 + 18, 40), [ink.2, ink.1, ink.0, 255], 2),
            "viền {:?} {:?} {:?}",
            px(&img, 57, 40),
            px(&img, 58, 40),
            px(&img, 59, 40)
        );
        assert_eq!(px(&img, 40 + 23, 40), [255, 255, 255, 255], "ngoài vòng");
        // Có số: ít nhất một điểm trong lòng gần màu mực (đen).
        let mut inked = 0;
        for y in 30..50 {
            for x in 32..48 {
                if near(px(&img, x, y), [0, 0, 0, 255], 40) {
                    inked += 1;
                }
            }
        }
        assert!(inked >= 10, "số 1 phải có nét: {inked}");
        // Số nằm giữa vòng: tâm hộp các điểm mực (đen) lệch tâm vòng không quá 2 px.
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
        for y in 28..53 {
            for x in 28..53 {
                if near(px(&img, x, y), [0, 0, 0, 255], 40) {
                    (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                }
            }
        }
        let (cx, cy) = (f64::from(x0 + x1 + 1) / 2.0, f64::from(y0 + y1 + 1) / 2.0);
        assert!(
            (cx - 40.0).abs() <= 2.0 && (cy - 40.0).abs() <= 2.0,
            "tâm số ({cx},{cy})"
        );
        // n = 0: không số.
        let s0 = Shape::Step {
            center: pt(40.0, 40.0),
            radius: 20.0,
            n: 0,
            color: RED,
        };
        let img0 = render(80, 80, 255, &[s0]);
        assert_eq!(px(&img0, 40, 40), bgr(RED));
        // Số nhiều chữ số không tràn vòng: cỡ chữ thu lại.
        let s99 = Shape::Step {
            center: pt(40.0, 40.0),
            radius: 20.0,
            n: 9999,
            color: RED,
        };
        let img = render(80, 80, 255, &[s99]);
        assert_eq!(px(&img, 40 + 23, 40), [255, 255, 255, 255]);
    }

    #[test]
    fn text_bounds_grow_with_content() {
        let o = pt(10.0, 10.0);
        let b = |t: &str, size: f32| shape_bounds(&text(o, t, size, YELLOW));
        let short = b("ab", 24.0);
        let long = b("abcdefgh", 24.0);
        assert!(long.w > short.w, "{short:?} {long:?}");
        let small = b("abcd", 16.0);
        let big = b("abcd", 48.0);
        assert!(big.w > small.w && big.h > small.h);
        let two = b("ab\ncd", 24.0);
        assert!(two.h > short.h, "nhiều dòng cao hơn: {short:?} {two:?}");
        assert!(short.w > 0 && short.h > 0);
        // Hộp chứa gốc đặt chữ.
        assert!(short.x <= 10 && short.y <= 10);
        // Hình khác Text: dùng Shape::bounds của core.
        let l = Shape::Line {
            a: pt(0.0, 0.0),
            b: pt(10.0, 0.0),
            width: 2.0,
            color: RED,
        };
        assert_eq!(Some(shape_bounds(&l)), l.bounds());
        // Danh sách điểm rỗng: hộp rỗng.
        let e = Shape::Pen {
            points: vec![],
            width: 2.0,
            color: RED,
        };
        assert_eq!(shape_bounds(&e).w, 0);
    }

    #[test]
    fn text_draws_fill_and_outline_inside_its_bounds() {
        let t = text(pt(20.0, 20.0), "HI", 80.0, YELLOW);
        let bounds = shape_bounds(&t);
        let img = render(240, 160, 128, &[t]);
        let (mut yellow, mut black, mut outside_changed) = (0, 0, 0);
        for y in 0..img.height {
            for x in 0..img.width {
                let p = px(&img, x, y);
                if p == bgr(YELLOW) {
                    yellow += 1;
                }
                if near(p, [0, 0, 0, 255], 8) {
                    black += 1;
                }
                let inside = (x as i32) >= bounds.x
                    && (x as i32) < bounds.x + bounds.w
                    && (y as i32) >= bounds.y
                    && (y as i32) < bounds.y + bounds.h;
                if !inside && p != [128, 128, 128, 255] {
                    outside_changed += 1;
                }
            }
        }
        assert!(yellow > 200, "ruột chữ màu: {yellow}");
        assert!(black > 100, "viền ngoài khác màu chữ: {black}");
        assert_eq!(
            outside_changed, 0,
            "mọi điểm đã vẽ nằm trong hộp bao {bounds:?}"
        );
        // Viền dày 2 × TEXT_OUTLINE_DIP × size / TEXT_DIP / 2 ở mỗi phía ngoài.
        let _ = TEXT_DIP;
    }

    #[test]
    fn text_unicode_empty_and_long_do_not_panic() {
        let o = pt(5.0, 5.0);
        for s in [
            "",
            "\n\n",
            "   ",
            "Việt Nam ạ ơ ư",
            "😀 emoji 👍🏽",
            "漢字かな",
            &"w".repeat(3000),
        ] {
            let t = text(o, s, 20.0, RED);
            let b = shape_bounds(&t);
            assert!(b.w >= 0 && b.h >= 0, "{s:?}: {b:?}");
            let _ = render(60, 40, 0, &[t]);
        }
        let blank = render(30, 30, 9, &[text(o, "", 20.0, RED)]);
        assert_eq!(blank, solid(30, 30, 9), "chuỗi rỗng không vẽ gì");
        let vi = shape_bounds(&text(o, "Việt", 24.0, RED));
        assert!(vi.w > 20 && vi.h > 10);
    }

    #[test]
    fn caret_follows_caret_position() {
        let o = pt(10.0, 20.0);
        let mut e = TextEdit::new(o, 20.0, YELLOW);
        let c0 = caret_rect(&e);
        assert_eq!((c0.x, c0.y), (10, 20));
        assert_eq!(c0.w, ((20.0f32 / TEXT_DIP).round() as i32).max(1));
        assert!(c0.h >= 20, "cao ít nhất một em: {c0:?}");
        e.insert("ab");
        let c2 = caret_rect(&e);
        e.insert("c");
        let c3 = caret_rect(&e);
        assert!(c2.x > c0.x && c3.x > c2.x, "{c0:?} {c2:?} {c3:?}");
        assert_eq!(c2.y, 20);
        e.insert("\nx");
        let c4 = caret_rect(&e);
        assert!(c4.y >= 20 + c0.h - 1, "xuống dòng: {c4:?}");
        assert!(c4.x > c0.x && c4.x < c3.x);
        // Con trỏ ở giữa chuỗi.
        e.left();
        e.left();
        let c5 = caret_rect(&e);
        assert_eq!(c5.x, c3.x, "đầu dòng 2 ở 'x' ... (đầu dòng 1 sau 'c')");
    }

    // ------------------------------------------------------------ z-order (yêu cầu của chủ dự án)

    #[test]
    fn z_order_later_rect_is_in_front() {
        let red = Shape::Rect {
            a: pt(10.0, 10.0),
            b: pt(50.0, 50.0),
            width: 10.0,
            color: RED,
        };
        let blue = Shape::Rect {
            a: pt(10.0, 10.0),
            b: pt(50.0, 50.0),
            width: 10.0,
            color: BLUE,
        };
        let img = render(60, 60, 0, &[red.clone(), blue.clone()]);
        assert_eq!(px(&img, 10, 30), bgr(BLUE), "thêm sau nằm trước");
        let img = render(60, 60, 0, &[blue, red]);
        assert_eq!(px(&img, 10, 30), bgr(RED), "đảo thứ tự thì đảo kết quả");
    }

    /// Mỗi loại hình phủ đặc điểm `probe` bằng màu `color`.
    fn covering(kind: usize, color: Rgb) -> (Shape, (u32, u32)) {
        match kind {
            0 => (
                Shape::Pen {
                    points: vec![pt(10.0, 40.0), pt(90.0, 40.0)],
                    width: 20.0,
                    color,
                },
                (50, 40),
            ),
            1 => (
                Shape::Line {
                    a: pt(10.0, 40.0),
                    b: pt(90.0, 40.0),
                    width: 20.0,
                    color,
                },
                (50, 40),
            ),
            2 => (
                Shape::Arrow {
                    a: pt(10.0, 40.0),
                    b: pt(100.0, 40.0),
                    width: 20.0,
                    color,
                },
                (50, 40),
            ),
            3 => (
                Shape::Rect {
                    a: pt(10.0, 10.0),
                    b: pt(90.0, 70.0),
                    width: 20.0,
                    color,
                },
                (10, 40),
            ),
            4 => (
                Shape::Ellipse {
                    a: pt(10.0, 10.0),
                    b: pt(90.0, 70.0),
                    width: 20.0,
                    color,
                },
                (10, 40),
            ),
            5 => (
                Shape::Step {
                    center: pt(50.0, 40.0),
                    radius: 30.0,
                    n: 0,
                    color,
                },
                (50, 40),
            ),
            _ => (blur_at(30.0, 20.0, 40.0, 40.0, color), (50, 40)),
        }
    }

    #[test]
    fn z_order_every_kind_against_blur() {
        for kind in 0..7 {
            let (shape, (x, y)) = covering(kind, RED);
            // Blur xanh phủ đặc điểm dò.
            let cover = blur_at(x as f32 - 4.0, y as f32 - 4.0, 8.0, 8.0, BLUE);
            let img = render(100, 80, 200, &[shape.clone(), cover.clone()]);
            assert_eq!(
                px(&img, x, y),
                bgr(BLUE),
                "loại {kind}: Blur sau che hình trước"
            );
            let img = render(100, 80, 200, &[cover, shape]);
            assert!(
                near(px(&img, x, y), bgr(RED), 2),
                "loại {kind}: hình sau nằm trước Blur"
            );
        }
    }

    #[test]
    fn z_order_marker_blends_only_with_what_is_below() {
        let blue = blue_block();
        let marker = Shape::Marker {
            points: vec![pt(10.0, 20.0), pt(50.0, 20.0)],
            width: 10.0,
            color: RED,
        };
        // Marker sau: pha với xanh bên dưới (không còn là xanh thuần, không thành đỏ thuần).
        let img = render(60, 40, 100, &[blue.clone(), marker.clone()]);
        let p = px(&img, 30, 20);
        // 0,4 * (0,0,255) đỏ + 0,6 * xanh: B = 0,6 * 255 = 153, R = 102.
        assert!(near(p, [153, 0, 102, 255], 3), "{p:?}");
        // Marker trước, khối xanh sau: xanh thuần che hết.
        let img = render(60, 40, 100, &[marker, blue]);
        assert_eq!(px(&img, 30, 20), bgr(BLUE));
    }

    fn blue_block() -> Shape {
        blur_at(5.0, 10.0, 50.0, 20.0, BLUE)
    }

    #[test]
    fn z_order_text_vs_shapes_by_index() {
        // Tìm một điểm ruột chữ.
        let t = text(pt(20.0, 20.0), "H", 100.0, YELLOW);
        let alone = render(200, 200, 128, std::slice::from_ref(&t));
        let (mut found, mut cnt) = (None, 0);
        'f: for y in 0..alone.height {
            for x in 0..alone.width {
                if px(&alone, x, y) == bgr(YELLOW) {
                    // Cần cả lân cận trong ruột (ổn định trước cạnh khung/khối che).
                    cnt += 1;
                    if cnt > 400 {
                        found = Some((x, y));
                        break 'f;
                    }
                }
            }
        }
        let (px_, py_) = found.expect("phải có điểm ruột chữ");
        let (fx, fy) = (px_ as f32, py_ as f32);
        // Khối đặc (Blur) trước chữ: chữ nằm trên. Sau chữ: bị che.
        let cover = blur_at(fx - 3.0, fy - 3.0, 6.0, 6.0, BLUE);
        let img = render(200, 200, 128, &[cover.clone(), t.clone()]);
        assert_eq!(
            px(&img, px_, py_),
            bgr(YELLOW),
            "chữ thêm sau nằm trên khối"
        );
        let img = render(200, 200, 128, &[t.clone(), cover]);
        assert_eq!(px(&img, px_, py_), bgr(BLUE), "chữ thêm trước bị khối che");
        // Khung (Rect) dày đi qua điểm đó.
        let frame = Shape::Rect {
            a: pt(fx, fy - 50.0),
            b: pt(fx + 80.0, fy + 50.0),
            width: 20.0,
            color: RED,
        };
        let img = render(200, 200, 128, &[frame.clone(), t.clone()]);
        assert_eq!(px(&img, px_, py_), bgr(YELLOW), "chữ sau nằm trên khung");
        let img = render(200, 200, 128, &[t, frame]);
        assert_eq!(px(&img, px_, py_), bgr(RED), "khung sau nằm trên chữ");
    }

    #[test]
    fn z_order_follows_index_not_kind() {
        // Rect, Text, Rect: Rect thứ hai nằm trên cả chữ lẫn Rect đầu; thứ tự không gom theo loại.
        let r1 = Shape::Rect {
            a: pt(10.0, 10.0),
            b: pt(90.0, 70.0),
            width: 20.0,
            color: RED,
        };
        let r2 = Shape::Rect {
            a: pt(10.0, 10.0),
            b: pt(90.0, 70.0),
            width: 20.0,
            color: BLUE,
        };
        let t = text(pt(12.0, 30.0), "HHH", 40.0, YELLOW);
        let img = render(100, 80, 0, &[r1, t, r2]);
        assert_eq!(px(&img, 10, 40), bgr(BLUE));
        assert_eq!(px(&img, 89, 40), bgr(BLUE));
    }

    // ------------------------------------------------------------ burn

    #[test]
    fn burn_offsets_clips_and_stays_opaque() {
        // Ảnh đã cắt từ vùng (100, 50): hình ở tọa độ ảnh chụp.
        let mut img = solid(40, 30, 0);
        let crop = Rect {
            x: 100,
            y: 50,
            w: 40,
            h: 30,
        };
        let r = Shape::Rect {
            a: pt(110.0, 60.0),
            b: pt(130.0, 75.0),
            width: 4.0,
            color: RED,
        };
        assert_eq!(burn(&mut img, &[r], crop), Ok(()));
        assert_eq!(px(&img, 10, 12), bgr(RED), "cạnh trái ở (110-100, ..)");
        assert_eq!(px(&img, 5, 12), [0, 0, 0, 255]);
        assert!(all_opaque(&img));
        // Hình tràn mép ảnh và ngoài ảnh.
        let mut img = solid(40, 30, 0);
        let crop = full(&img);
        let l = Shape::Line {
            a: pt(-50.0, 15.0),
            b: pt(500.0, 15.0),
            width: 4.0,
            color: RED,
        };
        let far = Shape::Rect {
            a: pt(1000.0, 1000.0),
            b: pt(2000.0, 2000.0),
            width: 4.0,
            color: BLUE,
        };
        assert_eq!(burn(&mut img, &[l, far], crop), Ok(()));
        assert_eq!(px(&img, 0, 15), bgr(RED));
        assert_eq!(px(&img, 39, 15), bgr(RED));
        assert_eq!(px(&img, 20, 5), [0, 0, 0, 255]);
        // Vùng cắt nhỏ hơn ảnh: chỉ chạm trong vùng cắt.
        let mut img = solid(60, 40, 0);
        let crop = Rect {
            x: 0,
            y: 0,
            w: 20,
            h: 20,
        };
        let b = blur_at(0.0, 0.0, 60.0, 40.0, BLUE);
        assert_eq!(burn(&mut img, &[b], crop), Ok(()));
        assert_eq!(px(&img, 10, 10), bgr(BLUE));
        assert_eq!(px(&img, 30, 10), [0, 0, 0, 255], "ngoài vùng cắt");
        assert_eq!(px(&img, 10, 30), [0, 0, 0, 255]);
    }

    #[test]
    fn burn_rejects_invalid_image_and_survives_hostile_numbers() {
        let mut bad = Image {
            width: 4,
            height: 4,
            bgra: vec![0; 3],
        };
        let s = Shape::Line {
            a: pt(0.0, 0.0),
            b: pt(3.0, 3.0),
            width: 2.0,
            color: RED,
        };
        assert!(burn(
            &mut bad,
            &[s],
            Rect {
                x: 0,
                y: 0,
                w: 4,
                h: 4
            }
        )
        .is_err());
        assert_eq!(bad.bgra, vec![0; 3], "ảnh không đổi khi lỗi");

        let nan = f32::NAN;
        let inf = f32::INFINITY;
        let hostile = vec![
            Shape::Line {
                a: pt(nan, 0.0),
                b: pt(5.0, 5.0),
                width: 2.0,
                color: RED,
            },
            Shape::Line {
                a: pt(0.0, 0.0),
                b: pt(inf, 5.0),
                width: 2.0,
                color: RED,
            },
            Shape::Pen {
                points: vec![pt(1e30, 1e30), pt(-1e30, 3.0)],
                width: 4.0,
                color: RED,
            },
            Shape::Rect {
                a: pt(0.0, 0.0),
                b: pt(5.0, 5.0),
                width: nan,
                color: RED,
            },
            Shape::Ellipse {
                a: pt(-inf, 0.0),
                b: pt(5.0, inf),
                width: 2.0,
                color: RED,
            },
            Shape::Arrow {
                a: pt(nan, nan),
                b: pt(nan, nan),
                width: 2.0,
                color: RED,
            },
            Shape::Arrow {
                a: pt(3.0, 3.0),
                b: pt(3.0, 3.0),
                width: 2.0,
                color: RED,
            },
            Shape::Marker {
                points: vec![pt(inf, inf)],
                width: 4.0,
                color: RED,
            },
            Shape::Blur {
                a: pt(nan, 1.0),
                b: pt(inf, 1e30),
                color: RED,
            },
            text(pt(nan, 1.0), "x", 20.0, RED),
            text(pt(1.0, 1.0), "x", f32::NAN, RED),
            text(pt(1.0, 1.0), "x", -5.0, RED),
            text(pt(1.0, 1.0), "x", 1e30, RED),
            Shape::Step {
                center: pt(nan, 1.0),
                radius: 5.0,
                n: 3,
                color: RED,
            },
            Shape::Step {
                center: pt(5.0, 5.0),
                radius: -3.0,
                n: 3,
                color: RED,
            },
            Shape::Step {
                center: pt(5.0, 5.0),
                radius: inf,
                n: 3,
                color: RED,
            },
        ];
        let mut img = solid(20, 20, 5);
        let crop = full(&img);
        let _ = burn(&mut img, &hostile, crop);
        assert!(all_opaque(&img));
        for s in &hostile {
            let _ = shape_bounds(s);
        }
    }

    #[test]
    fn burn_is_draw_shapes_in_list_order() {
        // `burn` và `draw_shapes` trên cùng ngữ cảnh cho cùng ảnh, nên cùng thứ tự lớp.
        let shapes = vec![
            blur_at(5.0, 5.0, 40.0, 30.0, BLUE),
            Shape::Marker {
                points: vec![pt(10.0, 20.0), pt(40.0, 20.0)],
                width: 12.0,
                color: RED,
            },
            text(pt(8.0, 8.0), "Ab", 24.0, YELLOW),
        ];
        let a = render(60, 50, 90, &shapes);
        let mut b = solid(60, 50, 90);
        let crop = full(&b);
        assert!(with_bitmap(&mut b, |c| draw_shapes(c, &shapes, 0, 0, crop)).is_ok());
        assert_eq!(a, b);
        // Đảo danh sách thì kết quả khác.
        let mut rev = shapes.clone();
        rev.reverse();
        assert_ne!(render(60, 50, 90, &rev), a);
    }

    // ---- Task 112: kiểu cho Hình và Chữ

    const GREEN: Rgb = Rgb(0, 200, 0);

    fn styled(oval: bool, fill: Option<Rgb>, width: f32, opacity: f32) -> Shape {
        Shape::Box {
            a: pt(10.0, 10.0),
            b: pt(50.0, 40.0),
            oval,
            style: Style {
                fill,
                stroke: RED,
                stroke_width: width,
                opacity,
            },
        }
    }

    #[test]
    fn box_border_has_the_right_color_and_thickness() {
        let img = render(60, 50, 0, &[styled(false, Some(GREEN), 4.0, 1.0)]);
        assert_eq!(px(&img, 7, 25), [0, 0, 0, 255], "ngoài viền");
        assert!(near(px(&img, 8, 25), bgr(RED), 3), "mép ngoài viền");
        assert!(near(px(&img, 11, 25), bgr(RED), 3), "mép trong viền");
        assert!(near(px(&img, 13, 25), bgr(GREEN), 3), "nền ngay sau viền");
        assert!(near(px(&img, 30, 25), bgr(GREEN), 3), "tâm là nền");
        assert!(!near(px(&img, 12, 25), bgr(RED), 30), "hết viền ở x = 12");
    }

    #[test]
    fn box_without_fill_is_hollow_and_zero_width_hides_the_border() {
        let img = render(60, 50, 40, &[styled(false, None, 4.0, 1.0)]);
        assert_eq!(px(&img, 30, 25), [40, 40, 40, 255]);
        assert!(near(px(&img, 10, 25), bgr(RED), 3));
        let img = render(60, 50, 40, &[styled(false, Some(GREEN), 0.0, 1.0)]);
        assert!(
            near(px(&img, 10, 25), bgr(GREEN), 3),
            "không viền: nền tới sát mép"
        );
        assert_eq!(px(&img, 8, 25), [40, 40, 40, 255]);
        let img = render(60, 50, 40, &[styled(true, None, 0.0, 1.0)]);
        assert_eq!(img, solid(60, 50, 40), "không nền, không viền: không vẽ gì");
    }

    #[test]
    fn box_fill_at_half_opacity_blends_with_the_screenshot_once() {
        let fill = Rgb(255, 255, 255);
        let img = render(60, 50, 0, &[styled(false, Some(fill), 4.0, 0.5)]);
        let c = px(&img, 30, 25);
        assert!(near(c, [128, 128, 128, 255], 3), "{c:?}");
        let e = px(&img, 9, 25);
        assert!(near(e, [0, 0, 128, 255], 3), "{e:?}");
        // Chỗ viền đè nền (x = 11): chỉ trộn MỘT lần, bằng màu viền 50%, không sẫm hơn.
        let o = px(&img, 11, 25);
        assert!(near(o, [0, 0, 128, 255], 3), "{o:?}");
        let img = render(60, 50, 200, &[styled(false, Some(Rgb(0, 0, 0)), 0.0, 0.5)]);
        let c = px(&img, 30, 25);
        assert!(near(c, [100, 100, 100, 255], 3), "{c:?}");
    }

    #[test]
    fn oval_box_fills_the_ellipse_only() {
        let img = render(60, 50, 0, &[styled(true, Some(GREEN), 2.0, 1.0)]);
        assert!(near(px(&img, 30, 25), bgr(GREEN), 3));
        assert_eq!(px(&img, 11, 11), [0, 0, 0, 255], "góc khung nằm ngoài elip");
    }

    fn label(text: &str, align: Align, fill: Option<Rgb>, width: f32, opacity: f32) -> Shape {
        Shape::Label {
            origin: pt(10.0, 10.0),
            text: text.into(),
            size: 24.0,
            color: RED,
            align,
            style: Style {
                fill,
                stroke: BLUE,
                stroke_width: width,
                opacity,
            },
        }
    }

    /// Cột nhỏ nhất và lớn nhất có điểm đỏ trong các hàng [y0, y1).
    fn red_span(img: &Image, y0: u32, y1: u32) -> Option<(u32, u32)> {
        let mut span: Option<(u32, u32)> = None;
        for y in y0..y1 {
            for x in 0..img.width {
                let p = px(img, x, y);
                if p[2] > 200 && p[1] < 60 && p[0] < 60 {
                    span = Some(match span {
                        None => (x, x),
                        Some((a, b)) => (a.min(x), b.max(x)),
                    });
                }
            }
        }
        span
    }

    #[test]
    fn label_alignment_places_each_line_in_the_box() {
        let text = "WWWWWWWW\nW";
        let mut starts = Vec::new();
        for align in Align::ALL {
            let s = label(text, align, Some(Rgb(255, 255, 255)), 0.0, 1.0);
            let bounds = shape_bounds(&s);
            let img = render(260, 120, 0, &[s]);
            let line_h = (bounds.h as f32 / 2.0) as u32;
            let second = red_span(&img, 10 + line_h + 2, 10 + 2 * line_h - 2).expect("dòng 2");
            let first = red_span(&img, 10 + 4, 10 + line_h - 2).expect("dòng 1");
            let centre2 = (second.0 + second.1) as f32 / 2.0;
            let centre1 = (first.0 + first.1) as f32 / 2.0;
            match align {
                Align::Left => assert!(second.0.abs_diff(first.0) <= 2, "{first:?} {second:?}"),
                Align::Center => assert!((centre2 - centre1).abs() <= 3.0, "{centre1} {centre2}"),
                Align::Right => assert!(second.1.abs_diff(first.1) <= 2, "{first:?} {second:?}"),
            }
            starts.push(second.0);
        }
        assert!(starts[0] < starts[1] && starts[1] < starts[2], "{starts:?}");
    }

    #[test]
    fn label_box_has_border_fill_and_padding() {
        let s = label("Hi", Align::Left, Some(GREEN), 3.0, 1.0);
        let b = shape_bounds(&s);
        let img = render(200, 100, 0, &[s]);
        assert!(
            near(px(&img, 11, 20), bgr(BLUE), 3),
            "{:?}",
            px(&img, 11, 20)
        );
        assert!(near(px(&img, 14, 20), bgr(GREEN), 3));
        assert!(b.x <= 10 && b.y <= 10 && b.right() > 10 + 20);
    }

    #[test]
    fn label_background_is_optional_and_blends_with_opacity() {
        let img = render(200, 100, 90, &[label("Hi", Align::Left, None, 0.0, 1.0)]);
        assert_eq!(px(&img, 11, 11), [90, 90, 90, 255]);
        let img = render(
            200,
            100,
            0,
            &[label("Hi", Align::Left, Some(Rgb(255, 255, 255)), 0.0, 0.5)],
        );
        assert!(near(px(&img, 11, 11), [128, 128, 128, 255], 3));
    }

    #[test]
    fn label_bounds_grow_with_text_and_contain_the_caret() {
        let one = shape_bounds(&label("a", Align::Left, None, 0.0, 1.0));
        let two = shape_bounds(&label("a\nbbbbbb", Align::Left, None, 0.0, 1.0));
        assert!(two.w > one.w && two.h > one.h);
        let thick = shape_bounds(&label("a", Align::Left, Some(GREEN), 12.0, 1.0));
        assert!(thick.w >= one.w);
        let mut e = TextEdit::new(pt(10.0, 10.0), 24.0, RED);
        e.insert("ab\ncdef");
        for align in Align::ALL {
            let r = label_caret_rect(&e, align);
            let s = label("ab\ncdef", align, None, 0.0, 1.0);
            let b = shape_bounds(&s);
            assert!(r.x >= b.x && r.x <= b.right(), "{align:?} {r:?} {b:?}");
            assert!(
                r.y >= b.y && r.bottom() <= b.bottom() + 1,
                "{align:?} {r:?} {b:?}"
            );
        }
        let mut short = TextEdit::new(pt(10.0, 10.0), 24.0, RED);
        short.insert("WWWWWW\nW");
        let (l, r) = (
            label_caret_rect(&short, Align::Left),
            label_caret_rect(&short, Align::Right),
        );
        assert!(r.x > l.x + 10, "{l:?} {r:?}");
    }

    #[test]
    fn styled_shapes_keep_z_order_and_burn_offsets() {
        let back = styled(false, Some(BLUE), 0.0, 1.0);
        let front = Shape::Box {
            a: pt(20.0, 15.0),
            b: pt(40.0, 35.0),
            oval: false,
            style: Style {
                fill: Some(GREEN),
                ..Style::new(RED, 0.0)
            },
        };
        let img = render(60, 50, 0, &[back.clone(), front.clone()]);
        assert!(near(px(&img, 30, 25), bgr(GREEN), 3));
        let img = render(60, 50, 0, &[front, back]);
        assert!(near(px(&img, 30, 25), bgr(BLUE), 3));
        let mut img = solid(30, 30, 0);
        let crop = Rect {
            x: 10,
            y: 10,
            w: 30,
            h: 30,
        };
        burn(&mut img, &[styled(false, Some(GREEN), 0.0, 1.0)], crop).unwrap();
        assert!(near(px(&img, 5, 5), bgr(GREEN), 3), "dịch theo vùng cắt");
    }

    /// Chạy tay (an toàn, chỉ ghi một tệp PNG vào thư mục tạm): ảnh mẫu có mọi loại hình để xem bằng
    /// mắt. Lệnh: `cargo test -p darkshot-macos --bin darkshot-macos -- --ignored annotate_sample
    /// --test-threads=1`.
    #[test]
    #[ignore = "chạy tay: ghi PNG mẫu vào thư mục tạm"]
    fn annotate_sample() {
        let (w, h) = (640u32, 420u32);
        let mut bgra = Vec::new();
        for y in 0..h {
            for x in 0..w {
                // Nền dải màu nhẹ để thấy marker trong suốt và vùng che.
                bgra.extend_from_slice(&[(200 - y / 4) as u8, (170 + x / 10) as u8, 210, 255]);
            }
        }
        let mut img = Image {
            width: w,
            height: h,
            bgra,
        };
        let shapes = vec![
            Shape::Pen {
                points: vec![
                    pt(30.0, 60.0),
                    pt(60.0, 30.0),
                    pt(90.0, 70.0),
                    pt(130.0, 35.0),
                    pt(160.0, 60.0),
                ],
                width: 4.0,
                color: RED,
            },
            Shape::Line {
                a: pt(200.0, 30.0),
                b: pt(320.0, 80.0),
                width: 4.0,
                color: BLUE,
            },
            Shape::Arrow {
                a: pt(360.0, 80.0),
                b: pt(500.0, 30.0),
                width: 4.0,
                color: RED,
            },
            Shape::Rect {
                a: pt(30.0, 110.0),
                b: pt(170.0, 200.0),
                width: 4.0,
                color: Rgb(0, 150, 0),
            },
            Shape::Ellipse {
                a: pt(200.0, 110.0),
                b: pt(340.0, 200.0),
                width: 4.0,
                color: Rgb(160, 0, 160),
            },
            Shape::Marker {
                points: vec![pt(370.0, 140.0), pt(560.0, 140.0)],
                width: 24.0,
                color: YELLOW,
            },
            text(pt(380.0, 120.0), "Marker trên chữ", 22.0, Rgb(20, 20, 20)),
            Shape::Blur {
                a: pt(30.0, 230.0),
                b: pt(170.0, 290.0),
                color: Rgb(60, 60, 60),
            },
            text(pt(40.0, 240.0), "Che sau, chữ trước", 20.0, YELLOW),
            text(pt(200.0, 230.0), "Việt Nam\nHai dòng 😀", 26.0, RED),
            Shape::Step {
                center: pt(60.0, 350.0),
                radius: 22.0,
                n: 1,
                color: RED,
            },
            Shape::Step {
                center: pt(130.0, 350.0),
                radius: 22.0,
                n: 2,
                color: BLUE,
            },
            Shape::Step {
                center: pt(200.0, 350.0),
                radius: 22.0,
                n: 12,
                color: YELLOW,
            },
            Shape::Rect {
                a: pt(420.0, 260.0),
                b: pt(600.0, 380.0),
                width: 12.0,
                color: RED,
            },
            Shape::Rect {
                a: pt(450.0, 290.0),
                b: pt(630.0, 400.0),
                width: 12.0,
                color: BLUE,
            },
            Shape::Box {
                a: pt(240.0, 320.0),
                b: pt(330.0, 400.0),
                oval: false,
                style: Style {
                    fill: Some(Rgb(255, 255, 255)),
                    stroke: Rgb(0, 90, 200),
                    stroke_width: 6.0,
                    opacity: 0.5,
                },
            },
            Shape::Box {
                a: pt(340.0, 320.0),
                b: pt(420.0, 400.0),
                oval: true,
                style: Style {
                    fill: Some(Rgb(255, 200, 0)),
                    stroke: Rgb(120, 0, 0),
                    stroke_width: 4.0,
                    opacity: 1.0,
                },
            },
            Shape::Label {
                origin: pt(450.0, 20.0),
                text: "Căn giữa\nmột dòng dài hơn\nxin chào".into(),
                size: 20.0,
                color: Rgb(20, 20, 20),
                align: Align::Center,
                style: Style {
                    fill: Some(Rgb(255, 255, 160)),
                    stroke: Rgb(120, 90, 0),
                    stroke_width: 3.0,
                    opacity: 0.85,
                },
            },
            Shape::Label {
                origin: pt(450.0, 150.0),
                text: "Căn phải\nngắn".into(),
                size: 20.0,
                color: YELLOW,
                align: Align::Right,
                style: Style::new(Rgb(0, 0, 0), 0.0),
            },
        ];
        let crop = full(&img);
        burn(&mut img, &shapes, crop).expect("burn");
        let png = darkshot_core::image::encode_png(&img).expect("png");
        let path = std::env::temp_dir().join("darkshot-annotate-sample.png");
        std::fs::write(&path, png).expect("write");
        println!("annotate_sample: {}", path.display());
    }
}
