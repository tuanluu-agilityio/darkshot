//! Vẽ các hình đánh dấu bằng GDI+: một hàm vẽ dùng chung cho lớp phủ và cho ảnh ra (`burn`).

use crate::gdiplus::{GdiPlus, Gp};
use darkshot_core::annotate::{
    arrow_geometry, arrow_min_head, blur_pixel_rect, outline_color, Pt, Shape, TextEdit,
    MARKER_ALPHA, STEP_RING, TEXT_DIP, TEXT_OUTLINE_DIP,
};
use darkshot_core::geometry::Rect;
use darkshot_core::image::Image;
use darkshot_core::theme::Rgb;
use std::ffi::c_void;
use windows::core::{w, PCWSTR};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GdiFlush, SelectObject,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
};
use windows::Win32::Graphics::GdiPlus::{
    CombineModeReplace, DashCapFlat, FillMode, FillModeAlternate, FillModeWinding, FontStyleBold,
    GdipAddPathLine2, GdipAddPathString, GdipCloneStringFormat, GdipCreateFont,
    GdipCreateFontFamilyFromName, GdipCreatePath, GdipCreatePen1, GdipCreateSolidFill,
    GdipDeleteBrush, GdipDeleteFont, GdipDeleteFontFamily, GdipDeletePath, GdipDeletePen,
    GdipDeleteStringFormat, GdipDrawEllipse, GdipDrawLine, GdipDrawLines, GdipDrawPath,
    GdipDrawRectangle, GdipFillEllipse, GdipFillPath, GdipFillPolygon, GdipFillRectangle,
    GdipFillRectangleI, GdipGetEmHeight, GdipGetLineSpacing, GdipGetPathWorldBounds,
    GdipGetPixelOffsetMode, GdipGetSmoothingMode, GdipMeasureString, GdipSetClipRectI,
    GdipSetPathFillMode, GdipSetPenEndCap, GdipSetPenLineCap197819, GdipSetPenLineJoin,
    GdipSetPenStartCap, GdipSetPixelOffsetMode, GdipSetSmoothingMode, GdipSetStringFormatFlags,
    GdipSetTextRenderingHint, GdipStringFormatGetGenericTypographic, GdipTranslateWorldTransform,
    GdipWidenPath, GpBrush, GpFont, GpFontFamily, GpGraphics, GpPath, GpPen, GpSolidFill,
    GpStringFormat, LineCap, LineCapFlat, LineCapRound, LineCapSquare, LineJoin, LineJoinMiter,
    LineJoinRound, MatrixOrderPrepend, PixelOffsetModeHalf, PointF, RectF, SmoothingModeAntiAlias,
    SmoothingModeNone, Status, StringFormatFlagsLineLimit, StringFormatFlagsMeasureTrailingSpaces,
    StringFormatFlagsNoClip, StringFormatFlagsNoFitBlackBox, StringFormatFlagsNoWrap,
    TextRenderingHintAntiAlias, UnitPixel,
};

/// Độ phẳng khi đổi đường cong thành đoạn thẳng cho `GdipWidenPath` (mặc định của GDI+).
const FLATNESS: f32 = 0.25;

fn ok(st: Status) -> bool {
    st.0 == 0
}

fn argb(c: Rgb, a: u8) -> u32 {
    (u32::from(a) << 24) | (u32::from(c.0) << 16) | (u32::from(c.1) << 8) | u32::from(c.2)
}

/// Điểm chung khi mọi điểm của nét trùng nhau (nét chỉ là một chấm); `None` khi rỗng hoặc có điểm khác.
fn single_spot(points: &[Pt]) -> Option<Pt> {
    let first = *points.first()?;
    points.iter().all(|p| *p == first).then_some(first)
}

fn pf(p: Pt) -> PointF {
    PointF { X: p.x, Y: p.y }
}

/// Bút GDI+; xóa khi drop.
struct Pen(*mut GpPen);

impl Pen {
    fn new(color: u32, width: f32, cap: LineCap, join: LineJoin) -> Option<Pen> {
        let mut p: *mut GpPen = std::ptr::null_mut();
        // SAFETY: con trỏ ra hợp lệ; kết quả kiểm null trước khi dùng.
        if !ok(unsafe { GdipCreatePen1(color, width, UnitPixel, &mut p) }) || p.is_null() {
            return None;
        }
        let pen = Pen(p);
        // SAFETY: bút hợp lệ vừa tạo.
        unsafe {
            GdipSetPenLineCap197819(pen.0, cap, cap, DashCapFlat);
            GdipSetPenLineJoin(pen.0, join);
        }
        Some(pen)
    }
}

impl Drop for Pen {
    fn drop(&mut self) {
        // SAFETY: bút do GdipCreatePen1 tạo, xóa đúng một lần.
        unsafe { GdipDeletePen(self.0) };
    }
}

/// Chổi tô đặc GDI+; xóa khi drop.
struct Brush(*mut GpSolidFill);

impl Brush {
    fn new(color: u32) -> Option<Brush> {
        let mut b: *mut GpSolidFill = std::ptr::null_mut();
        // SAFETY: con trỏ ra hợp lệ; kết quả kiểm null trước khi dùng.
        if !ok(unsafe { GdipCreateSolidFill(color, &mut b) }) || b.is_null() {
            return None;
        }
        Some(Brush(b))
    }

    fn raw(&self) -> *mut GpBrush {
        self.0.cast()
    }
}

impl Drop for Brush {
    fn drop(&mut self) {
        // SAFETY: chổi do GdipCreateSolidFill tạo, xóa đúng một lần.
        unsafe { GdipDeleteBrush(self.raw()) };
    }
}

/// Đường GDI+; xóa khi drop.
struct Path(*mut GpPath);

impl Path {
    fn new(mode: FillMode) -> Option<Path> {
        let mut p: *mut GpPath = std::ptr::null_mut();
        // SAFETY: con trỏ ra hợp lệ; kết quả kiểm null trước khi dùng.
        if !ok(unsafe { GdipCreatePath(mode, &mut p) }) || p.is_null() {
            return None;
        }
        Some(Path(p))
    }
}

impl Drop for Path {
    fn drop(&mut self) {
        // SAFETY: đường do GdipCreatePath tạo, xóa đúng một lần.
        unsafe { GdipDeletePath(self.0) };
    }
}

/// Phông, định dạng và khoảng cách dòng của chữ đánh dấu (Segoe UI đậm).
struct TextStyle {
    family: *mut GpFontFamily,
    format: *mut GpStringFormat,
    /// Chiều cao dòng chia cho em.
    line_ratio: f32,
}

