//! Bảng thuộc tính của Hình (khung, elip) và Chữ trên lớp phủ macOS (Task 112): kiểu mặc định
//! cho hình vẽ tiếp theo, bước tăng giảm độ dày và độ trong suốt, bố cục các ô của bảng và phép
//! thử trúng. Thuần logic (không gọi Objective-C) nên kiểm thử được không cần GUI.
//!
//! Bảng hiện cạnh thanh dọc khi công cụ Hình chữ nhật, Elip hoặc Chữ đang chọn, hoặc khi đang chọn
//! một hình thuộc loại đó (không cần nút riêng trên thanh dọc, để không đổi bố cục dùng chung với
//! Windows). Bảng dùng lại 48 màu của `PALETTE` cho ô màu.

use darkshot_core::annotate::{
    dip_to_px, Align, HighlightMode, Style, DEFAULT_COLOR, PALETTE, STROKE_DIP,
};
use darkshot_core::geometry::{Point, Rect};
use darkshot_core::overlay_layout::{clamp_into, intersect, scaled, side_on_right, GAP};
use darkshot_core::theme::Rgb;

/// Bảng đang sửa loại hình nào.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleKind {
    /// Hình chữ nhật: nền, viền (hoặc không viền), bo góc, độ trong suốt; màu viền là màu của nút
    /// Màu.
    Box,
    /// Elip: như `Box` nhưng không có bo góc (Task 114).
    Oval,
    /// Chữ: màu chữ, nền, màu viền, độ dày viền, độ trong suốt, căn lề; màu chữ đồng bộ với nút Màu.
    Label,
    /// Highlight chế độ tự do (nét Bút dạ quang): màu, độ to nhỏ nét, độ trong suốt (Task 113, 114).
    MarkerFree,
    /// Highlight chế độ hình chữ nhật: màu, độ trong suốt, không viền (Task 113).
    MarkerRect,
}

impl StyleKind {
    /// Có hàng Bo góc (Hình chữ nhật và Chữ).
    pub fn has_radius(self) -> bool {
        matches!(self, StyleKind::Box | StyleKind::Label)
    }

    /// Có ô tích Không viền (Hình và Chữ).
    pub fn has_border(self) -> bool {
        !self.is_highlight()
    }

    /// Bảng Highlight (có hàng chọn chế độ thay hàng Nền/Viền).
    pub fn is_highlight(self) -> bool {
        matches!(self, StyleKind::MarkerFree | StyleKind::MarkerRect)
    }
}

/// Ô màu của bảng đang áp cho đích nào: nền, viền, hoặc (chỉ Chữ) màu chữ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleTarget {
    Fill,
    Border,
    /// Màu chữ của khung chữ (cùng màu với nút Màu của thanh công cụ).
    Text,
}

impl StyleTarget {
    /// Đích thực sự dùng cho bảng `kind`: Hình không có màu chữ nên `Text` thành `Fill`.
    pub fn for_kind(self, kind: StyleKind) -> StyleTarget {
        if self == StyleTarget::Text && kind != StyleKind::Label {
            StyleTarget::Fill
        } else {
            self
        }
    }
}

/// Một ô bấm được của bảng.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleCell {
    Target(StyleTarget),
    NoFill,
    /// Chuyển chế độ Highlight cho lần vẽ tiếp (không đổi nét đã vẽ).
    Mode(HighlightMode),
    /// Chỉ số trong `PALETTE`.
    Color(usize),
    /// -1 giảm, +1 tăng.
    Width(i32),
    Opacity(i32),
    Align(Align),
    /// Ô tích Không viền (Task 114).
    NoBorder,
    /// Bo góc: -1 giảm, +1 tăng (px ảnh).
    Radius(i32),
    /// Nút thu gọn/mở bảng ở thanh tiêu đề.
    Collapse,
    /// Thanh tiêu đề: nhấn giữ để kéo bảng.
    Header,
}

/// Một thay đổi kiểu từ bảng.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StyleEdit {
    Fill(Option<Rgb>),
    Stroke(Rgb),
    WidthDip(f32),
    /// 0..=1.
    Opacity(f32),
    Align(Align),
    NoBorder(bool),
    /// Bo góc (px ảnh).
    RadiusPx(f32),
}

/// Bo góc lớn nhất bảng cho chọn (px ảnh); bước 2 px. Khi vẽ còn kẹp theo nửa cạnh ngắn của hộp.
pub const RADIUS_MAX_PX: f32 = 100.0;
pub const RADIUS_STEP_PX: f32 = 2.0;

/// Độ dày viền nhỏ nhất và lớn nhất (dip) bảng cho chọn; bước 1 dip.
pub const WIDTH_MIN_DIP: f32 = 0.0;
pub const WIDTH_MAX_DIP: f32 = darkshot_core::annotate::STYLE_WIDTH_MAX_DIP;
/// Độ dày nét Highlight tự do nhỏ nhất và lớn nhất (dip) bảng cho chọn; bước 2 dip.
pub const MARKER_WIDTH_MIN_DIP: f32 = 4.0;
pub const MARKER_WIDTH_MAX_DIP: f32 = 48.0;
/// Độ trong suốt nhỏ nhất bảng cho chọn (0 là vô hình); bước 10%.
pub const OPACITY_MIN: f32 = 0.1;

/// Kiểu mặc định cho hình vẽ tiếp theo, đơn vị dip (độc lập DPI).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StylePrefs {
    pub fill: Option<Rgb>,
    /// Màu viền riêng (chỉ Chữ dùng; viền Hình theo màu của nút Màu).
    pub stroke: Rgb,
    pub width_dip: f32,
    pub opacity: f32,
    /// Không viền (giữ `width_dip` và màu để khi tắt viền trở lại như cũ).
    pub no_border: bool,
    /// Bo góc (px ảnh, không theo DPI).
    pub radius_px: f32,
}

/// Hình chữ nhật, elip: không nền, đặc, viền như mức cỡ Vừa.
pub const BOX_DEFAULT: StylePrefs = StylePrefs {
    fill: None,
    stroke: DEFAULT_COLOR,
    width_dip: STROKE_DIP,
    opacity: 1.0,
    no_border: false,
    radius_px: 0.0,
};

/// Chữ: không nền, không viền, đặc (như chữ trước đây).
pub const LABEL_DEFAULT: StylePrefs = StylePrefs {
    fill: None,
    stroke: DEFAULT_COLOR,
    width_dip: 0.0,
    opacity: 1.0,
    no_border: false,
    radius_px: 0.0,
};

