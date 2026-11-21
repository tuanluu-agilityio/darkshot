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
use darkshot_core::overlay_layout::scaled;
use darkshot_core::theme::Rgb;

/// Bảng đang sửa loại hình nào.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleKind {
    /// Hình chữ nhật hoặc elip: nền, độ dày viền, độ trong suốt; màu viền là màu của nút Màu.
    Box,
    /// Chữ: nền, màu viền, độ dày viền, độ trong suốt, căn lề; màu chữ là màu của nút Màu.
    Label,
    /// Highlight chế độ tự do (nét Bút dạ quang): màu, độ to nhỏ nét (Task 113).
    MarkerFree,
    /// Highlight chế độ hình chữ nhật: màu, độ trong suốt, không viền (Task 113).
    MarkerRect,
}

impl StyleKind {
    /// Bảng Highlight (có hàng chọn chế độ thay hàng Nền/Viền).
    pub fn is_highlight(self) -> bool {
        matches!(self, StyleKind::MarkerFree | StyleKind::MarkerRect)
    }
}

/// Ô màu của bảng đang áp cho nền hay cho viền.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleTarget {
    Fill,
    Border,
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
}

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
}

/// Hình chữ nhật, elip: không nền, đặc, viền như mức cỡ Vừa.
pub const BOX_DEFAULT: StylePrefs = StylePrefs {
    fill: None,
    stroke: DEFAULT_COLOR,
    width_dip: STROKE_DIP,
    opacity: 1.0,
};

/// Chữ: không nền, không viền, đặc (như chữ trước đây).
pub const LABEL_DEFAULT: StylePrefs = StylePrefs {
    fill: None,
    stroke: DEFAULT_COLOR,
    width_dip: 0.0,
    opacity: 1.0,
};