impl TextStyle {
    fn new() -> Option<TextStyle> {
        let mut family: *mut GpFontFamily = std::ptr::null_mut();
        // SAFETY: tên phông là chuỗi hằng kết thúc bằng 0; con trỏ ra hợp lệ.
        let st = unsafe {
            GdipCreateFontFamilyFromName(w!("Segoe UI"), std::ptr::null_mut(), &mut family)
        };
        if !ok(st) || family.is_null() {
            return None;
        }
        let mut style = TextStyle {
            family,
            format: std::ptr::null_mut(),
            line_ratio: 0.0,
        };
        let mut generic: *mut GpStringFormat = std::ptr::null_mut();
        // SAFETY: định dạng chung của GDI+ không được xóa; chỉ bản sao thuộc về TextStyle.
        unsafe {
            if !ok(GdipStringFormatGetGenericTypographic(&mut generic)) || generic.is_null() {
                return None;
            }
            if !ok(GdipCloneStringFormat(generic, &mut style.format)) || style.format.is_null() {
                style.format = std::ptr::null_mut();
                return None;
            }
            GdipSetStringFormatFlags(
                style.format,
                StringFormatFlagsLineLimit.0
                    | StringFormatFlagsNoClip.0
                    | StringFormatFlagsNoFitBlackBox.0
                    | StringFormatFlagsNoWrap.0
                    | StringFormatFlagsMeasureTrailingSpaces.0,
            );
        }
        let (mut spacing, mut em) = (0u16, 0u16);
        // SAFETY: họ phông hợp lệ; con trỏ ra là biến cục bộ.
        unsafe {
            GdipGetLineSpacing(style.family, FontStyleBold.0, &mut spacing);
            GdipGetEmHeight(style.family, FontStyleBold.0, &mut em);
        }
        if spacing == 0 || em == 0 {
            return None;
        }
        style.line_ratio = f32::from(spacing) / f32::from(em);
        Some(style)
    }

    fn line_height(&self, size: f32) -> f32 {
        size * self.line_ratio
    }

    /// Đường chữ (mỗi dòng một lần `GdipAddPathString`, cách nhau chiều cao dòng của họ phông).
    fn path(&self, origin: Pt, text: &str, size: f32) -> Option<Path> {
        let path = Path::new(FillModeWinding)?;
        let lh = self.line_height(size);
        for (i, line) in text.split('\n').enumerate() {
            let wide: Vec<u16> = line.encode_utf16().collect();
            if wide.is_empty() {
                continue;
            }
            let len = i32::try_from(wide.len()).ok()?;
            let layout = RectF {
                X: origin.x,
                Y: origin.y + i as f32 * lh,
                Width: 0.0,
                Height: 0.0,
            };
            // SAFETY: wide sống tới hết lời gọi và có đúng `len` phần tử; phông, định dạng hợp lệ.
            let st = unsafe {
                GdipAddPathString(
                    path.0,
                    PCWSTR(wide.as_ptr()),
                    len,
                    self.family,
                    FontStyleBold.0,
                    size,
                    &layout,
                    self.format,
                )
            };
            if !ok(st) {
                return None;
            }
        }
        Some(path)
    }
}

impl Drop for TextStyle {
    fn drop(&mut self) {
        // SAFETY: họ phông và bản sao định dạng do TextStyle tạo, xóa đúng một lần.
        unsafe {
            if !self.format.is_null() {
                GdipDeleteStringFormat(self.format);
            }
            GdipDeleteFontFamily(self.family);
        }
    }
}

/// Bút viền chữ: dày 2 × TEXT_OUTLINE_DIP đổi ra px theo tỉ lệ `size / TEXT_DIP`, nối tròn.
fn text_outline_pen(size: f32, color: Rgb) -> Option<Pen> {
    let width = 2.0 * TEXT_OUTLINE_DIP * size / TEXT_DIP;
    Pen::new(
        argb(outline_color(color), 255),
        width,
        LineCapRound,
        LineJoinRound,
    )
}

/// Vẽ `shapes` lên `dc`: điểm ảnh (x, y) của ảnh chụp nằm ở (x - ox, y - oy) của dc; chỉ vẽ trong `clip`
/// (tọa độ ảnh chụp). GDI+ phải đang chạy. false khi không tạo được đối tượng GDI+ (đã vẽ được gì thì thôi).
pub fn draw_shapes(dc: HDC, shapes: &[Shape], ox: i32, oy: i32, clip: Rect) -> bool {
    if shapes.is_empty() {
        return true;
    }
    let Some(gp) = Gp::new(dc) else {
        return false;
    };
    let g = gp.raw();
    // SAFETY: g hợp lệ trong suốt đời của gp. Vùng cắt tính theo tọa độ thế giới (ảnh chụp).
    let setup = unsafe {
        ok(GdipTranslateWorldTransform(
            g,
            -(ox as f32),
            -(oy as f32),
            MatrixOrderPrepend,
        )) && ok(GdipSetClipRectI(
            g,
            clip.x,
            clip.y,
            clip.w,
            clip.h,
            CombineModeReplace,
        ))
    };
    if !setup {
        return false;
    }
    let mut text: Option<TextStyle> = None;
    let mut all = true;
    for s in shapes {
        all &= draw_one(g, s, &mut text);
    }
    all
}

/// Vẽ một hình; false khi không tạo được đối tượng GDI+ hoặc GDI+ báo lỗi.
fn draw_one(g: *mut GpGraphics, s: &Shape, text: &mut Option<TextStyle>) -> bool {
    match s {
        // Hình có kiểu và khối chữ chỉ do bản macOS tạo (Task 112); Windows không bao giờ gặp.
        Shape::Box { .. } | Shape::Label { .. } => true,
        Shape::Pen {
            points,
            width,
            color,
        } => {
            let c = argb(*color, 255);
            if points.is_empty() {
                return true;
            }
            match single_spot(points) {
                Some(p) => {
                    let Some(b) = Brush::new(c) else { return false };
                    let r = width / 2.0;
                    // SAFETY: g và chổi hợp lệ.
                    ok(unsafe { GdipFillEllipse(g, b.raw(), p.x - r, p.y - r, *width, *width) })
                }
                None => {
                    let pts = points.as_slice();
                    let Some(pen) = Pen::new(c, *width, LineCapRound, LineJoinRound) else {
                        return false;
                    };
                    let v: Vec<PointF> = pts.iter().map(|p| pf(*p)).collect();
                    let Ok(n) = i32::try_from(v.len()) else {
                        return false;
                    };
                    // SAFETY: v có đúng n điểm và sống tới hết lời gọi.
                    ok(unsafe { GdipDrawLines(g, pen.0, v.as_ptr(), n) })
                }
            }
        }
        Shape::Line { a, b, width, color } => {
            let Some(pen) = Pen::new(argb(*color, 255), *width, LineCapRound, LineJoinRound) else {
                return false;
            };
            // SAFETY: g và bút hợp lệ.
            ok(unsafe { GdipDrawLine(g, pen.0, a.x, a.y, b.x, b.y) })
        }
        Shape::Arrow { a, b, width, color } => draw_arrow(g, *a, *b, *width, *color),
        Shape::Rect { a, b, width, color } => {
            let Some(pen) = Pen::new(argb(*color, 255), *width, LineCapFlat, LineJoinMiter) else {
                return false;
            };
            let (x, y) = (a.x.min(b.x), a.y.min(b.y));
            let (w, h) = ((b.x - a.x).abs(), (b.y - a.y).abs());
            // SAFETY: g và bút hợp lệ.
            ok(unsafe { GdipDrawRectangle(g, pen.0, x, y, w, h) })
        }
        Shape::Marker {
            points,
            width,
            color,
        } => draw_marker(g, points, *width, *color),
        Shape::Blur { a, b, color } => draw_blur(g, *a, *b, *color),
        Shape::Text {
            origin,
            text: t,
            size,
            color,
        } => {
            if text.is_none() {
                *text = TextStyle::new();
            }
            let Some(style) = text.as_ref() else {
                return false;
            };
            draw_text(g, style, *origin, t, *size, *color)
        }
        Shape::Ellipse { a, b, width, color } => {
            let Some(pen) = Pen::new(argb(*color, 255), *width, LineCapRound, LineJoinRound) else {
                return false;
            };
            let (x, y) = (a.x.min(b.x), a.y.min(b.y));
            let (w, h) = ((b.x - a.x).abs(), (b.y - a.y).abs());
            // SAFETY: g và bút hợp lệ.
            ok(unsafe { GdipDrawEllipse(g, pen.0, x, y, w, h) })
        }
        Shape::Step {
            center,
            radius,
            n,
            color,
        } => {
            if *n != 0 && text.is_none() {
                *text = TextStyle::new();
            }
            draw_step(g, text.as_ref(), *center, *radius, *n, *color)
        }
    }
}