impl StylePrefs {
    /// Kiểu cụ thể ở `dpi` với màu viền `stroke` (đã kẹp).
    pub fn style(&self, stroke: Rgb, dpi: u32) -> Style {
        Style {
            fill: self.fill,
            stroke,
            stroke_width: dip_to_px(self.width_dip, dpi),
            opacity: self.opacity,
            radius: clamp_radius_px(self.radius_px),
            no_border: self.no_border,
        }
        .sanitized()
    }

    /// Áp một thay đổi (độ dày và độ trong suốt được kẹp vào khoảng của bảng).
    pub fn apply(&mut self, e: StyleEdit) {
        match e {
            StyleEdit::Fill(f) => self.fill = f,
            StyleEdit::Stroke(c) => self.stroke = c,
            StyleEdit::WidthDip(w) => self.width_dip = clamp_width(w),
            StyleEdit::Opacity(o) => self.opacity = clamp_opacity(o),
            StyleEdit::Align(_) => {}
            StyleEdit::NoBorder(b) => self.no_border = b,
            StyleEdit::RadiusPx(r) => self.radius_px = clamp_radius_px(r),
        }
    }
}

/// Bo góc (px) kẹp vào 0..=[`RADIUS_MAX_PX`]; không phải số thì 0.
pub fn clamp_radius_px(r: f32) -> f32 {
    if r.is_nan() {
        0.0
    } else {
        r.clamp(0.0, RADIUS_MAX_PX)
    }
}

/// Bước bo góc: `dir` = -1 hoặc +1, mỗi bước [`RADIUS_STEP_PX`] px, kẹp.
pub fn step_radius(px: f32, dir: i32) -> f32 {
    clamp_radius_px(px.round() + RADIUS_STEP_PX * dir as f32)
}

/// Trạng thái các ô đích màu của bảng (Task 114, mở rộng Task 115): đúng MỘT ô sáng tại một thời
/// điểm. Chữ có ba đích (Màu chữ, Nền, Viền), Hình có hai (Nền, Viền). Đích Nền sáng khi đang có
/// màu nền; chưa có màu nền thì `Không nền` sáng thay cho `Nền` (đúng một trong hai).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChipFocus {
    pub text: bool,
    pub fill: bool,
    pub border: bool,
    pub no_fill: bool,
}

pub fn chip_focus(kind: StyleKind, target: StyleTarget, fill: Option<Rgb>) -> ChipFocus {
    let target = target.for_kind(kind);
    ChipFocus {
        text: target == StyleTarget::Text,
        fill: target == StyleTarget::Fill && fill.is_some(),
        border: target == StyleTarget::Border,
        no_fill: target == StyleTarget::Fill && fill.is_none(),
    }
}

/// Độ dày (dip) kẹp vào [`WIDTH_MIN_DIP`, `WIDTH_MAX_DIP`]; không phải số thì mức nhỏ nhất.
pub fn clamp_width(w: f32) -> f32 {
    if w.is_nan() {
        WIDTH_MIN_DIP
    } else {
        w.clamp(WIDTH_MIN_DIP, WIDTH_MAX_DIP)
    }
}

/// Độ dày nét Highlight (dip) kẹp vào [`MARKER_WIDTH_MIN_DIP`, `MARKER_WIDTH_MAX_DIP`]; không phải
/// số thì mức nhỏ nhất.
pub fn clamp_marker_width(w: f32) -> f32 {
    if w.is_nan() {
        MARKER_WIDTH_MIN_DIP
    } else {
        w.clamp(MARKER_WIDTH_MIN_DIP, MARKER_WIDTH_MAX_DIP)
    }
}

/// Bước độ dày nét Highlight: `dir` = -1 hoặc +1, mỗi bước 2 dip, kẹp.
pub fn step_marker_width(dip: f32, dir: i32) -> f32 {
    clamp_marker_width(dip.round() + 2.0 * dir as f32)
}

/// Độ trong suốt kẹp vào [`OPACITY_MIN`, 1]; không phải số thì 1.
pub fn clamp_opacity(o: f32) -> f32 {
    if o.is_nan() {
        1.0
    } else {
        o.clamp(OPACITY_MIN, 1.0)
    }
}

/// Bước độ dày: `dir` = -1 hoặc +1, làm tròn về số nguyên dip rồi kẹp.
pub fn step_width(dip: f32, dir: i32) -> f32 {
    clamp_width(dip.round() + dir as f32)
}

/// Bước độ trong suốt: bội của 10% gần nhất cộng `dir` bước, kẹp.
pub fn step_opacity(o: f32, dir: i32) -> f32 {
    let tenths = (o * 10.0).round() + dir as f32;
    clamp_opacity(tenths / 10.0)
}

/// Phần trăm độ trong suốt để hiện (0..=100).
pub fn opacity_percent(o: f32) -> u32 {
    (clamp_opacity(o) * 100.0).round() as u32
}

/// Giá trị bảng đang hiện: của hình đang chọn nếu có, không thì kiểu mặc định.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StyleView {
    pub kind: StyleKind,
    pub fill: Option<Rgb>,
    pub stroke: Rgb,
    /// Màu chữ (chỉ Chữ; Hình lấy màu của nút Màu).
    pub text: Rgb,
    pub width_dip: f32,
    pub opacity: f32,
    pub align: Option<Align>,
    /// Chế độ Highlight sẽ dùng cho lần vẽ tiếp (chỉ bảng Highlight).
    pub mode: Option<HighlightMode>,
    pub no_border: bool,
    /// Bo góc (px ảnh).
    pub radius_px: f32,
}

// ---------------------------------------------------------------- bố cục (đơn vị 96 DPI)

const PAD: i32 = 8;
const CELL: i32 = 22;
const CELL_GAP: i32 = 2;
const COLS: i32 = 8;
const ROWS: i32 = 6;
const ROW_H: i32 = 24;
const ROW_GAP: i32 = 6;
const SECTION_GAP: i32 = 8;
/// Khe giữa hàng ô đích màu và lưới màu ngay dưới (gần hơn các khe khác để lưới thuộc về hàng ô).
const CHIP_GAP: i32 = 4;
const BTN: i32 = 24;
const VALUE_W: i32 = 44;
/// Thanh tiêu đề (kéo bảng, nút thu gọn).
const HEAD_H: i32 = 20;
const HEAD_BTN: i32 = 20;

const NONE: Rect = Rect {
    x: 0,
    y: 0,
    w: 0,
    h: 0,
};