impl StylePrefs {
    /// Kiểu cụ thể ở `dpi` với màu viền `stroke` (đã kẹp).
    pub fn style(&self, stroke: Rgb, dpi: u32) -> Style {
        Style {
            fill: self.fill,
            stroke,
            stroke_width: dip_to_px(self.width_dip, dpi),
            opacity: self.opacity,
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
        }
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
    pub width_dip: f32,
    pub opacity: f32,
    pub align: Option<Align>,
    /// Chế độ Highlight sẽ dùng cho lần vẽ tiếp (chỉ bảng Highlight).
    pub mode: Option<HighlightMode>,
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
const BTN: i32 = 24;
const VALUE_W: i32 = 44;

/// Bố cục các ô của bảng đặt ở `panel`.
#[derive(Clone, Debug, PartialEq)]
pub struct StyleLayout {
    pub panel: Rect,
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
    pub opacity_label: Rect,
    pub opacity_minus: Rect,
    pub opacity_value: Rect,
    pub opacity_plus: Rect,
    /// Ba ô căn lề (chỉ khi `StyleKind::Label`).
    pub aligns: Option<[Rect; 3]>,
}

/// Kích thước (rộng, cao) của bảng ở `dpi`.
pub fn panel_size(kind: StyleKind, dpi: u32) -> (i32, i32) {
    let w = 2 * PAD + COLS * CELL + (COLS - 1) * CELL_GAP;
    let mut h = PAD + ROW_H + SECTION_GAP + ROWS * CELL + (ROWS - 1) * CELL_GAP;
    h += SECTION_GAP + ROW_H + ROW_GAP + ROW_H;
    if kind == StyleKind::Label {
        h += ROW_GAP + ROW_H;
    }
    h += PAD;
    (scaled(w, dpi), scaled(h, dpi))
}

fn rect(x: i32, y: i32, w: i32, h: i32, dpi: u32) -> Rect {
    Rect {
        x: scaled(x, dpi),
        y: scaled(y, dpi),
        w: scaled(w, dpi),
        h: scaled(h, dpi),
    }
}

fn shift(r: Rect, o: Rect) -> Rect {
    Rect {
        x: r.x + o.x,
        y: r.y + o.y,
        ..r
    }
}

/// Bố cục của bảng `kind` có góc trên trái ở `panel` (cỡ `panel_size`).
pub fn layout(panel: Rect, kind: StyleKind, dpi: u32) -> StyleLayout {
    let inner = COLS * CELL + (COLS - 1) * CELL_GAP;
    let chip_w = (inner - 2 * 4) / 3;
    let mut y = PAD;
    let chip = |i: i32| rect(PAD + i * (chip_w + 4), y, chip_w, ROW_H, dpi);
    let (mut fill_chip, mut border_chip, mut no_fill) = (chip(0), chip(1), chip(2));
    let mode_w = (inner - 4) / 2;
    let modes = kind
        .is_highlight()
        .then(|| [0, 1].map(|i| rect(PAD + i * (mode_w + 4), y, mode_w, ROW_H, dpi)));
    if modes.is_some() {
        let none = Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        };
        (fill_chip, border_chip, no_fill) = (none, none, none);
    }
    y += ROW_H + SECTION_GAP;
    let cells: Vec<Rect> = (0..PALETTE.len() as i32)
        .map(|i| {
            let (c, r) = (i % COLS, i / COLS);
            rect(
                PAD + c * (CELL + CELL_GAP),
                y + r * (CELL + CELL_GAP),
                CELL,
                CELL,
                dpi,
            )
        })
        .collect();
    y += ROWS * CELL + (ROWS - 1) * CELL_GAP + SECTION_GAP;
    let label_w = inner - 2 * BTN - VALUE_W;
    let stepper = |y: i32| {
        (
            rect(PAD, y, label_w, ROW_H, dpi),
            rect(PAD + label_w, y, BTN, ROW_H, dpi),
            rect(PAD + label_w + BTN, y, VALUE_W, ROW_H, dpi),
            rect(PAD + label_w + BTN + VALUE_W, y, BTN, ROW_H, dpi),
        )
    };
    let (width_label, width_minus, width_value, width_plus) = stepper(y);
    y += ROW_H + ROW_GAP;
    let (opacity_label, opacity_minus, opacity_value, opacity_plus) = stepper(y);
    y += ROW_H + ROW_GAP;
    let aligns = (kind == StyleKind::Label).then(|| {
        let w = (inner - 2 * 4) / 3;
        [0, 1, 2].map(|i| rect(PAD + i * (w + 4), y, w, ROW_H, dpi))
    });
    let at = |r: Rect| shift(r, panel);
    StyleLayout {
        panel,
        fill_chip: at(fill_chip),
        border_chip: at(border_chip),
        no_fill: at(no_fill),
        modes: modes.map(|m| m.map(at)),
        kind,
        cells: cells.into_iter().map(at).collect(),
        width_label: at(width_label),
        width_minus: at(width_minus),
        width_value: at(width_value),
        width_plus: at(width_plus),
        opacity_label: at(opacity_label),
        opacity_minus: at(opacity_minus),
        opacity_value: at(opacity_value),
        opacity_plus: at(opacity_plus),
        aligns: aligns.map(|a| a.map(at)),
    }
}

/// Ô dưới điểm `p`; lề, nhãn và giá trị trả `None`.
pub fn cell_at(l: &StyleLayout, p: Point) -> Option<StyleCell> {
    if !l.panel.contains(p) {
        return None;
    }
    if let Some(m) = &l.modes {
        if let Some(i) = m.iter().position(|r| r.contains(p)) {
            return Some(StyleCell::Mode(HighlightMode::ALL[i]));
        }
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
    // Bảng Highlight chỉ có một bộ nút tăng giảm, tùy chế độ.
    let (use_width, use_opacity) = (
        l.kind != StyleKind::MarkerRect,
        l.kind != StyleKind::MarkerFree,
    );
    if use_width && l.width_minus.contains(p) {
        return Some(StyleCell::Width(-1));
    }
    if use_width && l.width_plus.contains(p) {
        return Some(StyleCell::Width(1));
    }
    if use_opacity && l.opacity_minus.contains(p) {
        return Some(StyleCell::Opacity(-1));
    }
    if use_opacity && l.opacity_plus.contains(p) {
        return Some(StyleCell::Opacity(1));
    }
    let aligns = l.aligns.as_ref()?;
    Align::ALL
        .iter()
        .zip(aligns)
        .find(|(_, r)| r.contains(p))
        .map(|(a, _)| StyleCell::Align(*a))
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
                for r in &rs {
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
                assert_eq!(panel_size(kind, dpi), panel_size(StyleKind::Box, dpi));
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
                if kind == StyleKind::MarkerFree {
                    assert_eq!((w, o), (Some(StyleCell::Width(1)), None));
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
                for r in &rs {
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
}