/// Vòng đánh số: lòng màu `color`, viền và số màu `outline_color(color)`. Viền dày
/// `radius * STEP_RING` nằm trong hình tròn bán kính `radius` (mép ngoài của viền là mép vòng).
/// `n == 0`: không vẽ số. `style` chỉ cần khi có số.
fn draw_step(
    g: *mut GpGraphics,
    style: Option<&TextStyle>,
    center: Pt,
    radius: f32,
    n: u32,
    color: Rgb,
) -> bool {
    let ring = radius * STEP_RING;
    let mid = radius - ring / 2.0;
    let ink = argb(outline_color(color), 255);
    let (Some(fill), Some(pen)) = (
        Brush::new(argb(color, 255)),
        Pen::new(ink, ring, LineCapFlat, LineJoinRound),
    ) else {
        return false;
    };
    let (x, y, d) = (center.x - mid, center.y - mid, 2.0 * mid);
    // SAFETY: g, chổi và bút hợp lệ.
    let disc = unsafe {
        ok(GdipFillEllipse(g, fill.raw(), x, y, d, d)) && ok(GdipDrawEllipse(g, pen.0, x, y, d, d))
    };
    if !disc {
        return false;
    }
    drop(fill);
    drop(pen);
    if n == 0 {
        return true;
    }
    let Some(style) = style else {
        return false;
    };
    draw_step_number(g, style, center, radius, n, ink)
}

/// Hộp (không viền) của đường chữ `s` cỡ `size` đặt ở gốc (0, 0).
fn path_box(style: &TextStyle, s: &str, size: f32) -> Option<RectF> {
    let path = style.path(Pt { x: 0.0, y: 0.0 }, s, size)?;
    let mut b = RectF::default();
    // SAFETY: đường hợp lệ; b là biến cục bộ; không ma trận, không bút.
    let st = unsafe { GdipGetPathWorldBounds(path.0, &mut b, std::ptr::null(), std::ptr::null()) };
    (ok(st) && b.Width > 0.0 && b.Height > 0.0).then_some(b)
}

/// Số `n` giữa vòng: cỡ em `radius * 1.1`, thu theo tỉ lệ nếu rộng quá `1.5 * radius`;
/// tâm hộp đường đo được trùng `center`.
fn draw_step_number(
    g: *mut GpGraphics,
    style: &TextStyle,
    center: Pt,
    radius: f32,
    n: u32,
    ink: u32,
) -> bool {
    let s = n.to_string();
    let mut size = radius * 1.1;
    let Some(mut b) = path_box(style, &s, size) else {
        return false;
    };
    let max_w = 1.5 * radius;
    if b.Width > max_w {
        let k = max_w / b.Width;
        size *= k;
        // Đường chữ đặt ở gốc (0, 0) co giãn tuyến tính theo cỡ em.
        b = RectF {
            X: b.X * k,
            Y: b.Y * k,
            Width: b.Width * k,
            Height: b.Height * k,
        };
    }
    let origin = Pt {
        x: center.x - (b.X + b.Width / 2.0),
        y: center.y - (b.Y + b.Height / 2.0),
    };
    let (Some(path), Some(brush)) = (style.path(origin, &s, size), Brush::new(ink)) else {
        return false;
    };
    // SAFETY: g, đường và chổi hợp lệ.
    ok(unsafe { GdipFillPath(g, brush.raw(), path.0) })
}

fn draw_arrow(g: *mut GpGraphics, a: Pt, b: Pt, width: f32, color: Rgb) -> bool {
    let c = argb(color, 255);
    let geo = arrow_geometry(a, b, width, arrow_min_head(width));
    if geo.shaft_end != a {
        let Some(pen) = Pen::new(c, width, LineCapFlat, LineJoinRound) else {
            return false;
        };
        // Đầu tròn ở `a`, đầu phẳng ở phía mũi; thân lấn 1 px vào đầu mũi tên để không lộ đường nối
        // khử răng cưa giữa thân và tam giác.
        let (dx, dy) = (b.x - geo.shaft_end.x, b.y - geo.shaft_end.y);
        let depth = dx.hypot(dy);
        let k = if depth > 0.0 {
            (1.0f32).min(depth / 2.0) / depth
        } else {
            0.0
        };
        let end = Pt {
            x: geo.shaft_end.x + dx * k,
            y: geo.shaft_end.y + dy * k,
        };
        // SAFETY: g và bút hợp lệ.
        let drawn = unsafe {
            GdipSetPenStartCap(pen.0, LineCapRound);
            GdipSetPenEndCap(pen.0, LineCapFlat);
            GdipDrawLine(g, pen.0, a.x, a.y, end.x, end.y)
        };
        if !ok(drawn) {
            return false;
        }
    }
    let Some(brush) = Brush::new(c) else {
        return false;
    };
    let head = geo.head.map(pf);
    // SAFETY: head có đúng 3 điểm; g và chổi hợp lệ.
    ok(unsafe { GdipFillPolygon(g, brush.raw(), head.as_ptr(), 3, FillModeAlternate) })
}

/// Bút dạ quang: nới đường thành viền (`GdipWidenPath`) rồi tô một lần với `FillModeWinding`,
/// nên chỗ nét tự cắt chỉ được phủ một lần (không đậm hơn).
fn draw_marker(g: *mut GpGraphics, points: &[Pt], width: f32, color: Rgb) -> bool {
    if points.is_empty() {
        return true;
    }
    let Some(brush) = Brush::new(argb(color, MARKER_ALPHA)) else {
        return false;
    };
    // Mọi điểm trùng nhau: `GdipWidenPath` không nới được đường dài 0, vẽ ô vuông như nét một điểm.
    if let Some(first) = single_spot(points) {
        let r = width / 2.0;
        // SAFETY: g và chổi hợp lệ.
        return ok(unsafe {
            GdipFillRectangle(g, brush.raw(), first.x - r, first.y - r, width, width)
        });
    }
    let (Some(path), Some(pen)) = (
        Path::new(FillModeWinding),
        Pen::new(argb(color, 255), width, LineCapSquare, LineJoinRound),
    ) else {
        return false;
    };
    let v: Vec<PointF> = points.iter().map(|p| pf(*p)).collect();
    let Ok(n) = i32::try_from(v.len()) else {
        return false;
    };
    // SAFETY: v có đúng n điểm và sống tới hết lời gọi; đường, bút, chổi, g hợp lệ.
    unsafe {
        ok(GdipAddPathLine2(path.0, v.as_ptr(), n))
            && ok(GdipWidenPath(path.0, pen.0, std::ptr::null_mut(), FLATNESS))
            && ok(GdipSetPathFillMode(path.0, FillModeWinding))
            && ok(GdipFillPath(g, brush.raw(), path.0))
    }
}