/// Bố cục các ô của bảng đặt ở `panel`. Ô nào bảng không có (hoặc đang thu gọn) là `Rect` rộng 0.
#[derive(Clone, Debug, PartialEq)]
pub struct StyleLayout {
    pub panel: Rect,
    /// Thanh tiêu đề: kéo để dời bảng.
    pub header: Rect,
    /// Nút thu gọn/mở nằm trong thanh tiêu đề.
    pub collapse: Rect,
    pub collapsed: bool,
    /// Ô đích Màu chữ (chỉ Chữ; Hình: rộng 0).
    pub text_chip: Rect,
    pub fill_chip: Rect,
    pub border_chip: Rect,
    pub no_fill: Rect,
    /// Hai ô chọn chế độ (chỉ bảng Highlight; khi đó ba ô trên không có, kích thước 0).
    pub modes: Option<[Rect; 2]>,
    pub kind: StyleKind,
    pub cells: Vec<Rect>,
    pub width_label: Rect,
    pub width_minus: Rect,
    pub width_value: Rect,
    pub width_plus: Rect,
    /// Ô tích Không viền (Hình, Chữ).
    pub no_border: Rect,
    /// Hàng Bo góc (Hình chữ nhật, Chữ).
    pub radius_label: Rect,
    pub radius_minus: Rect,
    pub radius_value: Rect,
    pub radius_plus: Rect,
    pub opacity_label: Rect,
    pub opacity_minus: Rect,
    pub opacity_value: Rect,
    pub opacity_plus: Rect,
    /// Ba ô căn lề (chỉ khi `StyleKind::Label`).
    pub aligns: Option<[Rect; 3]>,
}

/// Chiều cao (96 DPI) của bảng `kind` khi mở rộng hoặc `collapsed` (chỉ thanh tiêu đề).
fn panel_h(kind: StyleKind, collapsed: bool) -> i32 {
    if collapsed {
        return HEAD_H;
    }
    let mut h = HEAD_H + SECTION_GAP;
    h += ROW_H + CHIP_GAP + ROWS * CELL + (ROWS - 1) * CELL_GAP;
    h += SECTION_GAP;
    // Độ dày viền (hoặc độ to nét), rồi độ trong suốt.
    h += ROW_H + ROW_GAP + ROW_H;
    if kind.has_border() {
        h += ROW_GAP + ROW_H;
    }
    if kind.has_radius() {
        h += ROW_GAP + ROW_H;
    }
    if kind == StyleKind::Label {
        h += ROW_GAP + ROW_H;
    }
    h + PAD
}

fn panel_w() -> i32 {
    2 * PAD + COLS * CELL + (COLS - 1) * CELL_GAP
}

/// Kích thước (rộng, cao) của bảng mở rộng ở `dpi`.
#[cfg(test)]
pub fn panel_size(kind: StyleKind, dpi: u32) -> (i32, i32) {
    panel_size_ex(kind, false, dpi)
}

/// Kích thước của bảng, thu gọn (chỉ thanh tiêu đề) hoặc mở rộng.
pub fn panel_size_ex(kind: StyleKind, collapsed: bool, dpi: u32) -> (i32, i32) {
    (
        scaled(panel_w(), dpi),
        scaled(panel_h(kind, collapsed), dpi),
    )
}

/// Như `scaled` nhưng 0 vẫn là 0 (`scaled` không trả dưới 1).
fn sc(v: i32, dpi: u32) -> i32 {
    if v == 0 {
        0
    } else {
        scaled(v, dpi)
    }
}

fn rect(x: i32, y: i32, w: i32, h: i32, dpi: u32) -> Rect {
    Rect {
        x: sc(x, dpi),
        y: sc(y, dpi),
        w: sc(w, dpi),
        h: sc(h, dpi),
    }
}

fn shift(r: Rect, o: Rect) -> Rect {
    if r.w == 0 && r.h == 0 {
        return NONE;
    }
    Rect {
        x: r.x + o.x,
        y: r.y + o.y,
        ..r
    }
}

/// Bề rộng bốn ô đích của bảng Chữ (96 DPI): Màu chữ, Nền, Viền, Không nền; vừa kín `inner`.
fn chip_widths(inner: i32) -> [i32; 4] {
    let avail = inner - 3 * 4;
    let (text, fill, border) = (avail * 30 / 100, avail * 18 / 100, avail * 20 / 100);
    [text, fill, border, avail - text - fill - border]
}

/// Bố cục của bảng `kind` mở rộng có góc trên trái ở `panel` (cỡ `panel_size`).
#[cfg(test)]
pub fn layout(panel: Rect, kind: StyleKind, dpi: u32) -> StyleLayout {
    layout_ex(panel, kind, false, dpi)
}

/// Bố cục của bảng `kind`; `collapsed` thì chỉ có thanh tiêu đề và nút thu gọn.
pub fn layout_ex(panel: Rect, kind: StyleKind, collapsed: bool, dpi: u32) -> StyleLayout {
    let inner = COLS * CELL + (COLS - 1) * CELL_GAP;
    let at = |r: Rect| shift(r, panel);
    let header = rect(0, 0, panel_w(), HEAD_H, dpi);
    let collapse = rect(panel_w() - HEAD_BTN, 0, HEAD_BTN, HEAD_H, dpi);
    let mut l = StyleLayout {
        panel,
        header: at(header),
        collapse: at(collapse),
        collapsed,
        text_chip: NONE,
        fill_chip: NONE,
        border_chip: NONE,
        no_fill: NONE,
        modes: None,
        kind,
        cells: Vec::new(),
        width_label: NONE,
        width_minus: NONE,
        width_value: NONE,
        width_plus: NONE,
        no_border: NONE,
        radius_label: NONE,
        radius_minus: NONE,
        radius_value: NONE,
        radius_plus: NONE,
        opacity_label: NONE,
        opacity_minus: NONE,
        opacity_value: NONE,
        opacity_plus: NONE,
        aligns: None,
    };
    if collapsed {
        return l;
    }
    let mut y = HEAD_H + SECTION_GAP;
    let chip_w = (inner - 2 * 4) / 3;
    if kind.is_highlight() {
        let mode_w = (inner - 4) / 2;
        l.modes = Some([0, 1].map(|i| at(rect(PAD + i * (mode_w + 4), y, mode_w, ROW_H, dpi))));
    } else if kind == StyleKind::Label {
        // Chữ: Màu chữ, Nền, Viền, Không nền (rộng theo độ dài nhãn), cùng một hàng.
        let widths = chip_widths(inner);
        let mut x = PAD;
        let mut chip = |w: i32| {
            let r = at(rect(x, y, w, ROW_H, dpi));
            x += w + 4;
            r
        };
        l.text_chip = chip(widths[0]);
        l.fill_chip = chip(widths[1]);
        l.border_chip = chip(widths[2]);
        l.no_fill = chip(widths[3]);
    } else {
        let chip = |i: i32| at(rect(PAD + i * (chip_w + 4), y, chip_w, ROW_H, dpi));
        (l.fill_chip, l.border_chip, l.no_fill) = (chip(0), chip(1), chip(2));
    }
    y += ROW_H + CHIP_GAP;
    l.cells = (0..PALETTE.len() as i32)
        .map(|i| {
            let (c, r) = (i % COLS, i / COLS);
            at(rect(
                PAD + c * (CELL + CELL_GAP),
                y + r * (CELL + CELL_GAP),
                CELL,
                CELL,
                dpi,
            ))
        })
        .collect();
    y += ROWS * CELL + (ROWS - 1) * CELL_GAP + SECTION_GAP;
    let label_w = inner - 2 * BTN - VALUE_W;
    let stepper = |y: i32| {
        (
            at(rect(PAD, y, label_w, ROW_H, dpi)),
            at(rect(PAD + label_w, y, BTN, ROW_H, dpi)),
            at(rect(PAD + label_w + BTN, y, VALUE_W, ROW_H, dpi)),
            at(rect(PAD + label_w + BTN + VALUE_W, y, BTN, ROW_H, dpi)),
        )
    };
    (l.width_label, l.width_minus, l.width_value, l.width_plus) = stepper(y);
    y += ROW_H + ROW_GAP;
    if kind.has_border() {
        l.no_border = at(rect(PAD, y, inner, ROW_H, dpi));
        y += ROW_H + ROW_GAP;
    }
    if kind.has_radius() {
        (
            l.radius_label,
            l.radius_minus,
            l.radius_value,
            l.radius_plus,
        ) = stepper(y);
        y += ROW_H + ROW_GAP;
    }
    (
        l.opacity_label,
        l.opacity_minus,
        l.opacity_value,
        l.opacity_plus,
    ) = stepper(y);
    y += ROW_H + ROW_GAP;
    if kind == StyleKind::Label {
        let w = (inner - 2 * 4) / 3;
        l.aligns = Some([0, 1, 2].map(|i| at(rect(PAD + i * (w + 4), y, w, ROW_H, dpi))));
    }
    l
}