/// Vùng che: tô đặc `blur_pixel_rect(a, b)` bằng `color`, alpha 255, không khử răng cưa.
/// Lõi đục phủ đúng các điểm ảnh của hình chữ nhật: tắt khử răng cưa, lệch nửa pixel (tâm điểm ảnh
/// i ở i + 0,5 nên hình [x, x + w) phủ trọn đúng w điểm ảnh), rồi trả lại chế độ cũ.
fn draw_blur(g: *mut GpGraphics, a: Pt, b: Pt, color: Rgb) -> bool {
    let r = blur_pixel_rect(a, b);
    if r.w <= 0 || r.h <= 0 {
        return true;
    }
    let Some(brush) = Brush::new(argb(color, 255)) else {
        return false;
    };
    let mut smooth = SmoothingModeAntiAlias;
    let mut offset = PixelOffsetModeHalf;
    // SAFETY: g và chổi hợp lệ; hai con trỏ ra trỏ tới biến cục bộ.
    unsafe {
        let smooth_saved = ok(GdipGetSmoothingMode(g, &mut smooth));
        let offset_saved = ok(GdipGetPixelOffsetMode(g, &mut offset));
        let drawn = smooth_saved
            && offset_saved
            && ok(GdipSetSmoothingMode(g, SmoothingModeNone))
            && ok(GdipSetPixelOffsetMode(g, PixelOffsetModeHalf))
            && ok(GdipFillRectangleI(g, brush.raw(), r.x, r.y, r.w, r.h));
        // Trả chế độ cũ trên mọi đường đi để các hình sau không bị ảnh hưởng; chế độ nào không đọc
        // được thì không trả (không ghi đè bằng giá trị khởi tạo).
        let mut restored = true;
        if smooth_saved {
            restored &= ok(GdipSetSmoothingMode(g, smooth));
        }
        if offset_saved {
            restored &= ok(GdipSetPixelOffsetMode(g, offset));
        }
        drawn && restored
    }
}

fn draw_text(
    g: *mut GpGraphics,
    style: &TextStyle,
    origin: Pt,
    text: &str,
    size: f32,
    color: Rgb,
) -> bool {
    let (Some(path), Some(pen), Some(brush)) = (
        style.path(origin, text, size),
        text_outline_pen(size, color),
        Brush::new(argb(color, 255)),
    ) else {
        return false;
    };
    // SAFETY: đường, bút, chổi, g hợp lệ.
    unsafe { ok(GdipDrawPath(g, pen.0, path.0)) && ok(GdipFillPath(g, brush.raw(), path.0)) }
}

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

/// Hộp bao (tọa độ ảnh chụp) của một hình Text, đo bằng GDI+ cùng phông với lúc vẽ; cũng dùng cho mọi hình khác (gọi Shape::bounds).
pub fn shape_bounds(s: &Shape) -> Rect {
    if let Some(r) = s.bounds() {
        return r;
    }
    let empty = Rect {
        x: 0,
        y: 0,
        w: 0,
        h: 0,
    };
    let Shape::Text {
        origin,
        text,
        size,
        color,
    } = s
    else {
        // Pen/Marker không có điểm: không vẽ gì.
        return empty;
    };
    text_bounds(*origin, text, *size, *color).unwrap_or_else(|| {
        // Không đo được: ước lượng rộng tay (mỗi ký tự một em, dòng 1,5 em, cộng viền).
        let lines = text.split('\n').count().max(1) as f32;
        let cols = text
            .split('\n')
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0) as f32;
        let pad = 2.0 * TEXT_OUTLINE_DIP * size / TEXT_DIP;
        round_out(
            origin.x - pad,
            origin.y - pad,
            origin.x + cols * size + pad,
            origin.y + lines * size * 1.5 + pad,
        )
    })
}

fn text_bounds(origin: Pt, text: &str, size: f32, color: Rgb) -> Option<Rect> {
    let gdiplus = GdiPlus::start();
    if !gdiplus.ok() {
        return None;
    }
    let r = {
        let style = TextStyle::new()?;
        let path = style.path(origin, text, size)?;
        let pen = text_outline_pen(size, color)?;
        let mut b = RectF::default();
        // SAFETY: đường và bút hợp lệ; b là biến cục bộ.
        if !ok(unsafe { GdipGetPathWorldBounds(path.0, &mut b, std::ptr::null(), pen.0) }) {
            return None;
        }
        // Gồm cả góc đầu dòng đầu để hộp luôn chứa vị trí đặt chữ.
        let (x0, y0) = (b.X.min(origin.x), b.Y.min(origin.y));
        round_out(x0, y0, b.X + b.Width, b.Y + b.Height)
    };
    drop(gdiplus);
    Some(r)
}

/// DC bộ nhớ (để đo chữ); xóa khi drop.
struct MemDc(HDC);

impl MemDc {
    fn new() -> Option<MemDc> {
        // SAFETY: tạo DC bộ nhớ tương thích màn hình; kiểm hợp lệ trước khi dùng.
        let dc = unsafe { CreateCompatibleDC(None) };
        (!dc.is_invalid()).then_some(MemDc(dc))
    }
}

impl Drop for MemDc {
    fn drop(&mut self) {
        // SAFETY: DC do CreateCompatibleDC tạo, xóa đúng một lần.
        let _ = unsafe { DeleteDC(self.0) };
    }
}

/// Phông GDI+; xóa khi drop.
struct Font(*mut GpFont);

impl Drop for Font {
    fn drop(&mut self) {
        // SAFETY: phông do GdipCreateFont tạo, xóa đúng một lần.
        unsafe { GdipDeleteFont(self.0) };
    }
}

/// Bề rộng tiến (advance) của `s` theo phông chữ đánh dấu cỡ `size`, đo không bám lưới pixel
/// để khớp với chữ vẽ bằng đường.
fn advance_width(style: &TextStyle, s: &str, size: f32) -> Option<f32> {
    let wide: Vec<u16> = s.encode_utf16().collect();
    if wide.is_empty() {
        return Some(0.0);
    }
    let len = i32::try_from(wide.len()).ok()?;
    let dc = MemDc::new()?;
    let gp = Gp::new(dc.0)?;
    let mut f: *mut GpFont = std::ptr::null_mut();
    // SAFETY: họ phông hợp lệ; con trỏ ra là biến cục bộ.
    if !ok(unsafe { GdipCreateFont(style.family, size, FontStyleBold.0, UnitPixel, &mut f) })
        || f.is_null()
    {
        return None;
    }
    let font = Font(f);
    let layout = RectF::default();
    let mut bbox = RectF::default();
    // SAFETY: wide có đúng `len` phần tử; g, phông, định dạng hợp lệ; con trỏ ra cục bộ.
    let st = unsafe {
        GdipSetTextRenderingHint(gp.raw(), TextRenderingHintAntiAlias);
        GdipMeasureString(
            gp.raw(),
            PCWSTR(wide.as_ptr()),
            len,
            font.0,
            &layout,
            style.format,
            &mut bbox,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    drop(font);
    drop(gp);
    drop(dc);
    ok(st).then_some(bbox.Width)
}

/// Vị trí và chiều cao con trỏ nháy (tọa độ ảnh chụp) của ô chữ đang soạn.
pub fn caret_rect(edit: &TextEdit) -> Rect {
    // TextEdit không cho đọc gốc và cỡ chữ: lấy qua bản sao có thêm một ký tự (to_shape luôn Some).
    let mut probe = edit.clone();
    probe.insert("x");
    let Some(Shape::Text { origin, size, .. }) = probe.to_shape() else {
        return Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        };
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
    let gdiplus = GdiPlus::start();
    let measured = if gdiplus.ok() {
        TextStyle::new().and_then(|style| {
            let w = advance_width(&style, &prefix, size)?;
            Some((w, style.line_height(size)))
        })
    } else {
        None
    };
    drop(gdiplus);
    // Không đo được: ước lượng nửa em mỗi ký tự, dòng 1,33 em (gần Segoe UI).
    let (dx, lh) = measured.unwrap_or((prefix.chars().count() as f32 * size * 0.5, size * 1.33));
    Rect {
        x: (origin.x + dx).floor() as i32,
        y: (origin.y + line as f32 * lh).floor() as i32,
        w: ((size / TEXT_DIP).round() as i32).max(1),
        h: (lh.ceil() as i32).max(1),
    }
}

/// In `shapes` vào ảnh đã cắt từ vùng `crop` (tọa độ ảnh chụp). Tự khởi động GDI+. Ảnh giữ alpha 255.
pub fn burn(img: &mut Image, shapes: &[Shape], crop: Rect) -> Result<(), String> {
    if shapes.is_empty() {
        return Ok(());
    }
    let gdiplus = GdiPlus::start();
    if !gdiplus.ok() {
        return Err("GDI+ could not start".into());
    }
    let r = with_dib(img, |dc| draw_shapes(dc, shapes, crop.x, crop.y, crop));
    drop(gdiplus);
    r
}

/// DC bộ nhớ có DIB 32 bit được chọn sẵn; giải phóng khi drop.
struct Dib {
    dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
}

impl Dib {
    fn new(w: i32, h: i32) -> Option<Dib> {
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        // SAFETY: DC và DIB vừa tạo; mọi đường lỗi giải phóng phần đã tạo.
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return None;
            }
            let bmp = match CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
                Ok(b) if !bits.is_null() => b,
                Ok(b) => {
                    let _ = DeleteObject(b.into());
                    let _ = DeleteDC(dc);
                    return None;
                }
                Err(_) => {
                    let _ = DeleteDC(dc);
                    return None;
                }
            };
            let old = SelectObject(dc, bmp.into());
            Some(Dib {
                dc,
                bmp,
                old,
                bits: bits.cast(),
            })
        }
    }
}