/// Ô dưới điểm `p`; lề, nhãn và giá trị trả `None`.
pub fn cell_at(l: &StyleLayout, p: Point) -> Option<StyleCell> {
    if !l.panel.contains(p) {
        return None;
    }
    if l.collapse.contains(p) {
        return Some(StyleCell::Collapse);
    }
    if l.header.contains(p) {
        return Some(StyleCell::Header);
    }
    if l.collapsed {
        return None;
    }
    if let Some(m) = &l.modes {
        if let Some(i) = m.iter().position(|r| r.contains(p)) {
            return Some(StyleCell::Mode(HighlightMode::ALL[i]));
        }
    }
    if l.text_chip.contains(p) {
        return Some(StyleCell::Target(StyleTarget::Text));
    }
    if l.fill_chip.contains(p) {
        return Some(StyleCell::Target(StyleTarget::Fill));
    }
    if l.border_chip.contains(p) {
        return Some(StyleCell::Target(StyleTarget::Border));
    }
    if l.no_fill.contains(p) {
        return Some(StyleCell::NoFill);
    }
    if let Some(i) = l.cells.iter().position(|c| c.contains(p)) {
        return Some(StyleCell::Color(i));
    }
    // Highlight hình chữ nhật không có độ dày nét.
    let use_width = l.kind != StyleKind::MarkerRect;
    if use_width && l.width_minus.contains(p) {
        return Some(StyleCell::Width(-1));
    }
    if use_width && l.width_plus.contains(p) {
        return Some(StyleCell::Width(1));
    }
    if l.no_border.contains(p) {
        return Some(StyleCell::NoBorder);
    }
    if l.radius_minus.contains(p) {
        return Some(StyleCell::Radius(-1));
    }
    if l.radius_plus.contains(p) {
        return Some(StyleCell::Radius(1));
    }
    if l.opacity_minus.contains(p) {
        return Some(StyleCell::Opacity(-1));
    }
    if l.opacity_plus.contains(p) {
        return Some(StyleCell::Opacity(1));
    }
    let aligns = l.aligns.as_ref()?;
    Align::ALL
        .iter()
        .zip(aligns)
        .find(|(_, r)| r.contains(p))
        .map(|(a, _)| StyleCell::Align(*a))
}

// ---------------------------------------------------------------- vị trí bảng (Task 114)

fn overlaps(a: Rect, b: Rect) -> bool {
    a.w > 0 && a.h > 0 && b.w > 0 && b.h > 0 && intersect(a, b).is_some()
}

fn overlap_area(a: Rect, b: Rect) -> i64 {
    intersect(a, b).map_or(0, |r| i64::from(r.w) * i64::from(r.h))
}

/// Vị trí ứng viên của bảng cỡ `(w, h)`: cạnh thanh dọc (phía xa vùng chọn) rồi bên phải, bên trái,
/// dưới, trên vùng chọn; mỗi ứng viên đã kẹp vào màn hình.
fn candidates(size: (i32, i32), side: Rect, sel: Rect, mon: Rect, dpi: u32) -> Vec<Rect> {
    let (w, h) = size;
    let gap = scaled(GAP, dpi);
    let at = |x: i32, y: i32| clamp_into(Rect { x, y, w, h }, mon);
    // Canh giữa theo cạnh của vùng chọn để bảng nằm cạnh phần đang làm.
    let (cy, cx) = (sel.y + sel.h / 2 - h / 2, sel.x + sel.w / 2 - w / 2);
    let beside_side = if side_on_right(side, sel) {
        at(side.right().saturating_add(gap), side.y)
    } else {
        at(side.x.saturating_sub(gap).saturating_sub(w), side.y)
    };
    vec![
        beside_side,
        at(sel.right().saturating_add(gap), cy),
        at(sel.x.saturating_sub(gap).saturating_sub(w), cy),
        at(cx, sel.bottom().saturating_add(gap)),
        at(cx, sel.y.saturating_sub(gap).saturating_sub(h)),
    ]
}

/// Ứng viên không giao vùng chọn, thanh dọc và thanh ngang (nếu có); `None` khi không có.
fn free_spot(
    size: (i32, i32),
    side: Rect,
    bar: Option<Rect>,
    sel: Rect,
    mon: Rect,
    dpi: u32,
) -> Option<Rect> {
    candidates(size, side, sel, mon, dpi).into_iter().find(|r| {
        !overlaps(*r, sel) && !overlaps(*r, side) && !bar.is_some_and(|b| overlaps(*r, b))
    })
}