impl Drop for Dib {
    fn drop(&mut self) {
        // SAFETY: bỏ chọn rồi xóa bitmap và DC do Dib tạo, mỗi thứ đúng một lần.
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.bmp.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

/// Chép `img` vào một DIB 32 bit, gọi `paint` với DC của nó, chép kết quả về `img` (alpha 255).
/// `img` không đổi khi lỗi.
fn with_dib(img: &mut Image, paint: impl FnOnce(HDC) -> bool) -> Result<(), String> {
    if !img.is_valid() {
        return Err("invalid image".into());
    }
    let (Ok(w), Ok(h)) = (i32::try_from(img.width), i32::try_from(img.height)) else {
        return Err("image too large".into());
    };
    let Some(dib) = Dib::new(w, h) else {
        return Err("could not create a bitmap".into());
    };
    let len = img.bgra.len();
    // SAFETY: DIB 32 bit từ trên xuống cỡ w×h có đúng w*h*4 = len byte (is_valid), hàng không đệm.
    unsafe { std::ptr::copy_nonoverlapping(img.bgra.as_ptr(), dib.bits, len) };
    if !paint(dib.dc) {
        return Err("could not draw the markings".into());
    }
    // SAFETY: đẩy hết lệnh GDI đang chờ trước khi đọc bộ nhớ của DIB.
    let _ = unsafe { GdiFlush() };
    // SAFETY: như trên; dib còn sống, hai vùng nhớ không chồng nhau.
    unsafe { std::ptr::copy_nonoverlapping(dib.bits, img.bgra.as_mut_ptr(), len) };
    drop(dib);
    for p in img.bgra.as_chunks_mut::<4>().0 {
        p[3] = 255;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gdiplus::GdiPlus;
    use darkshot_core::annotate::{arrow_min_head, Pt, PALETTE, STEP_RING};
    use darkshot_core::theme::Rgb;

    const RED: Rgb = Rgb(255, 0, 0);

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

    /// Ảnh giả ngẫu nhiên (LCG), alpha 255.
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
    fn line_pixels() {
        let mut img = solid(40, 20, 0);
        let line = Shape::Line {
            a: pt(5.0, 10.0),
            b: pt(35.0, 10.0),
            width: 3.0,
            color: RED,
        };
        let crop = full(&img);
        assert_eq!(burn(&mut img, &[line], crop), Ok(()));
        assert!(
            near(px(&img, 20, 10), [0, 0, 255, 255], 2),
            "{:?}",
            px(&img, 20, 10)
        );
        assert_eq!(px(&img, 20, 14), [0, 0, 0, 255]);
        assert!(all_opaque(&img));
    }

    #[test]
    fn rect_outline_not_filled() {
        let mut img = solid(60, 50, 0);
        let r = Shape::Rect {
            a: pt(50.0, 40.0),
            b: pt(10.0, 10.0),
            width: 3.0,
            color: RED,
        };
        let crop = full(&img);
        assert_eq!(burn(&mut img, &[r], crop), Ok(()));
        assert_eq!(px(&img, 30, 25), [0, 0, 0, 255]);
        assert!(
            near(px(&img, 30, 10), [0, 0, 255, 255], 2),
            "{:?}",
            px(&img, 30, 10)
        );
        assert!(all_opaque(&img));
    }

    #[test]
    fn arrow_head_filled() {
        assert_eq!(arrow_min_head(4.5), 18.0);
        let mut img = solid(100, 60, 0);
        let arrow = Shape::Arrow {
            a: pt(10.0, 30.0),
            b: pt(90.0, 30.0),
            width: 3.0,
            color: RED,
        };
        let crop = full(&img);
        assert_eq!(burn(&mut img, &[arrow], crop), Ok(()));
        // Trong tam giác, ngoài bề rộng của thân.
        assert!(
            near(px(&img, 82, 32), [0, 0, 255, 255], 2),
            "{:?}",
            px(&img, 82, 32)
        );
        assert!(
            near(px(&img, 84, 30), [0, 0, 255, 255], 2),
            "{:?}",
            px(&img, 84, 30)
        );
        // Thân.
        assert!(
            near(px(&img, 40, 30), [0, 0, 255, 255], 2),
            "{:?}",
            px(&img, 40, 30)
        );
        assert!(all_opaque(&img));
    }

    #[test]
    fn short_arrow_draws_head_only() {
        let mut img = solid(40, 20, 0);
        let arrow = Shape::Arrow {
            a: pt(20.0, 10.0),
            b: pt(25.0, 10.0),
            width: 3.0,
            color: RED,
        };
        let crop = full(&img);
        assert_eq!(burn(&mut img, &[arrow], crop), Ok(()));
        assert_eq!(px(&img, 17, 10), [0, 0, 0, 255]);
        // Không có đầu tròn của thân ở `a` (đầu tròn bán kính 1.5 sẽ phủ pixel 19).
        assert_eq!(px(&img, 19, 10), [0, 0, 0, 255]);
        // Đầu mũi tên vẫn có.
        assert!(px(&img, 23, 10)[2] > 200, "{:?}", px(&img, 23, 10));
    }

    #[test]
    fn marker_is_translucent() {
        let mut img = solid(60, 40, 0);
        let m = Shape::Marker {
            points: vec![pt(10.0, 20.0), pt(30.0, 22.0), pt(50.0, 20.0)],
            width: 16.0,
            color: Rgb(255, 255, 255),
        };
        let crop = full(&img);
        assert_eq!(burn(&mut img, &[m], crop), Ok(()));
        let p = px(&img, 30, 21);
        assert!(p[2].abs_diff(102) <= 6, "{p:?}");
        assert!(all_opaque(&img));
    }

    /// Pen và Marker có hai điểm trở lên trùng nhau vẽ như nét một điểm (chấm tròn, ô vuông),
    /// không làm `burn` lỗi.
    #[test]
    fn coincident_points_draw_a_dot() {
        let mut img = solid(50, 40, 0);
        let pen = Shape::Pen {
            points: vec![pt(10.0, 20.0); 3],
            width: 6.0,
            color: RED,
        };
        let marker = Shape::Marker {
            points: vec![pt(30.0, 20.0), pt(30.0, 20.0)],
            width: 8.0,
            color: Rgb(255, 255, 255),
        };
        let crop = full(&img);
        assert_eq!(burn(&mut img, &[pen, marker], crop), Ok(()));
        // Chấm tròn bán kính 3 ở (10, 20).
        assert!(
            near(px(&img, 10, 20), [0, 0, 255, 255], 2),
            "{:?}",
            px(&img, 10, 20)
        );
        assert_eq!(px(&img, 15, 20), [0, 0, 0, 255]);
        assert_eq!(px(&img, 10, 25), [0, 0, 0, 255]);
        // Ô vuông cạnh 8 quanh (30, 20): phủ 40% cả ở góc (ngoài hình tròn bán kính 4).
        let mid = px(&img, 30, 20);
        assert!(mid[2].abs_diff(102) <= 6, "{mid:?}");
        let corner = px(&img, 26, 16);
        assert!(corner[2].abs_diff(102) <= 6, "{corner:?}");
        assert_eq!(px(&img, 35, 20), [0, 0, 0, 255]);
        assert!(all_opaque(&img));
    }

    #[test]
    fn marker_self_overlap_not_darker() {
        let mut img = solid(80, 80, 255);
        // Hình X trong một nét: chéo xuống, lên thẳng, chéo xuống ngược lại.
        let m = Shape::Marker {
            points: vec![
                pt(10.0, 10.0),
                pt(70.0, 70.0),
                pt(70.0, 10.0),
                pt(10.0, 70.0),
            ],
            width: 16.0,
            color: Rgb(0, 0, 0),
        };
        let crop = full(&img);
        assert_eq!(burn(&mut img, &[m], crop), Ok(()));
        let cross = px(&img, 40, 40);
        let branch = px(&img, 25, 25);
        assert!(near(cross, branch, 3), "giao {cross:?} nhánh {branch:?}");
        assert!(branch[0] < 200, "nhánh không được vẽ: {branch:?}");
    }

    #[test]
    fn text_draws_outline_and_fill() {
        let _gp = GdiPlus::start();
        let mut img = solid(90, 80, 128);
        let t = Shape::Text {
            origin: pt(10.0, 10.0),
            text: "H".into(),
            size: 40.0,
            color: RED,
        };
        let b = shape_bounds(&t);
        let crop = full(&img);
        assert_eq!(burn(&mut img, std::slice::from_ref(&t), crop), Ok(()));
        let mut red = false;
        let mut black = false;
        for y in 0..img.height {
            for x in 0..img.width {
                let p = px(&img, x, y);
                if near(p, [0, 0, 255, 255], 2) {
                    red = true;
                }
                if near(p, [0, 0, 0, 255], 2) {
                    black = true;
                }
                if p != [128, 128, 128, 255] {
                    let (xi, yi) = (x as i32, y as i32);
                    assert!(
                        xi >= b.x && yi >= b.y && xi < b.right() && yi < b.bottom(),
                        "({x},{y}) ngoài {b:?}"
                    );
                }
            }
        }
        assert!(red && black, "đỏ {red}, đen {black}");
        assert!(all_opaque(&img));
    }

    fn sample_shapes() -> Vec<Shape> {
        vec![
            Shape::Pen {
                points: vec![pt(40.0, 30.0), pt(70.0, 50.0), pt(90.0, 35.0)],
                width: 3.0,
                color: PALETTE[3],
            },
            Shape::Pen {
                points: vec![pt(120.0, 60.0)],
                width: 4.5,
                color: PALETTE[4],
            },
            Shape::Line {
                a: pt(45.5, 75.25),
                b: pt(160.0, 25.0),
                width: 3.0,
                color: PALETTE[0],
            },
            Shape::Arrow {
                a: pt(60.0, 70.0),
                b: pt(140.0, 40.0),
                width: 4.5,
                color: PALETTE[1],
            },
            Shape::Rect {
                a: pt(55.0, 25.0),
                b: pt(130.0, 75.0),
                width: 3.0,
                color: PALETTE[5],
            },
            Shape::Marker {
                points: vec![pt(50.0, 50.0), pt(100.0, 60.0), pt(150.0, 30.0)],
                width: 16.0,
                color: PALETTE[2],
            },
            Shape::Blur {
                a: pt(85.3, 32.6),
                b: pt(115.8, 55.1),
                color: Rgb::hex(0x5A6B7C),
            },
            Shape::Text {
                origin: pt(70.0, 40.0),
                text: "Ab\nc".into(),
                size: 18.0,
                color: PALETTE[6],
            },
            Shape::Ellipse {
                a: pt(30.0, 20.0),
                b: pt(110.0, 80.0),
                width: 3.0,
                color: PALETTE[0],
            },
            Shape::Step {
                center: pt(95.0, 45.0),
                radius: 14.0,
                n: 3,
                color: PALETTE[4],
            },
        ]
    }

    #[test]
    fn burn_offsets_by_crop_origin() {
        let shapes = sample_shapes();
        let base = noise(200, 100, 3);
        let crop = Rect {
            x: 50,
            y: 20,
            w: 100,
            h: 60,
        };
        let mut whole = base.clone();
        assert_eq!(burn(&mut whole, &shapes, full(&base)), Ok(()));
        let expected = whole.crop(crop);
        let mut part = base.crop(crop);
        assert_eq!(burn(&mut part, &shapes, crop), Ok(()));
        assert_eq!(part, expected);
        assert_ne!(part, base.crop(crop));
    }

    #[test]
    fn clip_limits_drawing() {
        let _gp = GdiPlus::start();
        let shapes = sample_shapes();
        let base = noise(200, 100, 5);
        let clip = Rect {
            x: 80,
            y: 30,
            w: 30,
            h: 20,
        };
        let mut img = base.clone();
        let mut ok = false;
        assert_eq!(
            with_dib(&mut img, |dc| {
                ok = draw_shapes(dc, &shapes, 0, 0, clip);
                ok
            }),
            Ok(())
        );
        assert!(ok);
        let mut changed_inside = false;
        for y in 0..img.height {
            for x in 0..img.width {
                let inside = clip.contains(darkshot_core::geometry::Point {
                    x: x as i32,
                    y: y as i32,
                });
                let (a, b) = (px(&img, x, y), px(&base, x, y));
                if inside {
                    changed_inside |= a != b;
                } else {
                    assert_eq!(a, b, "({x},{y}) ngoài clip bị đổi");
                }
            }
        }
        assert!(changed_inside);
    }

    /// Ảnh caro đen trắng 1 px (giả chữ), alpha 255.
    fn checker(w: u32, h: u32) -> Image {
        let mut bgra = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let v = if (x + y) % 2 == 0 { 0 } else { 255 };
                bgra.extend_from_slice(&[v, v, v, 255]);
            }
        }
        Image {
            width: w,
            height: h,
            bgra,
        }
    }

    #[test]
    fn burn_blur_covers_every_pixel() {
        use darkshot_core::annotate::blur_pixel_rect;
        let base = checker(64, 48);
        let mut img = base.clone();
        let (a, b) = (pt(8.4, 6.6), pt(40.2, 30.9));
        let color = Rgb::hex(0x5A6B7C);
        let r = blur_pixel_rect(a, b);
        assert_eq!(
            r,
            Rect {
                x: 8,
                y: 6,
                w: 33,
                h: 25
            }
        );
        assert_eq!(
            burn(&mut img, &[Shape::Blur { a, b, color }], full(&base)),
            Ok(())
        );
        for y in 0..img.height {
            for x in 0..img.width {
                let inside = r.contains(darkshot_core::geometry::Point {
                    x: x as i32,
                    y: y as i32,
                });
                if inside {
                    assert_eq!(px(&img, x, y), [0x7C, 0x6B, 0x5A, 255], "({x},{y})");
                } else {
                    assert_eq!(px(&img, x, y), px(&base, x, y), "({x},{y}) ngoài vùng che");
                }
            }
        }
    }

    #[test]
    fn burn_blur_then_pen_order() {
        let base = solid(60, 40, 0);
        let color = Rgb::hex(0x5A6B7C);
        let blur = Shape::Blur {
            a: pt(10.0, 10.0),
            b: pt(50.0, 30.0),
            color,
        };
        let pen = Shape::Pen {
            points: vec![pt(5.0, 20.0), pt(55.0, 20.0)],
            width: 3.0,
            color: RED,
        };
        let cover = [0x7C, 0x6B, 0x5A, 255];
        // Pen trước rồi Blur: điểm của Pen trong vùng che bị phủ.
        let mut img = base.clone();
        assert_eq!(
            burn(&mut img, &[pen.clone(), blur.clone()], full(&base)),
            Ok(())
        );
        assert_eq!(px(&img, 30, 20), cover);
        assert_eq!(px(&img, 6, 20)[2], 255, "nét ngoài vùng che vẫn còn");
        // Blur trước rồi Pen: nét Pen nằm trên.
        let mut img = base.clone();
        assert_eq!(burn(&mut img, &[blur, pen], full(&base)), Ok(()));
        assert_eq!(px(&img, 30, 20), [0, 0, 255, 255]);
        assert_eq!(px(&img, 30, 12), cover);
    }

    #[test]
    fn burn_no_shapes_unchanged() {
        let mut img = checker(33, 17);
        let before = img.clone();
        assert_eq!(burn(&mut img, &[], full(&before)), Ok(()));
        assert_eq!(img, before);
    }

    #[test]
    fn caret_follows_line_and_column() {
        let _gp = GdiPlus::start();
        let mut e = TextEdit::new(pt(20.0, 30.0), 18.0, RED);
        let c0 = caret_rect(&e);
        assert_eq!((c0.x, c0.y), (20, 30));
        assert!(c0.h >= 18, "{c0:?}");
        e.insert("Hello");
        let c1 = caret_rect(&e);
        assert!(c1.x > c0.x + 20, "{c1:?}");
        assert_eq!(c1.y, c0.y);
        e.insert("\n");
        let c2 = caret_rect(&e);
        assert_eq!(c2.x, c0.x);
        assert!(c2.y >= c0.y + 18, "{c2:?}");
        // Hộp bao của chữ chứa con trỏ ở cuối dòng đầu.
        e.insert("x");
        let b = shape_bounds(&e.to_shape().unwrap());
        assert!(
            b.x <= c1.x && b.right() >= c1.x && b.y <= c0.y,
            "{b:?} {c1:?}"
        );
    }

    #[test]
    fn non_text_bounds_match_core() {
        for s in sample_shapes() {
            if let Some(b) = s.bounds() {
                assert_eq!(shape_bounds(&s), b);
            }
        }
    }

    /// Mọi điểm ảnh khác `base` nằm trong `b`.
    fn changed_within(img: &Image, base: &Image, b: Rect) {
        for y in 0..img.height {
            for x in 0..img.width {
                if px(img, x, y) != px(base, x, y) {
                    let (xi, yi) = (x as i32, y as i32);
                    assert!(
                        xi >= b.x && yi >= b.y && xi < b.right() && yi < b.bottom(),
                        "({x},{y}) ngoài {b:?}"
                    );
                }
            }
        }
    }

    fn rgb_px(c: Rgb) -> [u8; 4] {
        [c.2, c.1, c.0, 255]
    }

    #[test]
    fn ellipse_drawn_inside_bounds() {
        let base = solid(80, 60, 255);
        let mut img = base.clone();
        let e = Shape::Ellipse {
            a: pt(10.0, 10.0),
            b: pt(70.0, 50.0),
            width: 3.0,
            color: RED,
        };
        assert_eq!(
            burn(&mut img, std::slice::from_ref(&e), full(&base)),
            Ok(())
        );
        assert_ne!(px(&img, 10, 30), [255, 255, 255, 255], "mép trái");
        assert_eq!(px(&img, 40, 30), [255, 255, 255, 255], "tâm");
        changed_within(&img, &base, shape_bounds(&e));
        assert!(all_opaque(&img));
    }

    #[test]
    fn ellipse_circle_symmetric() {
        let base = solid(160, 160, 255);
        let mut img = base.clone();
        let e = Shape::Ellipse {
            a: pt(20.0, 20.0),
            b: pt(140.0, 140.0),
            width: 3.0,
            color: RED,
        };
        assert_eq!(burn(&mut img, &[e], full(&base)), Ok(()));
        // Đếm theo bốn góc phần tư quanh tâm (80, 80), bỏ hai trục.
        let mut q = [0u32; 4];
        for y in 0..img.height {
            for x in 0..img.width {
                if x == 80 || y == 80 || px(&img, x, y) == px(&base, x, y) {
                    continue;
                }
                q[usize::from(x > 80) + 2 * usize::from(y > 80)] += 1;
            }
        }
        let (lo, hi) = (*q.iter().min().unwrap(), *q.iter().max().unwrap());
        assert!(lo > 0, "{q:?}");
        assert!(
            f64::from(hi - lo) <= 0.02 * f64::from(hi),
            "lệch quá 2%: {q:?}"
        );
    }

    #[test]
    fn step_fill_ring_and_number() {
        let base = solid(80, 80, 128);
        let mut img = base.clone();
        let (cx, cy, r) = (40.0f32, 40.0f32, 20.0f32);
        let s = Shape::Step {
            center: pt(cx, cy),
            radius: r,
            n: 1,
            color: RED,
        };
        assert_eq!(
            burn(&mut img, std::slice::from_ref(&s), full(&base)),
            Ok(())
        );
        let fill = px(&img, (cx + 0.6 * r) as u32, cy as u32);
        assert!(near(fill, [0, 0, 255, 255], 2), "lòng {fill:?}");
        let ink = rgb_px(outline_color(RED));
        let edge_x = (cx + r - r * STEP_RING / 2.0).round() as u32;
        let ring = px(&img, edge_x, cy as u32);
        assert!(near(ring, ink, 40), "viền ({edge_x}) {ring:?}");
        // Số: ít nhất một điểm màu chữ trong hình vuông cạnh r quanh tâm.
        let half = (r / 2.0) as u32;
        let mut digit = false;
        for y in cy as u32 - half..cy as u32 + half {
            for x in cx as u32 - half..cx as u32 + half {
                digit |= near(px(&img, x, y), ink, 2);
            }
        }
        assert!(digit, "không thấy số");
        changed_within(&img, &base, shape_bounds(&s));
        assert!(all_opaque(&img));
    }

    #[test]
    fn step_number_fits() {
        let base = solid(60, 60, 128);
        let mut img = base.clone();
        let (cx, cy, r) = (30.0f32, 30.0f32, 12.0f32);
        let s = Shape::Step {
            center: pt(cx, cy),
            radius: r,
            n: 888,
            color: RED,
        };
        assert_eq!(burn(&mut img, &[s], full(&base)), Ok(()));
        let ink = rgb_px(outline_color(RED));
        let mut seen = 0;
        for y in 0..img.height {
            for x in 0..img.width {
                if near(px(&img, x, y), ink, 10) {
                    seen += 1;
                    let d = (x as f32 - cx).hypot(y as f32 - cy);
                    assert!(d <= r + 0.5, "({x},{y}) cách tâm {d} > {r}");
                }
            }
        }
        assert!(seen > 0);
    }

    /// Có điểm ảnh gần `want` trong hình vuông cạnh r quanh tâm.
    fn number_has(img: &Image, c: (u32, u32), r: u32, want: [u8; 4]) -> bool {
        let half = r / 2;
        (c.1 - half..c.1 + half)
            .any(|y| (c.0 - half..c.0 + half).any(|x| near(px(img, x, y), want, 10)))
    }

    #[test]
    fn step_readable_on_light_and_dark() {
        for (color, want) in [
            (Rgb::hex(0xFFFF00), [0, 0, 0, 255]),
            (Rgb::hex(0x000080), [255, 255, 255, 255]),
        ] {
            assert_eq!(rgb_px(outline_color(color)), want);
            let base = solid(80, 80, 128);
            let mut img = base.clone();
            let s = Shape::Step {
                center: pt(40.0, 40.0),
                radius: 20.0,
                n: 7,
                color,
            };
            assert_eq!(burn(&mut img, &[s], full(&base)), Ok(()));
            assert!(
                number_has(&img, (40, 40), 20, want),
                "{color:?}: không thấy số màu {want:?}"
            );
        }
    }

    #[test]
    fn step_zero_has_no_number() {
        let base = solid(80, 80, 128);
        let mut img = base.clone();
        let s = Shape::Step {
            center: pt(40.0, 40.0),
            radius: 20.0,
            n: 0,
            color: RED,
        };
        assert_eq!(burn(&mut img, &[s], full(&base)), Ok(()));
        assert!(!number_has(&img, (40, 40), 20, rgb_px(outline_color(RED))));
        assert!(near(px(&img, 40, 40), [0, 0, 255, 255], 2));
    }

    #[test]
    fn screen_matches_burn_for_new_shapes() {
        let shapes = vec![
            Shape::Ellipse {
                a: pt(15.5, 12.25),
                b: pt(90.0, 70.0),
                width: 4.5,
                color: PALETTE[1],
            },
            Shape::Step {
                center: pt(60.0, 45.0),
                radius: 18.0,
                n: 12,
                color: PALETTE[3],
            },
        ];
        let base = noise(120, 90, 11);
        let mut burned = base.clone();
        assert_eq!(burn(&mut burned, &shapes, full(&base)), Ok(()));
        let _gp = GdiPlus::start();
        let mut screen = base.clone();
        let mut ok = false;
        assert_eq!(
            with_dib(&mut screen, |dc| {
                ok = draw_shapes(dc, &shapes, 0, 0, full(&base));
                ok
            }),
            Ok(())
        );
        assert!(ok);
        assert!(screen == burned, "draw_shapes khác burn");
        assert!(screen != base, "không vẽ gì");
    }

    /// `burn` nhiều lần đủ loại hình: số đối tượng GDI của tiến trình không tăng.
    /// Bỏ qua mặc định vì kiểm thử khác chạy song song cũng tạo đối tượng GDI.
    #[test]
    #[ignore = "đếm GDI của cả tiến trình; chạy tay: cargo test -p darkshot-windows -- --ignored render_no_gdi_leak --test-threads=1"]
    fn render_no_gdi_leak() {
        use crate::capture::GDI_TEST_LOCK;
        use windows::Win32::System::Threading::{
            GetCurrentProcess, GetGuiResources, GR_GDIOBJECTS,
        };
        let _gdi = GDI_TEST_LOCK.lock();
        // SAFETY: chỉ đọc số đối tượng GDI của tiến trình này.
        let gdi = || unsafe { GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS) };
        let shapes = sample_shapes();
        let base = noise(200, 100, 9);
        let crop = Rect {
            x: 10,
            y: 5,
            w: 180,
            h: 90,
        };
        let mut warm = 0;
        for i in 1..=200 {
            let mut img = base.crop(crop);
            assert_eq!(burn(&mut img, &shapes, crop), Ok(()));
            if i == 2 {
                warm = gdi();
            }
        }
        let last = gdi();
        assert!(last <= warm, "GDI tăng từ {warm} lên {last}");
    }
}