/// Bảng có nên tự thu gọn: bảng mở rộng không có chỗ trống ngoài vùng chọn nhưng bảng thu gọn có.
pub fn auto_collapse(
    kind: StyleKind,
    side: Rect,
    bar: Option<Rect>,
    sel: Rect,
    mon: Rect,
    dpi: u32,
) -> bool {
    free_spot(panel_size_ex(kind, false, dpi), side, bar, sel, mon, dpi).is_none()
        && free_spot(panel_size_ex(kind, true, dpi), side, bar, sel, mon, dpi).is_some()
}

/// Hình chữ nhật của bảng thuộc tính `kind` (mở rộng hoặc thu gọn). Ưu tiên chỗ không phủ vùng chọn,
/// thanh dọc và thanh ngang (thứ tự: cạnh thanh dọc, phải, trái, dưới, trên vùng chọn); không còn
/// chỗ nào thì chọn nơi phủ vùng chọn ít nhất (người dùng kéo bảng đi được). Luôn nằm trong `mon`.
pub fn style_panel_rect(
    kind: StyleKind,
    collapsed: bool,
    side: Rect,
    bar: Option<Rect>,
    sel: Rect,
    mon: Rect,
    dpi: u32,
) -> Rect {
    let size = panel_size_ex(kind, collapsed, dpi);
    if let Some(r) = free_spot(size, side, bar, sel, mon, dpi) {
        return r;
    }
    let all = candidates(size, side, sel, mon, dpi);
    let mut best = all[0];
    let mut best_area = overlap_area(best, sel);
    for r in all.into_iter().skip(1) {
        let a = overlap_area(r, sel);
        if a < best_area {
            (best, best_area) = (r, a);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: i32, y: i32) -> Point {
        Point { x, y }
    }

    fn centre(r: Rect) -> Point {
        pt(r.x + r.w / 2, r.y + r.h / 2)
    }

    fn at(kind: StyleKind, dpi: u32) -> StyleLayout {
        let (w, h) = panel_size(kind, dpi);
        layout(
            Rect {
                x: 100,
                y: 50,
                w,
                h,
            },
            kind,
            dpi,
        )
    }

    #[test]
    fn defaults_match_the_old_look() {
        assert_eq!(BOX_DEFAULT.fill, None);
        assert_eq!(BOX_DEFAULT.opacity, 1.0);
        assert_eq!(BOX_DEFAULT.width_dip, STROKE_DIP);
        assert_eq!(LABEL_DEFAULT.fill, None);
        assert_eq!(LABEL_DEFAULT.width_dip, 0.0);
        assert_eq!(LABEL_DEFAULT.opacity, 1.0);
        let s = LABEL_DEFAULT.style(Rgb(1, 2, 3), 192);
        assert_eq!(
            (s.stroke_width, s.stroke, s.fill),
            (0.0, Rgb(1, 2, 3), None)
        );
        assert_eq!(BOX_DEFAULT.style(Rgb(0, 0, 0), 192).stroke_width, 6.0);
    }

    #[test]
    fn opacity_and_width_steps_are_clamped() {
        assert_eq!(step_opacity(1.0, 1), 1.0);
        assert_eq!(step_opacity(1.0, -1), 0.9);
        assert_eq!(step_opacity(0.5, -1), 0.4);
        assert_eq!(step_opacity(OPACITY_MIN, -1), OPACITY_MIN);
        assert_eq!(clamp_opacity(f32::NAN), 1.0);
        assert_eq!(clamp_opacity(-3.0), OPACITY_MIN);
        assert_eq!(clamp_opacity(9.0), 1.0);
        assert_eq!(step_width(3.0, 1), 4.0);
        assert_eq!(step_width(0.0, -1), 0.0);
        assert_eq!(step_width(WIDTH_MAX_DIP, 1), WIDTH_MAX_DIP);
        assert_eq!(clamp_width(f32::NAN), 0.0);
        assert_eq!(opacity_percent(0.5), 50);
        assert_eq!(opacity_percent(7.0), 100);
        let mut p = BOX_DEFAULT;
        p.apply(StyleEdit::Opacity(5.0));
        assert_eq!(p.opacity, 1.0);
        p.apply(StyleEdit::WidthDip(-4.0));
        assert_eq!(p.width_dip, 0.0);
        p.apply(StyleEdit::Fill(Some(Rgb(9, 9, 9))));
        assert_eq!(p.fill, Some(Rgb(9, 9, 9)));
    }

    #[test]
    fn every_cell_hits_back_to_itself_at_every_dpi() {
        for dpi in [96, 144, 192] {
            for kind in [StyleKind::Box, StyleKind::Label] {
                let l = at(kind, dpi);
                assert_eq!(l.cells.len(), PALETTE.len());
                assert_eq!(
                    cell_at(&l, centre(l.fill_chip)),
                    Some(StyleCell::Target(StyleTarget::Fill))
                );
                assert_eq!(
                    cell_at(&l, centre(l.border_chip)),
                    Some(StyleCell::Target(StyleTarget::Border))
                );
                assert_eq!(cell_at(&l, centre(l.no_fill)), Some(StyleCell::NoFill));
                for (i, c) in l.cells.iter().enumerate() {
                    assert_eq!(cell_at(&l, centre(*c)), Some(StyleCell::Color(i)));
                }
                assert_eq!(
                    cell_at(&l, centre(l.width_minus)),
                    Some(StyleCell::Width(-1))
                );
                assert_eq!(cell_at(&l, centre(l.width_plus)), Some(StyleCell::Width(1)));
                assert_eq!(
                    cell_at(&l, centre(l.opacity_minus)),
                    Some(StyleCell::Opacity(-1))
                );
                assert_eq!(
                    cell_at(&l, centre(l.opacity_plus)),
                    Some(StyleCell::Opacity(1))
                );
                // Nhãn và giá trị không bấm được; ngoài bảng không trúng gì.
                assert_eq!(cell_at(&l, centre(l.width_label)), None);
                assert_eq!(cell_at(&l, centre(l.opacity_value)), None);
                assert_eq!(cell_at(&l, pt(0, 0)), None);
                match (kind, l.aligns) {
                    (StyleKind::Label, Some(a)) => {
                        for (al, r) in Align::ALL.iter().zip(a) {
                            assert_eq!(cell_at(&l, centre(r)), Some(StyleCell::Align(*al)));
                        }
                    }
                    (StyleKind::Box, None) => {}
                    other => panic!("{other:?}"),
                }
            }
        }
    }

    #[test]
    fn all_cells_are_inside_the_panel_and_do_not_overlap() {
        for dpi in [96, 144, 192] {
            for kind in [StyleKind::Box, StyleKind::Label] {
                let l = at(kind, dpi);
                let mut rs = vec![
                    l.text_chip,
                    l.fill_chip,
                    l.border_chip,
                    l.no_fill,
                    l.width_minus,
                    l.width_value,
                    l.width_plus,
                    l.opacity_minus,
                    l.opacity_value,
                    l.opacity_plus,
                ];
                rs.extend(l.cells.iter().copied());
                if let Some(a) = l.aligns {
                    rs.extend(a);
                }
                for r in rs.iter().filter(|r| r.w > 0) {
                    assert!(
                        r.x >= l.panel.x
                            && r.y >= l.panel.y
                            && r.right() <= l.panel.right()
                            && r.bottom() <= l.panel.bottom(),
                        "{r:?} ngoài {:?}",
                        l.panel
                    );
                }
                for (i, a) in rs.iter().enumerate() {
                    for b in &rs[i + 1..] {
                        let disjoint = a.right() <= b.x
                            || b.right() <= a.x
                            || a.bottom() <= b.y
                            || b.bottom() <= a.y;
                        assert!(disjoint, "{a:?} chồng {b:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn label_panel_is_taller_than_box_panel() {
        let (_, hb) = panel_size(StyleKind::Box, 96);
        let (_, hl) = panel_size(StyleKind::Label, 96);
        assert!(hl > hb);
    }

    // ---- Task 113: Highlight hai chế độ

    #[test]
    fn marker_width_steps_by_two_inside_the_range() {
        assert_eq!(step_marker_width(16.0, 1), 18.0);
        assert_eq!(step_marker_width(16.0, -1), 14.0);
        assert_eq!(
            step_marker_width(MARKER_WIDTH_MIN_DIP, -1),
            MARKER_WIDTH_MIN_DIP
        );
        assert_eq!(
            step_marker_width(MARKER_WIDTH_MAX_DIP, 1),
            MARKER_WIDTH_MAX_DIP
        );
        assert_eq!(clamp_marker_width(f32::NAN), MARKER_WIDTH_MIN_DIP);
        assert_eq!(clamp_marker_width(500.0), MARKER_WIDTH_MAX_DIP);
        assert_eq!(clamp_marker_width(-3.0), MARKER_WIDTH_MIN_DIP);
    }

    #[test]
    fn highlight_panels_hit_modes_colors_and_only_their_own_stepper() {
        for dpi in [96, 144, 192] {
            for kind in [StyleKind::MarkerFree, StyleKind::MarkerRect] {
                let l = at(kind, dpi);
                let m = l.modes.expect("hàng chọn chế độ");
                assert_eq!(
                    cell_at(&l, centre(m[0])),
                    Some(StyleCell::Mode(HighlightMode::Freehand))
                );
                assert_eq!(
                    cell_at(&l, centre(m[1])),
                    Some(StyleCell::Mode(HighlightMode::Rect))
                );
                for (i, c) in l.cells.iter().enumerate() {
                    assert_eq!(cell_at(&l, centre(*c)), Some(StyleCell::Color(i)));
                }
                // Không có Nền/Viền/Không nền ở bảng Highlight.
                assert_eq!((l.fill_chip.w, l.border_chip.w, l.no_fill.w), (0, 0, 0));
                let (w, o) = (
                    cell_at(&l, centre(l.width_plus)),
                    cell_at(&l, centre(l.opacity_plus)),
                );
                // Tự do: độ to nét và độ trong suốt (Task 114); hình chữ nhật: chỉ độ trong suốt.
                if kind == StyleKind::MarkerFree {
                    assert_eq!(
                        (w, o),
                        (Some(StyleCell::Width(1)), Some(StyleCell::Opacity(1)))
                    );
                } else {
                    assert_eq!((w, o), (None, Some(StyleCell::Opacity(1))));
                }
                assert!(l.aligns.is_none());
                // Mọi ô nằm trong bảng và không chồng nhau.
                let mut rs: Vec<Rect> = m.to_vec();
                rs.extend(l.cells.iter().copied());
                rs.extend([
                    l.width_minus,
                    l.width_value,
                    l.width_plus,
                    l.opacity_minus,
                    l.opacity_value,
                    l.opacity_plus,
                ]);
                for r in rs.iter().filter(|r| r.w > 0) {
                    assert!(
                        r.x >= l.panel.x
                            && r.y >= l.panel.y
                            && r.right() <= l.panel.right()
                            && r.bottom() <= l.panel.bottom()
                    );
                }
                for (i, a) in rs.iter().enumerate() {
                    for b in &rs[i + 1..] {
                        assert!(
                            a.right() <= b.x
                                || b.right() <= a.x
                                || a.bottom() <= b.y
                                || b.bottom() <= a.y,
                            "{a:?} chồng {b:?}"
                        );
                    }
                }
            }
        }
        // Bảng Hình, Chữ không có hàng chế độ.
        assert!(at(StyleKind::Box, 96).modes.is_none());
        assert!(at(StyleKind::Label, 96).modes.is_none());
    }

    // ---- Task 114: sửa bảng kiểu

    #[test]
    fn fill_and_no_fill_chips_are_mutually_exclusive() {
        for target in [StyleTarget::Fill, StyleTarget::Border, StyleTarget::Text] {
            for fill in [None, Some(Rgb(1, 2, 3))] {
                let f = chip_focus(StyleKind::Box, target, fill);
                let lit = [f.text, f.fill, f.border, f.no_fill];
                assert_eq!(lit.iter().filter(|b| **b).count(), 1, "{target:?} {fill:?}");
                assert!(!f.text, "Hình không có đích Màu chữ");
                assert_eq!(f.border, target == StyleTarget::Border);
                let eff = target.for_kind(StyleKind::Box);
                assert_eq!(f.no_fill, eff == StyleTarget::Fill && fill.is_none());
            }
        }
    }

    #[test]
    fn exactly_one_target_is_lit_for_text_three_targets_and_shapes_two() {
        let some = Some(Rgb(1, 2, 3));
        for kind in [StyleKind::Label, StyleKind::Box, StyleKind::Oval] {
            for target in [StyleTarget::Fill, StyleTarget::Border, StyleTarget::Text] {
                for fill in [None, some] {
                    let f = chip_focus(kind, target, fill);
                    let n = [f.text, f.fill, f.border, f.no_fill]
                        .iter()
                        .filter(|b| **b)
                        .count();
                    assert_eq!(n, 1, "{kind:?} {target:?} {fill:?}: {f:?}");
                }
            }
        }
        let f = chip_focus(StyleKind::Label, StyleTarget::Text, some);
        assert!(f.text && !f.fill && !f.border && !f.no_fill);
        let f = chip_focus(StyleKind::Label, StyleTarget::Fill, None);
        assert!(f.no_fill && !f.fill && !f.text);
        // Hình không có Màu chữ: đích Màu chữ cũ rơi về Nền.
        let f = chip_focus(StyleKind::Box, StyleTarget::Text, some);
        assert!(f.fill && !f.text);
        assert_eq!(
            StyleTarget::Text.for_kind(StyleKind::Oval),
            StyleTarget::Fill
        );
        assert_eq!(
            StyleTarget::Text.for_kind(StyleKind::Label),
            StyleTarget::Text
        );
    }

    #[test]
    fn text_panel_has_four_chips_in_one_row_right_above_the_grid() {
        for dpi in [96, 144, 192] {
            let l = at(StyleKind::Label, dpi);
            let chips = [l.text_chip, l.fill_chip, l.border_chip, l.no_fill];
            for c in chips {
                assert!(c.w > 0 && c.y == chips[0].y && c.h == chips[0].h, "{c:?}");
            }
            for w in chips.windows(2) {
                assert!(w[0].right() <= w[1].x, "{w:?}");
            }
            assert!(chips[3].right() <= l.panel.right());
            assert_eq!(
                cell_at(&l, centre(l.text_chip)),
                Some(StyleCell::Target(StyleTarget::Text))
            );
            // Lưới ngay dưới hàng ô: khe nhỏ hơn khe giữa các nhóm khác.
            let gap = l.cells[0].y - chips[0].bottom();
            assert!(gap > 0 && gap <= scaled(CHIP_GAP, dpi) + 1, "khe {gap}");
            // Hình không có ô Màu chữ.
            let b = at(StyleKind::Box, dpi);
            assert_eq!(b.text_chip.w, 0);
            assert_ne!(
                cell_at(&b, pt(b.fill_chip.x + 1, b.fill_chip.y + 1)),
                Some(StyleCell::Target(StyleTarget::Text))
            );
        }
    }

    #[test]
    fn radius_steps_by_two_and_is_clamped() {
        assert_eq!(step_radius(0.0, 1), RADIUS_STEP_PX);
        assert_eq!(step_radius(0.0, -1), 0.0);
        assert_eq!(step_radius(RADIUS_MAX_PX, 1), RADIUS_MAX_PX);
        assert_eq!(clamp_radius_px(f32::NAN), 0.0);
        assert_eq!(clamp_radius_px(-5.0), 0.0);
        assert_eq!(clamp_radius_px(9999.0), RADIUS_MAX_PX);
    }

    #[test]
    fn no_border_and_radius_flow_into_the_style_and_keep_the_old_width() {
        let mut p = BOX_DEFAULT;
        p.apply(StyleEdit::NoBorder(true));
        p.apply(StyleEdit::RadiusPx(12.0));
        let on = p.style(Rgb(9, 9, 9), 96);
        assert!(on.no_border);
        assert_eq!(on.radius, 12.0);
        assert_eq!(on.border_width(), 0.0);
        // Tắt lại: độ dày và màu cũ còn nguyên.
        p.apply(StyleEdit::NoBorder(false));
        let off = p.style(Rgb(9, 9, 9), 96);
        assert_eq!(off.border_width(), STROKE_DIP);
        assert_eq!(off.stroke, Rgb(9, 9, 9));
        p.apply(StyleEdit::RadiusPx(-3.0));
        assert_eq!(p.radius_px, 0.0);
        assert_eq!((BOX_DEFAULT.radius_px, BOX_DEFAULT.no_border), (0.0, false));
        assert_eq!(
            (LABEL_DEFAULT.radius_px, LABEL_DEFAULT.no_border),
            (0.0, false)
        );
    }

    #[test]
    fn new_rows_hit_back_to_themselves_and_oval_has_no_radius_row() {
        for dpi in [96, 144, 192] {
            for kind in [StyleKind::Box, StyleKind::Label, StyleKind::Oval] {
                let l = at(kind, dpi);
                assert_eq!(cell_at(&l, centre(l.no_border)), Some(StyleCell::NoBorder));
                assert_eq!(cell_at(&l, centre(l.collapse)), Some(StyleCell::Collapse));
                assert_eq!(
                    cell_at(&l, pt(l.header.x + 2, l.header.y + 2)),
                    Some(StyleCell::Header)
                );
                if kind == StyleKind::Oval {
                    assert_eq!((l.radius_minus.w, l.radius_plus.w), (0, 0));
                    assert_eq!(cell_at(&l, pt(0, 0)), None);
                    assert!(!StyleKind::Oval.has_radius());
                } else {
                    assert_eq!(
                        cell_at(&l, centre(l.radius_minus)),
                        Some(StyleCell::Radius(-1))
                    );
                    assert_eq!(
                        cell_at(&l, centre(l.radius_plus)),
                        Some(StyleCell::Radius(1))
                    );
                }
                // Mọi ô nằm trong bảng và không chồng nhau.
                let mut rs = vec![
                    l.header,
                    l.fill_chip,
                    l.border_chip,
                    l.no_fill,
                    l.no_border,
                    l.width_minus,
                    l.width_value,
                    l.width_plus,
                    l.opacity_minus,
                    l.opacity_value,
                    l.opacity_plus,
                ];
                if kind != StyleKind::Oval {
                    rs.extend([l.radius_minus, l.radius_value, l.radius_plus]);
                }
                rs.extend(l.cells.iter().copied());
                if let Some(a) = l.aligns {
                    rs.extend(a);
                }
                for r in rs.iter().filter(|r| r.w > 0) {
                    assert!(
                        r.x >= l.panel.x
                            && r.y >= l.panel.y
                            && r.right() <= l.panel.right()
                            && r.bottom() <= l.panel.bottom(),
                        "{kind:?} {r:?} ngoài {:?}",
                        l.panel
                    );
                }
                for (i, a) in rs.iter().enumerate() {
                    for b in &rs[i + 1..] {
                        assert!(
                            a.right() <= b.x
                                || b.right() <= a.x
                                || a.bottom() <= b.y
                                || b.bottom() <= a.y,
                            "{kind:?} {a:?} chồng {b:?}"
                        );
                    }
                }
            }
        }
        assert!(!StyleKind::MarkerFree.has_border() && !StyleKind::MarkerRect.has_border());
    }

    #[test]
    fn collapsed_panel_is_only_the_header() {
        for kind in [StyleKind::Box, StyleKind::Label, StyleKind::MarkerFree] {
            let (w, h) = panel_size_ex(kind, true, 96);
            let (fw, fh) = panel_size_ex(kind, false, 96);
            assert_eq!(w, fw);
            assert!(h < fh / 4);
            let l = layout_ex(Rect { x: 10, y: 10, w, h }, kind, true, 96);
            assert!(l.cells.is_empty() && l.modes.is_none() && l.aligns.is_none());
            assert_eq!(cell_at(&l, centre(l.collapse)), Some(StyleCell::Collapse));
            // Dưới thanh tiêu đề không có ô nào.
            assert_eq!(cell_at(&l, pt(l.panel.x + 5, l.panel.bottom() + 5)), None);
        }
    }

    // ---- vị trí bảng

    const SCREEN: Rect = Rect {
        x: 0,
        y: 0,
        w: 1440,
        h: 900,
    };

    fn parts(sel: Rect, dpi: u32) -> (Rect, Rect) {
        // Thanh ngang dưới vùng chọn, thanh dọc bên phải (như `bar_rect`, `side_rect`): giả lập hai
        // hình chữ nhật cỡ thật gần đúng đủ cho kiểm thử bố cục.
        let bar = Rect {
            x: sel.right() - scaled(260, dpi),
            y: (sel.bottom() + scaled(6, dpi)).min(SCREEN.bottom() - scaled(40, dpi)),
            w: scaled(260, dpi),
            h: scaled(40, dpi),
        };
        let side = Rect {
            x: (sel.right() + scaled(6, dpi)).min(SCREEN.right() - scaled(44, dpi)),
            y: sel.y,
            w: scaled(44, dpi),
            h: scaled(330, dpi),
        };
        (bar, side)
    }

    fn inside(r: Rect, m: Rect) -> bool {
        r.x >= m.x && r.y >= m.y && r.right() <= m.right() && r.bottom() <= m.bottom()
    }

    #[test]
    fn panel_avoids_selection_bars_and_screen_edges_whenever_there_is_room() {
        let sels = [
            Rect {
                x: 500,
                y: 300,
                w: 400,
                h: 300,
            },
            Rect {
                x: 0,
                y: 0,
                w: 400,
                h: 300,
            },
            Rect {
                x: 1040,
                y: 600,
                w: 400,
                h: 300,
            },
            Rect {
                x: 100,
                y: 100,
                w: 700,
                h: 500,
            },
            Rect {
                x: 300,
                y: 0,
                w: 800,
                h: 880,
            },
        ];
        for dpi in [96u32, 192] {
            let mon = Rect {
                x: 0,
                y: 0,
                w: SCREEN.w * dpi as i32 / 96,
                h: SCREEN.h * dpi as i32 / 96,
            };
            for kind in [StyleKind::Box, StyleKind::Label, StyleKind::MarkerFree] {
                for s in sels {
                    let sel = Rect {
                        x: s.x * dpi as i32 / 96,
                        y: s.y * dpi as i32 / 96,
                        w: s.w * dpi as i32 / 96,
                        h: s.h * dpi as i32 / 96,
                    };
                    let (bar, side) = parts(sel, dpi);
                    let r = style_panel_rect(kind, false, side, Some(bar), sel, mon, dpi);
                    assert!(inside(r, mon), "{kind:?} {sel:?} -> {r:?}");
                    let (w, h) = panel_size_ex(kind, false, dpi);
                    assert_eq!((r.w, r.h), (w, h));
                    // Có chỗ trống ngoài vùng chọn thì không phủ vùng chọn, thanh dọc, thanh ngang.
                    let has_room = sel.x >= w + scaled(6, dpi) * 2
                        || mon.right() - sel.right() >= w + scaled(6, dpi) * 2;
                    if has_room {
                        assert!(!overlaps(r, sel), "{kind:?} {sel:?} -> {r:?}");
                        assert!(!overlaps(r, side), "{kind:?} {sel:?} -> {r:?} / {side:?}");
                        assert!(!overlaps(r, bar), "{kind:?} {sel:?} -> {r:?} / {bar:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn panel_uses_above_or_below_when_the_sides_are_full() {
        // Vùng chọn gần hết bề rộng nhưng còn dải cao phía dưới.
        let sel = Rect {
            x: 20,
            y: 20,
            w: 1400,
            h: 300,
        };
        let (bar, side) = parts(sel, 96);
        let side = Rect { x: 1390, ..side };
        let bar = Rect { y: 330, ..bar };
        let r = style_panel_rect(StyleKind::Box, false, side, Some(bar), sel, SCREEN, 96);
        assert!(inside(r, SCREEN));
        assert!(!overlaps(r, sel), "{r:?}");
    }

    #[test]
    fn full_screen_selection_still_gives_an_on_screen_panel_and_may_collapse() {
        let (bar, side) = parts(
            Rect {
                x: 0,
                y: 0,
                w: SCREEN.w - 60,
                h: SCREEN.h,
            },
            96,
        );
        let sel = SCREEN;
        for collapsed in [false, true] {
            let r = style_panel_rect(
                StyleKind::Label,
                collapsed,
                side,
                Some(bar),
                sel,
                SCREEN,
                96,
            );
            assert!(inside(r, SCREEN), "{r:?}");
        }
        // Không chỗ nào cho bảng, kể cả thu gọn: không tự thu gọn (người dùng kéo bảng đi).
        assert!(!auto_collapse(
            StyleKind::Label,
            side,
            Some(bar),
            sel,
            SCREEN,
            96
        ));
    }

    #[test]
    fn auto_collapse_only_when_the_collapsed_panel_fits_and_the_full_one_does_not() {
        // Vùng chọn phủ hết bề ngang; chỉ còn dải thấp phía dưới: đủ cho bảng thu gọn, không đủ cho
        // bảng đầy đủ.
        let mon = Rect {
            x: 0,
            y: 0,
            w: 1000,
            h: 500,
        };
        let sel = Rect {
            x: 0,
            y: 0,
            w: 1000,
            h: 440,
        };
        let side = Rect {
            x: 960,
            y: 10,
            w: 36,
            h: 100,
        };
        let bar = Rect {
            x: 700,
            y: 450,
            w: 290,
            h: 40,
        };
        let full = style_panel_rect(StyleKind::Label, false, side, Some(bar), sel, mon, 96);
        assert!(
            overlaps(full, sel),
            "không có chỗ cho bảng đầy đủ: {full:?}"
        );
        let small = style_panel_rect(StyleKind::Label, true, side, Some(bar), sel, mon, 96);
        assert!(
            inside(small, mon) && !overlaps(small, sel) && !overlaps(small, bar),
            "{small:?}"
        );
        assert!(auto_collapse(
            StyleKind::Label,
            side,
            Some(bar),
            sel,
            mon,
            96
        ));
        // Vùng chọn nhỏ: bảng đầy đủ có chỗ nên không tự thu gọn.
        let sel = Rect {
            x: 300,
            y: 100,
            w: 300,
            h: 200,
        };
        assert!(!auto_collapse(
            StyleKind::Label,
            side,
            Some(bar),
            sel,
            mon,
            96
        ));
    }

    fn overlaps(a: Rect, b: Rect) -> bool {
        a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
    }
}
