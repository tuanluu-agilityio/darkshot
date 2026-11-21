//! Mô hình thuần của công cụ đánh dấu trên lớp phủ macOS (Task 111): công cụ hiện tại, hình nhớ,
//! mức cỡ, màu, sổ hình (`Annotations`), nét đang vẽ, ô chữ đang gõ, chọn/di chuyển/đổi cỡ hình,
//! bảng Cỡ/Hình/màu, cử chỉ chuột và phím. Không gọi Objective-C: kiểm thử chạy được không cần GUI.
//!
//! Tọa độ: pixel của ảnh màn hình đang xét, gốc trên trái; hình lưu theo tọa độ ảnh chụp (`Pt`).
//!
//! QUY TẮC XẾP LỚP (yêu cầu của chủ dự án): hình hoặc chữ vẽ SAU nằm PHÍA TRƯỚC hình vẽ trước.
//! Chỉ số cuối của `Annotations::shapes()` là trên cùng. Mọi hình mới (nét, hình kéo, chữ vừa nhập
//! xong, số đánh) vào sổ bằng `Annotations::add` (luôn ở cuối). Chọn/di chuyển/đổi cỡ/đổi màu một
//! hình dùng `replace` nên KHÔNG đổi lớp của nó. Bấm chỗ chồng nhau chọn hình phía trước
//! (`topmost_hit` duyệt từ cuối). Hình đang vẽ dở và chữ đang gõ luôn hiện trên cùng khi vẽ.

use crate::annotate_draw::{label_caret_rect, shape_bounds};
use crate::annotate_style::{
    cell_at as style_cell_at, clamp_marker_width, clamp_opacity, clamp_width,
    layout as style_layout, panel_size, step_marker_width, step_opacity, step_width, StyleCell,
    StyleEdit, StyleKind, StylePrefs, StyleTarget, StyleView, BOX_DEFAULT, LABEL_DEFAULT,
};
use crate::overlay_input::bar_item_at;
use darkshot_core::annotate::{
    dip_to_px, highlight_from_stroke, press_mode, sanitize_paste, styled_from_stroke, Align,
    Annotations, Applied, Change, HighlightMode, PressMode, Pt, Shape, SizeLevel, Style, TextEdit,
    Tool, DEFAULT_COLOR, HIGHLIGHT_OPACITY, HIT_TOL_DIP, MARKER_DIP, MIN_EXTENT_DIP, PALETTE,
};
use darkshot_core::color::picker_layout;
use darkshot_core::color_picker::{VK_OEM_4, VK_OEM_6};
use darkshot_core::edit::{
    grip_at, handles, highlight_restyle, restyle, topmost_hit, transform, Gesture, Grip,
};
use darkshot_core::geometry::{Hit, Point, Rect};
use darkshot_core::image::Image;
use darkshot_core::overlay_layout::{
    flyout_rect, inflate, intersect, is_shape, scaled, selection_dirty, shape_cell_at,
    side_item_at, side_layout_for, size_cell_at, text_preview_bounds, to_pt, union, BarItem,
    SideItem, HANDLE_HIT, HANDLE_SIZE, TEXT_EMPTY_W, TEXT_FRAME_PAD,
};
use darkshot_core::overlay_stroke::{begin_stroke, end_stroke, extend_stroke, Stroke};
use darkshot_core::theme::Rgb;

const NO_RECT: Rect = Rect {
    x: 0,
    y: 0,
    w: 0,
    h: 0,
};

/// Phần nhãn "màu cơ bản" của bảng màu Windows (96 DPI) mà bản Mac bỏ đi (chưa có bộ chọn tự chọn).
const PALETTE_LABEL_TRIM: i32 = 22;
/// Chiều cao (96 DPI) của vùng 48 ô màu tính từ đầu bảng đầy đủ của core (ô cuối + lề).
const PALETTE_BASIC_BOTTOM: i32 = 178;

/// Mục đang mở bảng bật ra cạnh thanh dọc; không bao giờ hai bảng cùng lúc.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flyout {
    None,
    Sizes,
    Shapes,
    Palette,
}

/// Ngữ cảnh một sự kiện: vùng chọn, màn hình, DPI, ảnh gốc và hai thanh đã đặt.
pub struct Env<'a> {
    pub sel: Rect,
    pub mon: Rect,
    pub dpi: u32,
    pub img: &'a Image,
    /// Thanh ngang (Hoàn tác, Làm lại | các nút hành động); `None` khi đang kéo vùng chọn.
    pub bar: Option<Rect>,
    /// Thanh dọc công cụ; `None` khi đang kéo vùng chọn.
    pub side: Option<Rect>,
}

/// Bố cục của một sự kiện (chưa gắn ảnh): vùng chọn, màn hình, DPI, hai thanh đã đặt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub sel: Rect,
    pub mon: Rect,
    pub dpi: u32,
    pub bar: Option<Rect>,
    pub side: Option<Rect>,
}

impl Layout {
    /// Ngữ cảnh sự kiện với ảnh gốc `img`.
    pub fn env<'a>(&self, img: &'a Image) -> Env<'a> {
        Env {
            sel: self.sel,
            mon: self.mon,
            dpi: self.dpi,
            img,
            bar: self.bar,
            side: self.side,
        }
    }
}

/// Lựa chọn nhớ qua các lần mở lớp phủ (hình, cỡ, màu).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prefs {
    pub shape_tool: Tool,
    pub size: SizeLevel,
    pub color: Rgb,
    /// Kiểu mặc định của Hình chữ nhật và Elip (nền, độ dày viền, độ trong suốt).
    pub box_style: StylePrefs,
    /// Kiểu mặc định của Chữ (nền, viền, độ trong suốt).
    pub label_style: StylePrefs,
    pub align: Align,
    /// Chế độ của công cụ Highlight cho lần vẽ tiếp (Task 113).
    pub highlight_mode: HighlightMode,
    /// Độ dày nét Highlight tự do (dip).
    pub marker_width_dip: f32,
    /// Độ trong suốt của Highlight hình chữ nhật (0..=1).
    pub highlight_opacity: f32,
}

impl Prefs {
    pub const DEFAULT: Prefs = Prefs {
        shape_tool: Tool::Arrow,
        size: SizeLevel::Medium,
        color: DEFAULT_COLOR,
        box_style: BOX_DEFAULT,
        label_style: LABEL_DEFAULT,
        align: Align::Left,
        highlight_mode: HighlightMode::Freehand,
        marker_width_dip: MARKER_DIP,
        highlight_opacity: HIGHLIGHT_OPACITY,
    };
}

impl Default for Prefs {
    fn default() -> Prefs {
        Prefs::DEFAULT
    }
}

/// Kết quả của một cú nhấn chuột trong vùng công cụ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Down {
    /// Đã xử lý hết (hoặc bỏ qua); không làm gì thêm.
    Handled,
    /// Nhấn một nút của thanh ngang; `Model` giữ trạng thái nhấn và làm khi nhả.
    Bar(BarItem),
    /// Cú nhấn thuộc về vùng chọn: `Model` gọi `Selection::press`.
    Selection,
    /// Chuột đang được giữ cho một nét hoặc một cử chỉ sửa hình.
    Capture,
}

/// Con trỏ chuột theo ngữ cảnh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorKind {
    Arrow,
    Cross,
    IBeam,
    Move,
}

/// Lệnh soạn chữ từ bộ gõ (`doCommandBySelector:`) hoặc phím.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextCmd {
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Newline,
}

/// Ô chữ đang gõ cùng hình học đã đo (đo lại mỗi lần ô đổi, không đo khi vẽ hay khi nháy).
pub struct TextBox {
    pub edit: TextEdit,
    /// Điểm bấm tạo ô (tọa độ ảnh chụp).
    pub at: Point,
    /// DPI của màn hình chứa ô.
    pub dpi: u32,
    /// Cỡ chữ (px).
    pub size: f32,
    /// Con trỏ nháy (tọa độ ảnh chụp).
    pub caret: Rect,
    /// Khung nét đứt (tọa độ ảnh chụp).
    pub frame: Rect,
    /// Số ký tự bộ gõ đang soạn (chuỗi "marked"), nằm ngay trước con trỏ trong `edit`.
    pub marked: usize,
    /// Kiểu khối chữ (nền, viền, độ trong suốt) và căn lề; đổi được khi đang gõ.
    pub style: Style,
    pub align: Align,
}

impl TextBox {
    fn new(
        at: Point,
        dpi: u32,
        level: SizeLevel,
        color: Rgb,
        style: Style,
        align: Align,
    ) -> TextBox {
        let size = dip_to_px(level.text_dip(), dpi);
        let mut b = TextBox {
            edit: TextEdit::new(Self::origin(at), size, color),
            at,
            dpi,
            size,
            caret: NO_RECT,
            frame: NO_RECT,
            marked: 0,
            style,
            align,
        };
        b.measure();
        b
    }

    fn origin(at: Point) -> Pt {
        Pt {
            x: at.x as f32,
            y: at.y as f32,
        }
    }

    /// Đo lại con trỏ và khung. Khung gồm phần mực của chữ và con trỏ, nới `TEXT_FRAME_PAD`; ô chỉ
    /// có khoảng trắng thì tính từ điểm đặt ô, cao một dòng, rộng tối thiểu `TEXT_EMPTY_W`.
    fn measure(&mut self) {
        self.caret = label_caret_rect(&self.edit, self.align);
        // Hộp khối chữ (gồm đệm, viền) đo cả khi chỉ có khoảng trắng.
        let label = Shape::Label {
            origin: Self::origin(self.at),
            text: self.edit.text(),
            size: self.size,
            color: DEFAULT_COLOR,
            align: self.align,
            style: self.style,
        };
        let min = Rect {
            x: self.at.x,
            y: self.at.y,
            w: scaled(TEXT_EMPTY_W, self.dpi),
            h: self.caret.h,
        };
        let base = union(
            union(Some(shape_bounds(&label)), Some(min)),
            Some(self.caret),
        )
        .unwrap_or(self.caret);
        self.frame = inflate(base, scaled(TEXT_FRAME_PAD, self.dpi));
    }

    /// Đổi cỡ chữ theo mức `level` (giữ chữ, màu `color`, vị trí con trỏ) rồi đo lại.
    fn resize(&mut self, level: SizeLevel, color: Rgb) {
        self.size = dip_to_px(level.text_dip(), self.dpi);
        let text = self.edit.text();
        let caret = self.edit.caret();
        let mut edit = TextEdit::new(Self::origin(self.at), self.size, color);
        edit.insert(&text);
        // Sau khi chèn, con trỏ ở cuối chữ: lùi về chỗ cũ.
        for _ in caret..edit.caret() {
            edit.left();
        }
        self.edit = edit;
        self.measure();
    }

    /// Vùng màn hình ô chiếm (chữ, khung, con trỏ).
    pub fn dirty(&self) -> Rect {
        union(Some(inflate(self.frame, 1)), Some(self.caret)).unwrap_or(self.frame)
    }

    /// Bỏ chuỗi đang soạn (lùi con trỏ qua `marked` ký tự và xóa chúng).
    fn drop_marked(&mut self) {
        for _ in 0..self.marked {
            self.edit.backspace();
        }
        self.marked = 0;
    }
}

/// Cử chỉ sửa hình đang diễn ra (kéo di chuyển hoặc tay nắm).
struct Drag {
    g: Gesture,
    preview: Shape,
    preview_bounds: Rect,
    moved: bool,
    /// Điểm chuột cuối (đã kẹp vào vùng chọn) để Shift cập nhật bản xem trước mà không cần kéo.
    at: Pt,
    /// Lệch từ điểm bấm tới tâm tay nắm (tay nắm không nhảy về chuột).
    off: Pt,
}

/// Những gì thay đổi bố cục thanh/bảng khi so trước-sau một sự kiện (để vẽ lại cục bộ).
#[derive(Clone, Debug, PartialEq)]
pub struct ChromeSig {
    pub tool: Option<Tool>,
    pub shape_tool: Tool,
    pub size: SizeLevel,
    pub color: Rgb,
    pub flyout: Flyout,
    pub side_hover: Option<SideItem>,
    pub size_hover: Option<SizeLevel>,
    pub shape_hover: Option<Tool>,
    pub color_hover: Option<usize>,
    pub can_undo: bool,
    pub can_redo: bool,
    /// Bảng thuộc tính của Hình/Chữ: loại, giá trị đang hiện, ô màu đang áp, ô đang trỏ.
    pub style: Option<StyleView>,
    pub style_target: StyleTarget,
    pub style_hover: Option<StyleCell>,
}

/// Mã phím ảo (chữ hoa ASCII, như Windows) của ký tự đầu của `chars`; `[` và `]` thành `VK_OEM_4`,
/// `VK_OEM_6` để dùng `Tool::from_key` và `size_for_key` của core.
pub fn vk_of(chars: Option<&str>) -> Option<u16> {
    let c = chars?.chars().next()?;
    match c {
        '[' => Some(VK_OEM_4),
        ']' => Some(VK_OEM_6),
        c if c.is_ascii_alphabetic() => Some(u16::from(c.to_ascii_uppercase() as u8)),
        _ => None,
    }
}

/// Dung sai trúng hình (px) ở `dpi`.
pub fn shape_tol(dpi: u32) -> f32 {
    dip_to_px(HIT_TOL_DIP, dpi)
}

/// Bán kính trúng tay nắm của hình đang chọn (px) ở `dpi`.
pub fn grip_radius(dpi: u32) -> f32 {
    (scaled(HANDLE_SIZE, dpi) / 2 + scaled(HANDLE_HIT, dpi)) as f32
}

/// Bảng màu: cạnh nút Màu của thanh dọc `side`, chỉ có 48 màu cơ bản (không nhãn, không phần màu
/// tự chọn), kẹp vào màn hình.
pub fn palette_rect(side: Rect, sel: Rect, mon: Rect, dpi: u32) -> Rect {
    let btn = side_button(side, mon, dpi, SideItem::Color);
    let full = picker_layout(dpi, false).size;
    let h = scaled(PALETTE_BASIC_BOTTOM - PALETTE_LABEL_TRIM, dpi);
    flyout_rect(side, btn, sel, mon, dpi, (full.0, h))
}

/// Ô thứ `i` (theo `PALETTE`) của bảng màu `pal`.
pub fn palette_cell_rect(pal: Rect, dpi: u32, i: usize) -> Rect {
    let l = picker_layout(dpi, false);
    let trim = scaled(PALETTE_LABEL_TRIM, dpi);
    match l.basic.get(i) {
        Some(c) => Rect {
            x: pal.x + c.x,
            y: pal.y + c.y - trim,
            ..*c
        },
        None => NO_RECT,
    }
}

/// Chỉ số màu dưới điểm `p` của bảng màu `pal`; lề trả `None`.
pub fn palette_cell_at(pal: Rect, dpi: u32, p: Point) -> Option<usize> {
    (0..PALETTE.len()).find(|i| palette_cell_rect(pal, dpi, *i).contains(p))
}

fn side_button(side: Rect, mon: Rect, dpi: u32, item: SideItem) -> Rect {
    darkshot_core::overlay_layout::side_button(side, mon, dpi, item)
}

/// Hình chữ nhật của bảng `f` cạnh nút tương ứng của thanh dọc `side`; `None` khi `f` là
/// `Flyout::None`.
pub fn flyout_rect_for(f: Flyout, side: Rect, sel: Rect, mon: Rect, dpi: u32) -> Option<Rect> {
    Some(match f {
        Flyout::None => return None,
        Flyout::Sizes => {
            let btn = side_button(side, mon, dpi, SideItem::Size);
            darkshot_core::overlay_layout::size_flyout_rect(side, btn, sel, mon, dpi)
        }
        Flyout::Shapes => darkshot_core::overlay_layout::shapes_rect_for(side, sel, mon, dpi),
        Flyout::Palette => palette_rect(side, sel, mon, dpi),
    })
}

/// Đệm của ô gợi ý (96 DPI): ngang và dọc.
const TIP_PAD_X: i32 = 8;
const TIP_PAD_Y: i32 = 4;
/// Khe giữa ô gợi ý và thanh/bảng (96 DPI).
const TIP_GAP: i32 = 7;

fn tip_size(text: (i32, i32), dpi: u32) -> (i32, i32) {
    (
        text.0.max(0).saturating_add(2 * scaled(TIP_PAD_X, dpi)),
        text.1.max(0).saturating_add(2 * scaled(TIP_PAD_Y, dpi)),
    )
}

/// Ô gợi ý của một mục `btn` của thanh dọc `side`: cạnh thanh, phía xa vùng chọn (không đủ chỗ thì
/// phía kia), giữa theo chiều dọc nút; luôn trong màn hình `mon`.
pub fn side_tip_rect(
    side: Rect,
    btn: Rect,
    sel: Rect,
    mon: Rect,
    dpi: u32,
    text: (i32, i32),
) -> Rect {
    let (w, h) = tip_size(text, dpi);
    let gap = scaled(TIP_GAP, dpi);
    let right_x = side.right().saturating_add(gap);
    let left_x = side.x.saturating_sub(gap).saturating_sub(w);
    let fits_right = right_x.saturating_add(w) <= mon.right();
    let fits_left = left_x >= mon.x;
    let x = if darkshot_core::overlay_layout::side_on_right(side, sel) {
        if fits_right || !fits_left {
            right_x
        } else {
            left_x
        }
    } else if fits_left || !fits_right {
        left_x
    } else {
        right_x
    };
    darkshot_core::overlay_layout::clamp_into(
        Rect {
            x,
            y: btn.y + btn.h / 2 - h / 2,
            w,
            h,
        },
        mon,
    )
}

/// Ô gợi ý của ô `cell` trong bảng `fly`: trên bảng (không đủ chỗ thì dưới), giữa theo ô.
pub fn flyout_tip_rect(cell: Rect, fly: Rect, mon: Rect, dpi: u32, text: (i32, i32)) -> Rect {
    let (w, h) = tip_size(text, dpi);
    let gap = scaled(TIP_GAP, dpi);
    let above = fly.y.saturating_sub(gap).saturating_sub(h);
    let y = if above >= mon.y {
        above
    } else {
        fly.bottom().saturating_add(gap)
    };
    darkshot_core::overlay_layout::clamp_into(
        Rect {
            x: cell.x + cell.w / 2 - w / 2,
            y,
            w,
            h,
        },
        mon,
    )
}

/// Hình chữ nhật của bảng thuộc tính `kind` cạnh nút Cỡ của thanh dọc `side`.
///
/// Bảng này luôn hiện khi công cụ Hình/Chữ đang chọn nên ưu tiên đặt PHÍA NGOÀI thanh dọc (xa vùng
/// chọn) để không che vùng đang vẽ; không đủ chỗ trên màn hình thì mới đặt phía vùng chọn như các
/// bảng bật ra.
pub fn style_rect_for(kind: StyleKind, side: Rect, sel: Rect, mon: Rect, dpi: u32) -> Option<Rect> {
    let btn = side_button(side, mon, dpi, SideItem::Size);
    let (w, h) = panel_size(kind, dpi);
    let gap = scaled(darkshot_core::overlay_layout::GAP, dpi);
    let x = if darkshot_core::overlay_layout::side_on_right(side, sel) {
        side.right().saturating_add(gap)
    } else {
        side.x.saturating_sub(gap).saturating_sub(w)
    };
    if x >= mon.x && x.saturating_add(w) <= mon.right() {
        let r = Rect {
            x,
            y: btn.y + btn.h / 2 - h / 2,
            w,
            h,
        };
        return Some(darkshot_core::overlay_layout::clamp_into(r, mon));
    }
    Some(flyout_rect(side, btn, sel, mon, dpi, (w, h)))
}

/// Hình `shape` (Hình hoặc Chữ có kiểu) sau khi áp một thay đổi kiểu của bảng.
fn restyle_shape(shape: &Shape, edit: StyleEdit, dpi: u32) -> Shape {
    let Some(st) = shape.style() else {
        return shape.clone();
    };
    let new = match edit {
        StyleEdit::Fill(f) => Style { fill: f, ..st },
        StyleEdit::Stroke(c) => Style { stroke: c, ..st },
        StyleEdit::WidthDip(d) => Style {
            stroke_width: dip_to_px(clamp_width(d), dpi),
            ..st
        },
        StyleEdit::Opacity(v) => Style {
            opacity: clamp_opacity(v),
            ..st
        },
        StyleEdit::Align(_) => st,
    };
    let out = shape.with_style(new);
    match edit {
        StyleEdit::Align(a) => out.with_align(a),
        _ => out,
    }
}

/// Hợp các hộp bao đã đo của `shapes` thành danh sách song song (cùng chỉ số).
fn measure_all(shapes: &[Shape]) -> Vec<Rect> {
    shapes.iter().map(shape_bounds).collect()
}

/// Một lớp khi vẽ, từ dưới lên trên.
#[derive(Debug, PartialEq)]
pub enum Layer<'a> {
    /// Các hình liền nhau của sổ, theo chỉ số tăng dần.
    Run(&'a [Shape]),
    /// Hình đang kéo (bản xem trước), vẽ thay hình cùng chỉ số ở ĐÚNG vị trí của nó.
    Preview(&'a Shape),
    /// Nét đang vẽ dở, trên mọi hình đã có.
    Stroke(&'a Shape),
    /// Hình chữ nhật/elip đang kéo, đã mang kiểu (nền, độ trong suốt), trên mọi hình đã có.
    Boxed(Shape),
    /// Chữ đang gõ, trên cùng.
    Typing(Shape),
}

/// Trạng thái công cụ đánh dấu của một phiên lớp phủ.
pub struct Annot {
    tool: Option<Tool>,
    shape_tool: Tool,
    size: SizeLevel,
    color: Rgb,
    marks: Annotations,
    /// Hộp bao đã đo của từng hình, cùng thứ tự `marks.shapes()`.
    bounds: Vec<Rect>,
    stroke: Option<Stroke>,
    text: Option<TextBox>,
    selected: Option<usize>,
    gesture: Option<Drag>,
    flyout: Flyout,
    box_style: StylePrefs,
    label_style: StylePrefs,
    align: Align,
    highlight_mode: HighlightMode,
    marker_width_dip: f32,
    highlight_opacity: f32,
    /// Nét đang kéo là Highlight hình chữ nhật (kéo như Hình chữ nhật rồi đổi dạng khi chốt).
    highlight_drag: bool,
    style_target: StyleTarget,
    style_hover: Option<StyleCell>,
    style_pressed: Option<StyleCell>,
    side_hover: Option<SideItem>,
    side_pressed: Option<SideItem>,
    size_hover: Option<SizeLevel>,
    size_pressed: Option<SizeLevel>,
    shape_hover: Option<Tool>,
    shape_pressed: Option<Tool>,
    color_hover: Option<usize>,
    color_pressed: Option<usize>,
    caret_on: bool,
    /// Số lần mở ô chữ; chuỗi nháy con trỏ mang số này và dừng khi không còn khớp.
    text_epoch: u64,
    /// Vùng cần vẽ lại cục bộ (pixel ảnh màn hình) kể từ lần lấy cuối.
    dirty: Vec<Rect>,
}

impl Annot {
    pub fn new() -> Annot {
        Annot::with_prefs(Prefs::default())
    }

    pub fn with_prefs(p: Prefs) -> Annot {
        Annot {
            tool: None,
            shape_tool: p.shape_tool,
            size: p.size,
            color: p.color,
            marks: Annotations::default(),
            bounds: Vec::new(),
            stroke: None,
            text: None,
            selected: None,
            gesture: None,
            flyout: Flyout::None,
            box_style: p.box_style,
            label_style: p.label_style,
            align: p.align,
            highlight_mode: p.highlight_mode,
            marker_width_dip: clamp_marker_width(p.marker_width_dip),
            highlight_opacity: clamp_opacity(p.highlight_opacity),
            highlight_drag: false,
            style_target: StyleTarget::Fill,
            style_hover: None,
            style_pressed: None,
            side_hover: None,
            side_pressed: None,
            size_hover: None,
            size_pressed: None,
            shape_hover: None,
            shape_pressed: None,
            color_hover: None,
            color_pressed: None,
            caret_on: true,
            text_epoch: 0,
            dirty: Vec::new(),
        }
    }

    pub fn prefs(&self) -> Prefs {
        Prefs {
            shape_tool: self.shape_tool,
            size: self.size,
            color: self.color,
            box_style: self.box_style,
            label_style: self.label_style,
            align: self.align,
            highlight_mode: self.highlight_mode,
            marker_width_dip: self.marker_width_dip,
            highlight_opacity: self.highlight_opacity,
        }
    }

    // ------------------------------------------------------------------ truy vấn

    pub fn tool(&self) -> Option<Tool> {
        self.tool
    }

    pub fn shape_tool(&self) -> Tool {
        self.shape_tool
    }

    pub fn size(&self) -> SizeLevel {
        self.size
    }

    pub fn color(&self) -> Rgb {
        self.color
    }

    pub fn flyout(&self) -> Flyout {
        self.flyout
    }

    /// Hình đã vẽ, từ dưới lên trên (chỉ số cuối là trên cùng).
    #[cfg(test)]
    pub fn shapes(&self) -> &[Shape] {
        self.marks.shapes()
    }

    pub fn has_marks(&self) -> bool {
        !self.marks.is_empty()
    }

    #[cfg(test)]
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn text(&self) -> Option<&TextBox> {
        self.text.as_ref()
    }

    #[cfg(test)]
    pub fn stroke(&self) -> Option<&Stroke> {
        self.stroke.as_ref()
    }

    pub fn caret_on(&self) -> bool {
        self.caret_on
    }

    pub fn text_epoch(&self) -> u64 {
        self.text_epoch
    }

    pub fn side_hover(&self) -> Option<SideItem> {
        self.side_hover
    }

    pub fn size_hover(&self) -> Option<SizeLevel> {
        self.size_hover
    }

    pub fn shape_hover(&self) -> Option<Tool> {
        self.shape_hover
    }

    pub fn color_hover(&self) -> Option<usize> {
        self.color_hover
    }

    /// Nút Hoàn tác sáng khi có gì để hoàn tác (đang gõ thì Hoàn tác xóa ô chữ).
    pub fn can_undo(&self) -> bool {
        self.marks.can_undo() || self.text.is_some()
    }

    pub fn can_redo(&self) -> bool {
        self.marks.can_redo()
    }

    /// Chuột đang được giữ cho một nét hoặc một cử chỉ sửa hình.
    pub fn capturing(&self) -> bool {
        self.stroke.is_some() || self.gesture.is_some()
    }

    /// Có cú nhấn đang chờ nhả (thanh dọc hoặc bảng): `Model` không được kéo vùng chọn lúc này.
    pub fn press_pending(&self) -> bool {
        self.side_pressed.is_some()
            || self.size_pressed.is_some()
            || self.shape_pressed.is_some()
            || self.color_pressed.is_some()
            || self.style_pressed.is_some()
    }

    /// Hình trên cùng (chỉ số lớn nhất) trúng điểm `p` (theo công cụ Chọn); dùng để kiểm thử.
    pub fn topmost_at(&self, env: &Env, p: Point) -> Option<usize> {
        topmost_hit(
            self.marks.shapes(),
            &self.bounds,
            to_pt(p),
            shape_tol(env.dpi),
        )
    }

    /// Lấy và xóa danh sách vùng cần vẽ lại cục bộ.
    pub fn take_dirty(&mut self) -> Vec<Rect> {
        std::mem::take(&mut self.dirty)
    }

    pub fn chrome_sig(&self) -> ChromeSig {
        ChromeSig {
            tool: self.tool,
            shape_tool: self.shape_tool,
            size: self.size,
            color: self.color,
            flyout: self.flyout,
            side_hover: self.side_hover,
            size_hover: self.size_hover,
            shape_hover: self.shape_hover,
            color_hover: self.color_hover,
            can_undo: self.can_undo(),
            can_redo: self.can_redo(),
            style: self.style_view(96),
            style_target: self.style_target,
            style_hover: self.style_hover,
        }
    }

    // ------------------------------------------------------------------ bảng thuộc tính Hình/Chữ

    /// Loại hình bảng thuộc tính đang sửa: hình đang chọn (Hình có kiểu hoặc Chữ), hoặc ô chữ đang
    /// mở, hoặc công cụ Hình chữ nhật, Elip (Hình), Chữ; `None` khi không thuộc loại nào.
    pub fn style_kind(&self) -> Option<StyleKind> {
        if let Some(s) = self.selected.and_then(|i| self.marks.shapes().get(i)) {
            match s {
                Shape::Box { .. } => return Some(StyleKind::Box),
                Shape::Label { .. } => return Some(StyleKind::Label),
                Shape::Marker { .. } => return Some(StyleKind::MarkerFree),
                Shape::Highlight { .. } => return Some(StyleKind::MarkerRect),
                _ => {}
            }
        }
        if self.text.is_some() {
            return Some(StyleKind::Label);
        }
        match self.tool {
            Some(Tool::Rect | Tool::Ellipse) => Some(StyleKind::Box),
            Some(Tool::Text) => Some(StyleKind::Label),
            Some(Tool::Marker) => Some(match self.highlight_mode {
                HighlightMode::Freehand => StyleKind::MarkerFree,
                HighlightMode::Rect => StyleKind::MarkerRect,
            }),
            _ => None,
        }
    }

    /// Bảng thuộc tính đang hiện (không có bảng Cỡ/Hình/Màu mở).
    pub fn style_panel_open(&self) -> bool {
        self.flyout == Flyout::None && self.style_kind().is_some()
    }

    pub fn style_target(&self) -> StyleTarget {
        self.style_target
    }

    pub fn style_hover(&self) -> Option<StyleCell> {
        self.style_hover
    }

    /// Giá trị bảng thuộc tính đang hiện (độ dày theo dip ở `dpi`): của hình đang chọn nếu có,
    /// không thì kiểu mặc định cho hình vẽ sau.
    pub fn style_view(&self, dpi: u32) -> Option<StyleView> {
        let kind = self.style_kind()?;
        let px_to_dip = |px: f32| px * 96.0 / dpi.max(1) as f32;
        if kind.is_highlight() {
            return Some(self.highlight_view(kind, dpi));
        }
        if let Some(shape) = self.selected.and_then(|i| self.marks.shapes().get(i)) {
            if let Some(st) = shape.style() {
                return Some(StyleView {
                    kind,
                    fill: st.fill,
                    stroke: st.stroke,
                    width_dip: px_to_dip(st.stroke_width),
                    opacity: st.opacity,
                    align: shape.align(),
                    mode: None,
                });
            }
        }
        if let (StyleKind::Label, Some(b)) = (kind, self.text.as_ref()) {
            return Some(StyleView {
                kind,
                fill: b.style.fill,
                stroke: b.style.stroke,
                width_dip: px_to_dip(b.style.stroke_width),
                opacity: b.style.opacity,
                align: Some(b.align),
                mode: None,
            });
        }
        Some(match kind {
            StyleKind::Box => StyleView {
                kind,
                fill: self.box_style.fill,
                stroke: self.color,
                width_dip: self.box_style.width_dip,
                opacity: self.box_style.opacity,
                align: None,
                mode: None,
            },
            StyleKind::Label => StyleView {
                kind,
                fill: self.label_style.fill,
                stroke: self.label_style.stroke,
                width_dip: self.label_style.width_dip,
                opacity: self.label_style.opacity,
                align: Some(self.align),
                mode: None,
            },
            StyleKind::MarkerFree | StyleKind::MarkerRect => return None,
        })
    }

    /// Giá trị bảng Highlight: của nét hoặc hình đang chọn nếu có, không thì mặc định cho lần vẽ
    /// sau. `mode` luôn là chế độ cho lần vẽ tiếp.
    fn highlight_view(&self, kind: StyleKind, dpi: u32) -> StyleView {
        let mut v = StyleView {
            kind,
            fill: None,
            stroke: self.color,
            width_dip: self.marker_width_dip,
            opacity: self.highlight_opacity,
            align: None,
            mode: Some(self.highlight_mode),
        };
        match self.selected.and_then(|i| self.marks.shapes().get(i)) {
            Some(Shape::Marker { width, color, .. }) => {
                v.stroke = *color;
                v.width_dip = *width * 96.0 / dpi.max(1) as f32;
            }
            Some(Shape::Highlight { color, opacity, .. }) => {
                v.stroke = *color;
                v.opacity = *opacity;
            }
            _ => {}
        }
        v
    }

    /// Hình chữ nhật của bảng thuộc tính (cạnh nút Cỡ của thanh dọc); `None` khi bảng không hiện.
    pub fn style_rect(&self, env: &Env) -> Option<Rect> {
        let kind = self.style_kind().filter(|_| self.flyout == Flyout::None)?;
        style_rect_for(kind, env.side?, env.sel, env.mon, env.dpi)
    }

    /// Ô của bảng thuộc tính dưới `p`.
    pub fn style_cell_under(&self, env: &Env, p: Point) -> Option<StyleCell> {
        let kind = self.style_kind()?;
        let rect = self.style_rect(env)?;
        style_cell_at(&style_layout(rect, kind, env.dpi), p)
    }

    /// Áp một thay đổi kiểu từ bảng: cập nhật mặc định cho hình vẽ sau, rồi hình đang chọn (một
    /// bước hoàn tác, giữ lớp) hoặc ô chữ đang gõ.
    pub fn apply_style(&mut self, edit: StyleEdit, dpi: u32) {
        let Some(kind) = self.style_kind() else {
            return;
        };
        match kind {
            StyleKind::Box => self.box_style.apply(edit),
            StyleKind::Label => self.label_style.apply(edit),
            StyleKind::MarkerFree | StyleKind::MarkerRect => {
                return self.apply_highlight(edit, dpi);
            }
        }
        if let StyleEdit::Align(a) = edit {
            self.align = a;
        }
        // Viền của Hình là màu của nút Màu: đổi nó như chọn màu (hình đang chọn đổi theo).
        if let (StyleKind::Box, StyleEdit::Stroke(c)) = (kind, edit) {
            self.choose_color(c, dpi);
            return;
        }
        if let Some((i, shape)) = self.selected_styled() {
            if self.gesture.is_some() {
                return;
            }
            let new = restyle_shape(&shape, edit, dpi);
            let before = self.selection_area(dpi);
            if let Some(a) = self.marks.replace(i, new) {
                self.apply_change(&a);
                let after = self.selection_area(dpi);
                for r in before.into_iter().chain(after) {
                    self.mark(r);
                }
            }
            return;
        }
        if kind == StyleKind::Label {
            self.edit_text(|b| {
                let mut st = StylePrefs {
                    fill: b.style.fill,
                    stroke: b.style.stroke,
                    width_dip: b.style.stroke_width * 96.0 / b.dpi.max(1) as f32,
                    opacity: b.style.opacity,
                };
                st.apply(edit);
                b.style = st.style(st.stroke, b.dpi);
                if let StyleEdit::Align(a) = edit {
                    b.align = a;
                }
            });
        }
    }

    /// Áp thay đổi của bảng Highlight: độ dày nét (tự do) hoặc độ trong suốt (hình chữ nhật) vào
    /// mặc định cho lần vẽ sau, rồi vào nét hoặc hình đang chọn (một bước hoàn tác, giữ lớp).
    fn apply_highlight(&mut self, edit: StyleEdit, dpi: u32) {
        let (mut width_px, mut opacity) = (None, None);
        match edit {
            StyleEdit::WidthDip(d) => {
                self.marker_width_dip = clamp_marker_width(d);
                width_px = Some(dip_to_px(self.marker_width_dip, dpi));
            }
            StyleEdit::Opacity(o) => {
                self.highlight_opacity = clamp_opacity(o);
                opacity = Some(self.highlight_opacity);
            }
            _ => return,
        }
        if self.gesture.is_some() {
            return;
        }
        let Some(i) = self.selected else {
            return;
        };
        let Some(shape) = self.marks.shapes().get(i).cloned() else {
            return;
        };
        let new = highlight_restyle(&shape, width_px, opacity, None);
        if new == shape {
            return;
        }
        let before = self.selection_area(dpi);
        if let Some(a) = self.marks.replace(i, new) {
            self.apply_change(&a);
            let after = self.selection_area(dpi);
            for r in before.into_iter().chain(after) {
                self.mark(r);
            }
        }
    }

    /// Hình đang chọn nếu là Hình có kiểu hoặc Chữ có kiểu (chỉ số, bản sao).
    fn selected_styled(&self) -> Option<(usize, Shape)> {
        let i = self.selected?;
        let s = self.marks.shapes().get(i)?;
        s.style().map(|_| (i, s.clone()))
    }

    /// Kích hoạt ô `cell` của bảng thuộc tính.
    fn press_style_cell(&mut self, cell: StyleCell, dpi: u32) {
        let Some(view) = self.style_view(dpi) else {
            return;
        };
        if view.kind.is_highlight() {
            match cell {
                StyleCell::Mode(m) => self.highlight_mode = m,
                StyleCell::Color(i) => {
                    if let Some(c) = PALETTE.get(i) {
                        self.choose_color(*c, dpi);
                    }
                }
                StyleCell::Width(d) => self.apply_style(
                    StyleEdit::WidthDip(step_marker_width(view.width_dip, d)),
                    dpi,
                ),
                StyleCell::Opacity(d) => {
                    self.apply_style(StyleEdit::Opacity(step_opacity(view.opacity, d)), dpi)
                }
                _ => {}
            }
            return;
        }
        match cell {
            StyleCell::Mode(_) => {}
            StyleCell::Target(t) => self.style_target = t,
            StyleCell::NoFill => self.apply_style(StyleEdit::Fill(None), dpi),
            StyleCell::Color(i) => {
                if let Some(c) = PALETTE.get(i) {
                    match self.style_target {
                        StyleTarget::Fill => self.apply_style(StyleEdit::Fill(Some(*c)), dpi),
                        StyleTarget::Border => self.apply_style(StyleEdit::Stroke(*c), dpi),
                    }
                }
            }
            StyleCell::Width(d) => {
                self.apply_style(StyleEdit::WidthDip(step_width(view.width_dip, d)), dpi)
            }
            StyleCell::Opacity(d) => {
                self.apply_style(StyleEdit::Opacity(step_opacity(view.opacity, d)), dpi)
            }
            StyleCell::Align(a) => self.apply_style(StyleEdit::Align(a), dpi),
        }
    }

    /// Hình đang chọn như đang hiện (bản xem trước khi đang kéo) cùng hộp bao.
    pub fn shown_selection(&self) -> Option<(&Shape, Rect)> {
        if let Some(d) = &self.gesture {
            return Some((&d.preview, d.preview_bounds));
        }
        let i = self.selected?;
        let s = self.marks.shapes().get(i)?;
        let b = self
            .bounds
            .get(i)
            .copied()
            .unwrap_or_else(|| shape_bounds(s));
        Some((s, b))
    }

    /// Hình đang kéo (chỉ số trong sổ, bản xem trước); vẽ thay hình đó ở ĐÚNG vị trí của nó trong
    /// thứ tự, không nhảy lên trên cùng.
    pub fn preview(&self) -> Option<(usize, &Shape)> {
        self.gesture.as_ref().map(|d| (d.g.index, &d.preview))
    }

    /// Các lớp cần vẽ, từ dưới lên trên: hình đã có (hình đang kéo thay đúng chỗ của nó), rồi nét
    /// đang vẽ dở, rồi chữ đang gõ. Hình vẽ SAU nằm PHÍA TRƯỚC.
    pub fn layers(&self) -> Vec<Layer<'_>> {
        let shapes = self.marks.shapes();
        let mut v = Vec::new();
        match self.preview() {
            Some((gi, s)) if gi < shapes.len() => {
                v.push(Layer::Run(&shapes[..gi]));
                v.push(Layer::Preview(s));
                v.push(Layer::Run(&shapes[gi + 1..]));
            }
            _ => v.push(Layer::Run(shapes)),
        }
        if let Some(st) = &self.stroke {
            match &st.shape {
                Shape::Rect { .. } if self.highlight_drag => v.push(Layer::Boxed(
                    highlight_from_stroke(&st.shape, self.highlight_opacity),
                )),
                Shape::Rect { .. } | Shape::Ellipse { .. } => v.push(Layer::Boxed(
                    styled_from_stroke(&st.shape, self.box_style.fill, self.box_style.opacity),
                )),
                other => v.push(Layer::Stroke(other)),
            }
        }
        if let Some(shape) = self
            .text
            .as_ref()
            .and_then(|t| t.edit.to_label(t.align, t.style))
        {
            v.push(Layer::Typing(shape));
        }
        v
    }

    // ------------------------------------------------------------------ vẽ lại cục bộ

    fn mark(&mut self, r: Rect) {
        if r.w > 0 && r.h > 0 {
            self.dirty.push(r);
        }
    }

    fn selection_pad(&self, dpi: u32) -> i32 {
        scaled(HANDLE_SIZE, dpi) / 2 + 1
    }

    fn selection_area(&self, dpi: u32) -> Option<Rect> {
        self.shown_selection()
            .map(|(s, b)| selection_dirty(s, b, self.selection_pad(dpi)))
    }

    /// Đổi trạng thái chọn hình qua `f`, làm mới vùng chọn hình cũ và mới.
    fn change_selection(&mut self, dpi: u32, f: impl FnOnce(&mut Annot)) {
        let before = self.selection_area(dpi);
        f(self);
        let after = self.selection_area(dpi);
        for r in before.into_iter().chain(after) {
            self.mark(r);
        }
    }

    /// Áp thay đổi `a` (đã vào `marks`) lên `bounds` và đánh dấu vùng cũ/mới cần vẽ lại.
    fn apply_change(&mut self, a: &Applied) {
        let shapes = self.marks.shapes();
        let mut dirty = Vec::new();
        match &a.change {
            Change::Inserted { index } => {
                if let (Some(s), true) = (shapes.get(*index), *index <= self.bounds.len()) {
                    let r = shape_bounds(s);
                    self.bounds.insert(*index, r);
                    dirty.push(r);
                }
            }
            Change::Removed { index, shape } => {
                if *index < self.bounds.len() {
                    dirty.push(self.bounds.remove(*index));
                } else {
                    dirty.push(shape_bounds(shape));
                }
            }
            Change::Replaced { index, old } => {
                if let Some(s) = shapes.get(*index) {
                    let r = shape_bounds(s);
                    match self.bounds.get_mut(*index) {
                        Some(slot) => dirty.push(std::mem::replace(slot, r)),
                        None => dirty.push(shape_bounds(old)),
                    }
                    dirty.push(r);
                }
            }
        }
        for i in &a.renumbered {
            if let (Some(s), Some(slot)) = (shapes.get(*i), self.bounds.get_mut(*i)) {
                let r = shape_bounds(s);
                dirty.push(std::mem::replace(slot, r));
                dirty.push(r);
            }
        }
        if self.bounds.len() != shapes.len() {
            self.bounds = measure_all(shapes);
            dirty.extend(self.bounds.iter().copied());
        }
        for r in dirty {
            self.mark(r);
        }
    }

    // ------------------------------------------------------------------ sổ hình

    /// Thêm hình vào CUỐI sổ (trên cùng), ghi sổ hoàn tác.
    pub fn add_mark(&mut self, shape: Shape) {
        let a = self.marks.add(shape);
        self.apply_change(&a);
    }

    /// Hoàn tác. Đang gõ: ô có chữ thì chỉ xóa ô đó; ô rỗng thì hủy ô rồi hoàn tác hình cuối.
    /// Đang kéo: không làm gì.
    pub fn undo(&mut self, dpi: u32) {
        if self.capturing() {
            return;
        }
        if let Some(b) = &self.text {
            let had_text = !b.edit.text().is_empty();
            self.close_text(false);
            if had_text {
                return;
            }
        }
        self.deselect(dpi);
        if let Some(a) = self.marks.undo() {
            self.apply_change(&a);
        }
    }

    /// Làm lại. Ô chữ đang mở hoặc đang kéo: không làm gì.
    pub fn redo(&mut self, dpi: u32) {
        if self.text.is_some() || self.capturing() {
            return;
        }
        self.deselect(dpi);
        if let Some(a) = self.marks.redo() {
            self.apply_change(&a);
        }
    }

    /// Xóa hình đang chọn (ghi sổ), bỏ chọn.
    pub fn delete_selected(&mut self, dpi: u32) {
        let Some(i) = self.selected else {
            return;
        };
        self.deselect(dpi);
        if let Some(a) = self.marks.remove(i) {
            self.apply_change(&a);
        }
    }

    /// Chốt mọi thứ đang dở (ô chữ vào sổ ở CUỐI, bỏ chọn, bỏ nét dở) và lấy hình để đốt vào ảnh,
    /// từ dưới lên trên theo chỉ số. Sau lệnh này sổ rỗng.
    pub fn take_shapes(&mut self, dpi: u32) -> Vec<Shape> {
        self.close_text(true);
        self.stroke = None;
        self.deselect(dpi);
        self.bounds.clear();
        std::mem::take(&mut self.marks).into_shapes()
    }

    // ------------------------------------------------------------------ chọn hình

    /// Bỏ chọn hình (hủy cử chỉ đang kéo nếu có, không ghi sổ).
    pub fn deselect(&mut self, dpi: u32) {
        self.cancel_gesture(dpi);
        if self.selected.is_some() {
            self.change_selection(dpi, |s| s.selected = None);
        }
    }

    /// Hủy cử chỉ kéo hình: hình về như cũ, không ghi sổ; vẫn chọn hình.
    pub fn cancel_gesture(&mut self, dpi: u32) {
        if self.gesture.is_some() {
            self.change_selection(dpi, |s| s.gesture = None);
        }
    }

    fn shape_grip_at(&self, env: &Env, p: Point) -> Option<Grip> {
        if self.tool != Some(Tool::Select) {
            return None;
        }
        let (s, b) = self.shown_selection()?;
        grip_at(s, b, to_pt(p), grip_radius(env.dpi))
    }

    /// Tay nắm sẽ được nắm khi bấm tại `p`. Bấm trúng chính hình đang chọn mà xa tâm mọi tay nắm
    /// hơn nửa cỡ tay nắm (hình nhỏ: vùng trúng tay nắm phủ lòng hình) thì là di chuyển, không nắm.
    fn press_grip(&self, env: &Env, p: Point) -> Option<Grip> {
        let grip = self.shape_grip_at(env, p)?;
        if self.selected.is_some() && self.shape_hit_at(env, p) == self.selected {
            let (s, b) = self.shown_selection()?;
            let q = to_pt(p);
            let near = handles(s, b)
                .into_iter()
                .map(|(_, c)| (c.x - q.x).abs().max((c.y - q.y).abs()))
                .fold(f32::INFINITY, f32::min);
            if near > (scaled(HANDLE_SIZE, env.dpi) / 2) as f32 {
                return None;
            }
        }
        Some(grip)
    }

    /// Hình PHÍA TRƯỚC nhất (chỉ số lớn nhất) trúng `p` trong vùng chọn khi công cụ Chọn bật.
    fn shape_hit_at(&self, env: &Env, p: Point) -> Option<usize> {
        if self.tool != Some(Tool::Select) || !env.sel.contains(p) {
            return None;
        }
        self.topmost_at(env, p)
    }

    fn clamp_to_sel(&self, env: &Env, p: Point) -> Pt {
        to_pt(Point {
            x: p.x.max(env.sel.x).min(env.sel.right() - 1),
            y: p.y.max(env.sel.y).min(env.sel.bottom() - 1),
        })
    }

    /// Chọn hình `index` và bắt đầu kéo nó bằng tay nắm `grip` từ `p`.
    fn begin_gesture(&mut self, env: &Env, index: usize, grip: Grip, p: Point) -> Down {
        let Some(original) = self.marks.shapes().get(index).cloned() else {
            return Down::Handled;
        };
        let original_bounds = self
            .bounds
            .get(index)
            .copied()
            .unwrap_or_else(|| shape_bounds(&original));
        let start = self.clamp_to_sel(env, p);
        let off = match grip {
            Grip::Move => None,
            _ => handles(&original, original_bounds)
                .into_iter()
                .find(|(g, _)| *g == grip)
                .map(|(_, c)| Pt {
                    x: c.x - start.x,
                    y: c.y - start.y,
                }),
        }
        .filter(|o| o.x.is_finite() && o.y.is_finite())
        .unwrap_or(Pt { x: 0.0, y: 0.0 });
        self.change_selection(env.dpi, |s| {
            s.selected = Some(index);
            s.gesture = Some(Drag {
                g: Gesture {
                    index,
                    grip,
                    start,
                    original: original.clone(),
                    original_bounds,
                },
                preview: original,
                preview_bounds: original_bounds,
                moved: false,
                at: start,
                off,
            });
        });
        self.side_pressed = None;
        Down::Capture
    }

    /// Kéo cử chỉ tới `at` (đã kẹp vào vùng chọn): tính hình xem trước, làm mới hộp cũ và mới.
    fn drag_gesture(&mut self, env: &Env, at: Pt, shift: bool) {
        let Some(d) = self.gesture.as_mut() else {
            return;
        };
        d.at = at;
        if at != d.g.start {
            d.moved = true;
        }
        if !d.moved {
            return;
        }
        let to = Pt {
            x: at.x + d.off.x,
            y: at.y + d.off.y,
        };
        let shape = transform(&d.g, to, shift, dip_to_px(MIN_EXTENT_DIP, env.dpi), env.img);
        if d.preview == shape {
            return;
        }
        let bounds = text_preview_bounds(&d.g.original, d.g.original_bounds, &shape)
            .unwrap_or_else(|| shape_bounds(&shape));
        self.change_selection(env.dpi, |s| {
            if let Some(d) = s.gesture.as_mut() {
                d.preview = shape;
                d.preview_bounds = bounds;
            }
        });
    }

    /// Chốt cử chỉ: thay hình bằng bản xem trước (một mục sổ, hình GIỮ NGUYÊN chỉ số tức giữ
    /// lớp; không đổi thì không ghi); giữ chọn.
    fn commit_gesture(&mut self, dpi: u32) {
        let before = self.selection_area(dpi);
        let Some(d) = self.gesture.take() else {
            return;
        };
        let preview = d.moved.then_some(d.preview);
        if let Some(a) = preview.and_then(|s| self.marks.replace(d.g.index, s)) {
            self.apply_change(&a);
        }
        let after = self.selection_area(dpi);
        for r in before.into_iter().chain(after) {
            self.mark(r);
        }
    }

    /// Áp mức `level` và/hoặc màu `color` cho hình đang chọn (một mục sổ; Làm mờ bỏ qua). `replace`
    /// giữ chỉ số nên không đổi lớp.
    fn restyle_selected(&mut self, dpi: u32, level: Option<SizeLevel>, color: Option<Rgb>) {
        if self.gesture.is_some() {
            return;
        }
        let Some((shape, _)) = self.shown_selection() else {
            return;
        };
        let Some(i) = self.selected else {
            return;
        };
        let new = restyle(shape, level, color, dpi);
        let before = self.selection_area(dpi);
        if let Some(a) = self.marks.replace(i, new) {
            self.apply_change(&a);
            let after = self.selection_area(dpi);
            for r in before.into_iter().chain(after) {
                self.mark(r);
            }
        }
    }

    // ------------------------------------------------------------------ công cụ, cỡ, màu, bảng

    fn remember_shape(&mut self, t: Tool) {
        if is_shape(t) {
            self.shape_tool = t;
        }
    }

    fn close_flyouts(&mut self) {
        self.flyout = Flyout::None;
        self.size_hover = None;
        self.size_pressed = None;
        self.shape_hover = None;
        self.shape_pressed = None;
        self.color_hover = None;
        self.color_pressed = None;
        self.style_hover = None;
        self.style_pressed = None;
    }

    /// Bật `t` (tắt nếu đang bật); bật một hình thì nhớ nó cho nút Hình. Bảng đóng.
    pub fn toggle_tool(&mut self, t: Tool, dpi: u32) {
        self.close_text(true);
        self.deselect(dpi);
        let on = self.tool != Some(t);
        if on {
            self.remember_shape(t);
        }
        self.close_flyouts();
        self.tool = on.then_some(t);
    }

    /// Bật `t` (không bật/tắt), nhớ nếu là hình, đóng mọi bảng.
    pub fn use_tool(&mut self, t: Tool, dpi: u32) {
        if self.tool != Some(t) {
            self.close_text(true);
            self.deselect(dpi);
        }
        self.remember_shape(t);
        self.close_flyouts();
        self.tool = Some(t);
    }

    /// Đặt mức cỡ cho hình vẽ sau đó; hình đang chọn và ô chữ đang mở đổi cỡ theo.
    pub fn set_size(&mut self, level: SizeLevel, dpi: u32) {
        self.size = level;
        // Độ dày viền mặc định của Hình theo mức cỡ (bảng thuộc tính có thể chỉnh riêng sau đó).
        self.box_style.width_dip = clamp_width(level.stroke_dip());
        self.marker_width_dip = clamp_marker_width(level.marker_dip());
        self.restyle_selected(dpi, Some(level), None);
        let color = self.color;
        let Some(b) = self.text.as_mut() else {
            return;
        };
        if b.size == dip_to_px(level.text_dip(), b.dpi) {
            return;
        }
        let before = b.dirty();
        b.resize(level, color);
        let after = b.dirty();
        self.caret_on = true;
        self.mark(before);
        self.mark(after);
    }

    /// Dùng màu `c` cho công cụ vẽ, ô chữ đang gõ và hình đang chọn; đóng bảng màu.
    pub fn choose_color(&mut self, c: Rgb, dpi: u32) {
        self.color = c;
        self.close_flyouts();
        self.edit_text(|b| b.edit.set_color(c));
        self.restyle_selected(dpi, None, Some(c));
    }

    /// Kích hoạt một mục của thanh dọc (nhả chuột trên mục đó).
    pub fn activate_side(&mut self, item: SideItem, dpi: u32) {
        match item {
            SideItem::Tool(t) => self.toggle_tool(t, dpi),
            // Đang dùng một hình: bật/tắt bảng Hình; chưa: bật hình nhớ, không mở bảng.
            SideItem::Shapes if !self.tool.is_some_and(is_shape) => {
                self.use_tool(self.shape_tool, dpi)
            }
            SideItem::Shapes if self.flyout == Flyout::Shapes => self.close_flyouts(),
            SideItem::Shapes => self.open_flyout(Flyout::Shapes),
            SideItem::Size if self.flyout == Flyout::Sizes => self.close_flyouts(),
            SideItem::Size => self.open_flyout(Flyout::Sizes),
            SideItem::Color if self.flyout == Flyout::Palette => self.close_flyouts(),
            SideItem::Color => self.open_flyout(Flyout::Palette),
        }
    }

    fn open_flyout(&mut self, f: Flyout) {
        self.close_flyouts();
        self.flyout = f;
    }

    // ------------------------------------------------------------------ chữ

    /// Mở ô chữ tại `p` (đã chốt ô cũ).
    pub fn open_text(&mut self, env: &Env, p: Point) {
        let style = self.label_style.style(self.label_style.stroke, env.dpi);
        let b = TextBox::new(p, env.dpi, self.size, self.color, style, self.align);
        self.mark(b.dirty());
        self.text = Some(b);
        self.caret_on = true;
        self.text_epoch = self.text_epoch.wrapping_add(1);
    }

    /// Đóng ô chữ: `commit` thì chốt vào sổ ở CUỐI (bỏ nếu chỉ có khoảng trắng), không thì hủy.
    pub fn close_text(&mut self, commit: bool) {
        let Some(mut b) = self.text.take() else {
            return;
        };
        self.mark(b.dirty());
        // Chuỗi đang soạn dở: chốt thì giữ như đã hiện, hủy thì bỏ.
        if commit {
            b.marked = 0;
        } else {
            b.drop_marked();
        }
        if commit {
            if let Some(shape) = b.edit.to_label(b.align, b.style) {
                // Chữ vào CUỐI sổ: nằm trên mọi hình đã vẽ trước đó.
                self.add_mark(shape);
            }
        }
    }

    /// Sửa ô chữ đang gõ qua `f`, đo lại, làm mới vùng cũ và mới.
    fn edit_text(&mut self, f: impl FnOnce(&mut TextBox)) {
        let Some(b) = self.text.as_mut() else {
            return;
        };
        let before = b.dirty();
        f(b);
        b.measure();
        let after = b.dirty();
        self.caret_on = true;
        self.mark(before);
        self.mark(after);
    }

    /// Chèn chữ đã chốt từ bộ gõ (`insertText:`); thay chuỗi đang soạn nếu có.
    pub fn text_insert(&mut self, s: &str) {
        self.edit_text(|b| {
            b.drop_marked();
            b.edit.insert(s);
        });
    }

    /// Chuỗi đang soạn của bộ gõ (`setMarkedText:`): thay chuỗi soạn trước đó bằng `s`.
    pub fn text_marked(&mut self, s: &str) {
        self.edit_text(|b| {
            b.drop_marked();
            let before = b.edit.caret();
            b.edit.insert(s);
            b.marked = b.edit.caret().saturating_sub(before);
        });
    }

    /// Bộ gõ chốt chuỗi đang soạn như đã hiện (`unmarkText`).
    pub fn text_unmark(&mut self) {
        if let Some(b) = self.text.as_mut() {
            b.marked = 0;
        }
    }

    /// Dán chữ thuần (đã lọc bằng `sanitize_paste`).
    pub fn text_paste(&mut self, raw: &str) {
        let clean = sanitize_paste(raw);
        if !clean.is_empty() {
            self.text_insert(&clean);
        }
    }

    pub fn text_command(&mut self, c: TextCmd) {
        self.edit_text(|b| match c {
            TextCmd::Backspace => b.edit.backspace(),
            TextCmd::Delete => b.edit.delete(),
            TextCmd::Left => b.edit.left(),
            TextCmd::Right => b.edit.right(),
            TextCmd::Up => b.edit.up(),
            TextCmd::Down => b.edit.down(),
            TextCmd::Home => b.edit.home(),
            TextCmd::End => b.edit.end(),
            TextCmd::Newline => b.edit.insert("\n"),
        });
    }

    /// Ô chữ đang mở và bộ gõ đang soạn dở.
    pub fn text_composing(&self) -> bool {
        self.text.as_ref().is_some_and(|b| b.marked > 0)
    }

    /// Ép đóng ô chữ bỏ cả chuỗi đang soạn (lối thoát khi bộ gõ không trả Esc).
    pub fn force_close_text(&mut self) {
        self.close_text(false);
    }

    /// Nhịp nháy: đảo con trỏ, chỉ làm mới ô con trỏ (không đo lại chữ).
    pub fn blink(&mut self) {
        let Some(caret) = self.text.as_ref().map(|b| b.caret) else {
            return;
        };
        self.caret_on = !self.caret_on;
        self.mark(caret);
    }

    // ------------------------------------------------------------------ phím

    /// Esc theo thứ tự Windows: đóng bảng, kết thúc ô chữ (hủy), hủy cử chỉ kéo hình, bỏ chọn hình.
    /// `false` khi không còn gì để đóng: bên gọi hủy cả lớp phủ.
    pub fn escape(&mut self, dpi: u32) -> bool {
        if self.flyout != Flyout::None {
            self.close_flyouts();
            true
        } else if self.text.is_some() {
            self.close_text(false);
            true
        } else if self.gesture.is_some() {
            self.cancel_gesture(dpi);
            true
        } else if self.selected.is_some() {
            self.deselect(dpi);
            true
        } else {
            false
        }
    }

    /// Phím công cụ và cỡ (không phím bổ trợ, không lặp, không đang kéo): `chars` là
    /// `charactersIgnoringModifiers`. Trả `true` khi dùng phím.
    pub fn tool_key(&mut self, chars: Option<&str>, dpi: u32) -> bool {
        let Some(vk) = vk_of(chars) else {
            return false;
        };
        if let Some(t) = Tool::from_key(vk) {
            self.toggle_tool(t, dpi);
            return true;
        }
        if let Some(level) = darkshot_core::color_picker::size_for_key(self.size, vk) {
            self.set_size(level, dpi);
            return true;
        }
        false
    }

    // ------------------------------------------------------------------ chuột

    fn flyout_rect_of(&self, env: &Env) -> Option<Rect> {
        flyout_rect_for(self.flyout, env.side?, env.sel, env.mon, env.dpi)
    }

    /// Mục thanh dọc dưới `p`.
    pub fn side_item_under(&self, env: &Env, p: Point) -> Option<SideItem> {
        let side = env.side?;
        let l = side_layout_for(env.mon, env.dpi);
        side_item_at(side, &l, p)
    }

    /// Một cú nhấn chuột trái ở `p` (`hit`: kết quả `Selection::hit_test`). Mọi thứ trước
    /// `press_mode` (ô chữ, bảng, thanh) theo đúng thứ tự của bản Windows.
    pub fn down(&mut self, env: &Env, p: Point, hit: Hit) -> Down {
        if self.capturing() {
            return Down::Handled;
        }
        // Bấm ra chỗ khác thì chốt ô chữ; trên thanh dọc, trên Hoàn tác/Làm lại và khi một bảng
        // mở (để đổi màu, cỡ ô đang gõ) thì chưa.
        let on_side = env.side.is_some_and(|r| r.contains(p));
        let item = env.bar.and_then(|b| bar_item_at(b, p, env.dpi));
        let on_history = matches!(item, Some(BarItem::Undo | BarItem::Redo));
        let flyout = self.flyout != Flyout::None;
        let style_rect = self.style_rect(env);
        let on_style = style_rect.is_some_and(|r| r.contains(p));
        if self.text.is_some() && !flyout && !on_side && !on_history && !on_style {
            self.close_text(true);
        }
        // Bảng thuộc tính (không phải bảng bật ra từ thanh dọc): bấm ô thì nhớ để làm khi nhả,
        // bấm lề bảng thì nuốt (không vẽ xuyên bảng).
        if on_style {
            self.style_pressed = self.style_cell_under(env, p);
            return Down::Handled;
        }
        if let Some(fly) = self.flyout_rect_of(env) {
            match self.flyout {
                Flyout::Shapes => {
                    if let Some(t) = shape_cell_at(fly, env.dpi, p) {
                        self.shape_pressed = Some(t);
                        return Down::Handled;
                    }
                }
                Flyout::Sizes => {
                    if let Some(l) = size_cell_at(fly, env.dpi, p) {
                        self.size_pressed = Some(l);
                        return Down::Handled;
                    }
                }
                Flyout::Palette => {
                    if let Some(i) = palette_cell_at(fly, env.dpi, p) {
                        self.color_pressed = Some(i);
                        return Down::Handled;
                    }
                }
                Flyout::None => {}
            }
            // Bấm ngoài ô của bảng (lề bảng thì không làm gì) chỉ đóng bảng, kể cả trên nút Màu.
            if !fly.contains(p) {
                self.close_flyouts();
            }
            return Down::Handled;
        }
        if let Some(bar) = env.bar {
            if bar.contains(p) {
                return match item {
                    Some(it) => Down::Bar(it),
                    None => Down::Handled,
                };
            }
        }
        if let Some(side) = env.side {
            if side.contains(p) {
                self.side_pressed = self.side_item_under(env, p);
                return Down::Handled;
            }
        }
        // Công cụ Chọn: tay nắm của hình đang chọn, rồi hình PHÍA TRƯỚC nhất trúng điểm bấm (trước
        // tay nắm vùng chọn).
        if let (Some(grip), Some(i)) = (self.press_grip(env, p), self.selected) {
            return self.begin_gesture(env, i, grip, p);
        }
        if let Some(i) = self.shape_hit_at(env, p) {
            return self.begin_gesture(env, i, Grip::Move, p);
        }
        match press_mode(self.tool, self.has_marks(), hit) {
            PressMode::Ignore => Down::Handled,
            PressMode::Pick => {
                self.deselect(env.dpi);
                Down::Handled
            }
            PressMode::Draw => self.draw_down(env, p),
            PressMode::Selection => {
                if hit == Hit::Outside {
                    self.deselect(env.dpi);
                }
                Down::Selection
            }
        }
    }

    fn draw_down(&mut self, env: &Env, p: Point) -> Down {
        let Some(tool) = self.tool else {
            return Down::Handled;
        };
        if tool == Tool::Text {
            self.close_text(true);
            self.open_text(env, p);
            return Down::Handled;
        }
        let at = Pt {
            x: p.x as f32,
            y: p.y as f32,
        };
        if tool == Tool::Step {
            // Đặt ngay một vòng tại điểm bấm (không kéo); lõi đánh số, vào CUỐI sổ.
            self.add_mark(Shape::Step {
                center: at,
                radius: dip_to_px(self.size.step_dip(), env.dpi),
                n: 0,
                color: self.color,
            });
            return Down::Handled;
        }
        // Highlight hình chữ nhật: kéo như Hình chữ nhật, đổi dạng khi chốt.
        let rect_mode = tool == Tool::Marker && self.highlight_mode == HighlightMode::Rect;
        let begin_tool = if rect_mode { Tool::Rect } else { tool };
        let Some(mut stroke) =
            begin_stroke(begin_tool, at, env.dpi, self.size, self.color, env.img)
        else {
            return Down::Handled;
        };
        self.highlight_drag = rect_mode;
        if let Shape::Marker { width, .. } = &mut stroke.shape {
            *width = dip_to_px(self.marker_width_dip, env.dpi);
        }
        // Hình chữ nhật, elip: độ dày viền theo kiểu mặc định (bảng thuộc tính) chứ không chỉ mức cỡ.
        if let Shape::Rect { width, .. } | Shape::Ellipse { width, .. } = &mut stroke.shape {
            *width = dip_to_px(self.box_style.width_dip, env.dpi);
        }
        if let Some(b) = stroke.shape.bounds() {
            self.mark(b);
        }
        self.side_hover = None;
        self.stroke = Some(stroke);
        Down::Capture
    }

    /// Kéo tới `p` khi đang giữ nét hoặc cử chỉ.
    pub fn moved(&mut self, env: &Env, p: Point, shift: bool) {
        if self.gesture.is_some() {
            let at = self.clamp_to_sel(env, p);
            self.drag_gesture(env, at, shift);
        } else if let Some(st) = self.stroke.as_mut() {
            let at = Pt {
                x: p.x as f32,
                y: p.y as f32,
            };
            if let Some(r) = extend_stroke(st, at, shift, env.img) {
                self.mark(r);
            }
        }
    }

    /// Nhả chuột ở `p`: chốt nét hoặc cử chỉ, hoặc kích hoạt mục thanh dọc/bảng đã nhấn.
    /// Trả `true` khi đã dùng cú nhả.
    pub fn up(&mut self, env: &Env, p: Point, shift: bool) -> bool {
        if self.gesture.is_some() {
            let at = self.clamp_to_sel(env, p);
            self.drag_gesture(env, at, shift);
            self.commit_gesture(env.dpi);
            return true;
        }
        if self.stroke.is_some() {
            self.moved(env, p, shift);
            self.commit_stroke(env);
            return true;
        }
        if let Some(cell) = self.style_pressed.take() {
            if self.style_cell_under(env, p) == Some(cell) {
                self.press_style_cell(cell, env.dpi);
            }
            return true;
        }
        if let Some(level) = self.size_pressed.take() {
            let fly = self.flyout_rect_of(env);
            if fly.and_then(|f| size_cell_at(f, env.dpi, p)) == Some(level) {
                self.set_size(level, env.dpi);
                self.close_flyouts();
            }
            return true;
        }
        if let Some(t) = self.shape_pressed.take() {
            let fly = self.flyout_rect_of(env);
            if fly.and_then(|f| shape_cell_at(f, env.dpi, p)) == Some(t) {
                self.use_tool(t, env.dpi);
            }
            return true;
        }
        if let Some(i) = self.color_pressed.take() {
            let fly = self.flyout_rect_of(env);
            if fly.and_then(|f| palette_cell_at(f, env.dpi, p)) == Some(i) {
                if let Some(c) = PALETTE.get(i) {
                    self.choose_color(*c, env.dpi);
                }
            }
            return true;
        }
        if let Some(item) = self.side_pressed.take() {
            if self.side_item_under(env, p) == Some(item) {
                self.activate_side(item, env.dpi);
            }
            return true;
        }
        false
    }

    /// Chốt nét đang vẽ vào sổ ở CUỐI (hoặc bỏ nếu quá ngắn).
    fn commit_stroke(&mut self, env: &Env) {
        let Some(s) = self.stroke.take() else {
            return;
        };
        let drawn = s.shape.bounds();
        let highlight = std::mem::take(&mut self.highlight_drag);
        match end_stroke(s, env.img) {
            Some(shape) => {
                let shape = if highlight {
                    highlight_from_stroke(&shape, self.highlight_opacity)
                } else {
                    styled_from_stroke(&shape, self.box_style.fill, self.box_style.opacity)
                };
                self.add_mark(shape)
            }
            None => {
                if let Some(r) = drawn {
                    self.mark(r);
                }
            }
        }
    }

    /// Mất chuột giữa chừng (cửa sổ mất key, màn hình đổi): chốt nét và cử chỉ đang giữ.
    pub fn lost_capture(&mut self, env: &Env) {
        if self.stroke.is_some() {
            self.commit_stroke(env);
        }
        if self.gesture.is_some() {
            self.commit_gesture(env.dpi);
        }
        self.side_pressed = None;
        self.size_pressed = None;
        self.shape_pressed = None;
        self.color_pressed = None;
        self.style_pressed = None;
    }

    /// Nhấn hoặc nhả Shift khi đang kéo hình hoặc nét có ràng buộc: cập nhật ngay theo điểm chuột
    /// cuối.
    pub fn shift_changed(&mut self, env: &Env, shift: bool) {
        if let Some(d) = &self.gesture {
            let at = d.at;
            self.drag_gesture(env, at, shift);
            return;
        }
        let Some(st) = self.stroke.as_mut() else {
            return;
        };
        if !matches!(
            st.tool,
            Tool::Line | Tool::Arrow | Tool::Rect | Tool::Ellipse | Tool::Blur
        ) {
            return;
        }
        let at = st.cursor;
        if let Some(r) = extend_stroke(st, at, shift, env.img) {
            self.mark(r);
        }
    }

    /// Chuột trỏ (không nhấn) ở `p`: mục đang trỏ của thanh dọc và bảng.
    pub fn hover(&mut self, env: &Env, p: Point) {
        self.side_hover = self.side_item_under(env, p);
        let fly = self.flyout_rect_of(env);
        self.size_hover = match (self.flyout, fly) {
            (Flyout::Sizes, Some(f)) => size_cell_at(f, env.dpi, p),
            _ => None,
        };
        self.shape_hover = match (self.flyout, fly) {
            (Flyout::Shapes, Some(f)) => shape_cell_at(f, env.dpi, p),
            _ => None,
        };
        self.color_hover = match (self.flyout, fly) {
            (Flyout::Palette, Some(f)) => palette_cell_at(f, env.dpi, p),
            _ => None,
        };
        self.style_hover = self.style_cell_under(env, p);
    }

    /// Chuột rời khỏi vùng công cụ (sang màn hình khác): bỏ mọi mục đang trỏ.
    pub fn clear_hover(&mut self) {
        self.side_hover = None;
        self.size_hover = None;
        self.shape_hover = None;
        self.color_hover = None;
        self.style_hover = None;
    }

    /// Con trỏ chuột theo ngữ cảnh ở `p`.
    pub fn cursor_at(&self, env: &Env, p: Point, hit: Hit) -> CursorKind {
        if self.stroke.is_some() {
            return CursorKind::Cross;
        }
        if self.gesture.is_some() {
            return CursorKind::Move;
        }
        if self.flyout != Flyout::None || self.style_rect(env).is_some_and(|r| r.contains(p)) {
            return CursorKind::Arrow;
        }
        let on_bar =
            env.bar.is_some_and(|r| r.contains(p)) || env.side.is_some_and(|r| r.contains(p));
        if on_bar {
            return CursorKind::Arrow;
        }
        if self.press_grip(env, p).is_some() || self.shape_hit_at(env, p).is_some() {
            return CursorKind::Move;
        }
        match press_mode(self.tool, self.has_marks(), hit) {
            PressMode::Draw if self.tool == Some(Tool::Text) => CursorKind::IBeam,
            PressMode::Draw => CursorKind::Cross,
            PressMode::Ignore | PressMode::Pick => CursorKind::Arrow,
            PressMode::Selection => match hit {
                Hit::Inside => CursorKind::Move,
                _ => CursorKind::Cross,
            },
        }
    }

    /// Mục `SIDE_ITEMS` có được tô sáng (đang chọn) không.
    pub fn side_selected(&self, item: SideItem) -> bool {
        match item {
            SideItem::Tool(t) => Some(t) == self.tool,
            SideItem::Shapes => self.tool.is_some_and(is_shape),
            SideItem::Size | SideItem::Color => false,
        }
    }

    /// Vùng thanh dọc, bảng và gợi ý cần vẽ lại khi trạng thái bố cục đổi (inflate để chứa gợi ý).
    pub fn chrome_rects(&self, env: &Env) -> Vec<Rect> {
        let pad = scaled(darkshot_core::overlay_layout::BUTTON, env.dpi) * 2;
        let mut v = Vec::new();
        if let Some(b) = env.bar {
            v.push(inflate(b, pad));
        }
        if let Some(s) = env.side {
            v.push(inflate(s, pad));
            for f in [Flyout::Sizes, Flyout::Shapes, Flyout::Palette] {
                if let Some(r) = flyout_rect_for(f, s, env.sel, env.mon, env.dpi) {
                    v.push(inflate(r, 2));
                }
            }
            // Bảng thuộc tính: luôn gồm cả cỡ cao nhất (Chữ) để vùng bảng vừa đóng cũng được vẽ lại.
            if let Some(r) = style_rect_for(StyleKind::Label, s, env.sel, env.mon, env.dpi) {
                v.push(inflate(r, 2));
            }
        }
        v.into_iter()
            .filter_map(|r| intersect(r, env.mon))
            .collect()
    }
}

impl Default for Annot {
    fn default() -> Annot {
        Annot::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotate_style::{MARKER_WIDTH_MAX_DIP, WIDTH_MAX_DIP};
    use crate::overlay_input::{bar_rect, side_rect};
    use darkshot_core::annotate::STROKE_DIP;
    use darkshot_core::annotate::{constrain_45, constrain_square};
    use darkshot_core::geometry::Selection;
    use darkshot_core::overlay_layout::{size_cell_rect, BUTTON, GAP, SHAPE_TOOLS};

    const MON: Rect = Rect {
        x: 0,
        y: 0,
        w: 1000,
        h: 700,
    };
    const SEL: Rect = Rect {
        x: 50,
        y: 50,
        w: 400,
        h: 300,
    };

    fn pt(x: i32, y: i32) -> Point {
        Point { x, y }
    }

    fn solid(w: u32, h: u32) -> Image {
        let mut bgra = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                bgra.extend_from_slice(&[(x % 251) as u8, (y % 241) as u8, 90, 255]);
            }
        }
        Image {
            width: w,
            height: h,
            bgra,
        }
    }

    /// Bàn thử: sổ công cụ, vùng chọn cố định, ảnh gốc thật; tạo ngữ cảnh cho từng sự kiện.
    struct H {
        a: Annot,
        sel: Rect,
        mon: Rect,
        dpi: u32,
        img: Image,
    }

    impl H {
        fn new() -> H {
            H::with(SEL, MON, 96)
        }

        fn with(sel: Rect, mon: Rect, dpi: u32) -> H {
            H {
                a: Annot::new(),
                sel,
                mon,
                dpi,
                img: solid(mon.w as u32, mon.h as u32),
            }
        }

        fn layout(&self) -> Layout {
            let bar = bar_rect(self.sel, self.dpi, self.mon);
            Layout {
                sel: self.sel,
                mon: self.mon,
                dpi: self.dpi,
                bar: Some(bar),
                side: Some(side_rect(self.sel, bar, self.dpi, self.mon)),
            }
        }

        fn hit(&self, p: Point) -> Hit {
            Selection::with_rect(self.mon, Some(self.sel)).hit_test(p, 6)
        }

        fn down(&mut self, p: Point) -> Down {
            let (lay, hit) = (self.layout(), self.hit(p));
            self.a.down(&lay.env(&self.img), p, hit)
        }

        fn drag(&mut self, p: Point, shift: bool) {
            let lay = self.layout();
            self.a.moved(&lay.env(&self.img), p, shift);
        }

        fn up(&mut self, p: Point, shift: bool) -> bool {
            let lay = self.layout();
            self.a.up(&lay.env(&self.img), p, shift)
        }

        fn hover(&mut self, p: Point) {
            let lay = self.layout();
            self.a.hover(&lay.env(&self.img), p);
        }

        /// Kéo từ `from` tới `to` (một lần kéo) bằng công cụ hiện tại.
        fn gesture(&mut self, from: Point, to: Point) {
            self.down(from);
            self.drag(to, false);
            self.up(to, false);
        }

        fn draw(&mut self, tool: Tool, from: Point, to: Point) {
            self.a.use_tool(tool, self.dpi);
            self.gesture(from, to);
        }

        fn click_side(&mut self, item: SideItem) {
            let lay = self.layout();
            let side = lay.side.unwrap();
            let l = side_layout_for(lay.mon, lay.dpi);
            let i = darkshot_core::overlay_layout::side_index(item);
            let r = darkshot_core::overlay_layout::side_item_rect(side, &l, i).unwrap();
            let c = pt(r.x + r.w / 2, r.y + r.h / 2);
            assert_eq!(self.down(c), Down::Handled);
            assert!(self.up(c, false));
        }
    }

    fn red() -> Rgb {
        PALETTE[0]
    }

    /// Dấu của từng hình trong sổ: `a.x` của Rect (hình khác là -1).
    fn rect_xs(a: &Annot) -> Vec<i32> {
        a.shapes()
            .iter()
            .map(|s| match s {
                Shape::Box { a, .. } => a.x as i32,
                _ => -1,
            })
            .collect()
    }

    // ---------------------------------------------------------------- xếp lớp (z-order)

    #[test]
    fn z_order_new_shapes_go_on_top() {
        let mut h = H::new();
        h.draw(Tool::Rect, pt(100, 100), pt(180, 160));
        h.draw(Tool::Rect, pt(120, 110), pt(200, 170));
        h.draw(Tool::Rect, pt(140, 120), pt(220, 180));
        // A rồi B rồi C: shapes() = [A, B, C], cuối là trên cùng.
        assert_eq!(rect_xs(&h.a), vec![100, 120, 140]);
        // Chữ và số vừa đánh cũng vào cuối.
        h.a.use_tool(Tool::Step, 96);
        h.down(pt(300, 200));
        h.a.use_tool(Tool::Text, 96);
        h.down(pt(320, 220));
        h.a.text_insert("xin");
        h.a.close_text(true);
        let kinds: Vec<&str> =
            h.a.shapes()
                .iter()
                .map(|s| match s {
                    Shape::Box { oval: false, .. } => "rect",
                    Shape::Step { .. } => "step",
                    Shape::Label { .. } => "text",
                    _ => "?",
                })
                .collect();
        assert_eq!(kinds, ["rect", "rect", "rect", "step", "text"]);
    }

    #[test]
    fn z_order_add_after_undo_goes_to_the_end_and_redo_restores_position() {
        let mut h = H::new();
        h.draw(Tool::Rect, pt(100, 100), pt(180, 160));
        h.draw(Tool::Rect, pt(120, 110), pt(200, 170));
        h.a.undo(96);
        assert_eq!(rect_xs(&h.a), vec![100]);
        // Hoàn tác rồi vẽ hình mới: nằm trên cùng.
        h.draw(Tool::Rect, pt(140, 120), pt(220, 180));
        assert_eq!(rect_xs(&h.a), vec![100, 140]);
        assert!(!h.a.can_redo(), "vẽ mới xóa ngăn làm lại");
        // Hoàn tác hai lần rồi làm lại hai lần: đúng thứ tự cũ.
        h.a.undo(96);
        h.a.undo(96);
        assert!(h.a.shapes().is_empty());
        h.a.redo(96);
        h.a.redo(96);
        assert_eq!(rect_xs(&h.a), vec![100, 140]);
    }

    #[test]
    fn z_order_undo_of_a_delete_restores_the_old_position() {
        let mut h = H::new();
        h.draw(Tool::Rect, pt(100, 100), pt(180, 160));
        h.draw(Tool::Rect, pt(120, 110), pt(200, 170));
        h.draw(Tool::Rect, pt(140, 120), pt(220, 180));
        // Chọn hình giữa (bấm cạnh trái của nó) rồi xóa.
        h.a.use_tool(Tool::Select, 96);
        h.down(pt(120, 140));
        h.up(pt(120, 140), false);
        assert_eq!(h.a.selected(), Some(1));
        h.a.delete_selected(96);
        assert_eq!(rect_xs(&h.a), vec![100, 140]);
        h.a.undo(96);
        assert_eq!(rect_xs(&h.a), vec![100, 120, 140], "về đúng lớp giữa");
    }

    #[test]
    fn z_order_replace_keeps_the_index_when_moving_resizing_and_recoloring() {
        let mut h = H::new();
        h.draw(Tool::Rect, pt(100, 100), pt(180, 160));
        h.draw(Tool::Rect, pt(120, 110), pt(200, 170));
        h.draw(Tool::Rect, pt(140, 120), pt(220, 180));
        h.a.use_tool(Tool::Select, 96);
        // Chọn hình giữa (điểm bấm trên cạnh trái, xa mọi tay nắm).
        h.down(pt(120, 125));
        h.up(pt(120, 125), false);
        assert_eq!(h.a.selected(), Some(1));
        // Di chuyển: vẫn ở chỉ số 1, không nhảy lên trên cùng.
        h.down(pt(120, 125));
        h.drag(pt(150, 135), false);
        h.up(pt(150, 135), false);
        assert_eq!(h.a.shapes().len(), 3);
        assert_eq!(rect_xs(&h.a)[0], 100);
        assert_eq!(rect_xs(&h.a)[2], 140);
        assert_eq!(rect_xs(&h.a)[1], 150, "hình giữa đã dời nhưng vẫn ở giữa");
        match &h.a.shapes()[1] {
            Shape::Box { a, b, .. } => assert_eq!((a.y, b.x, b.y), (120.0, 230.0, 180.0)),
            other => panic!("{other:?}"),
        }
        assert_eq!(h.a.selected(), Some(1));
        // Đổi màu và đổi cỡ khi đang chọn: cũng giữ chỉ số.
        h.a.choose_color(red(), 96);
        h.a.set_size(SizeLevel::Huge, 96);
        assert_eq!(h.a.shapes().len(), 3);
        match &h.a.shapes()[1] {
            Shape::Box { style, .. } => {
                assert_eq!(style.stroke, red());
                assert_eq!(style.stroke_width, SizeLevel::Huge.stroke_dip());
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(rect_xs(&h.a)[0], 100);
        assert_eq!(rect_xs(&h.a)[2], 140);
    }

    #[test]
    fn z_order_overlapping_click_selects_the_front_shape() {
        let mut h = H::new();
        // Hai khung có cạnh trên chung đoạn (150..250, y = 100): khung vẽ sau nằm phía trước.
        h.draw(Tool::Rect, pt(100, 100), pt(250, 200));
        h.draw(Tool::Rect, pt(150, 100), pt(300, 200));
        h.a.use_tool(Tool::Select, 96);
        let lay = h.layout();
        assert_eq!(h.a.topmost_at(&lay.env(&h.img), pt(200, 100)), Some(1));
        h.down(pt(200, 100));
        h.up(pt(200, 100), false);
        assert_eq!(h.a.selected(), Some(1), "chọn hình phía trước");
        // Tay nắm hiện theo hình được chọn.
        let (shape, _) = h.a.shown_selection().unwrap();
        assert!(matches!(shape, Shape::Box { a, .. } if a.x as i32 == 150));
        // Hình thêm sau (lên trên) thì lại được chọn trước dù chỗ bấm cũ.
        h.a.deselect(96);
        h.draw(Tool::Rect, pt(160, 100), pt(240, 200));
        h.a.use_tool(Tool::Select, 96);
        let lay = h.layout();
        assert_eq!(h.a.topmost_at(&lay.env(&h.img), pt(200, 100)), Some(2));
    }

    #[test]
    fn z_order_in_progress_stroke_and_typing_are_drawn_last() {
        let mut h = H::new();
        h.draw(Tool::Rect, pt(100, 100), pt(180, 160));
        h.draw(Tool::Rect, pt(120, 110), pt(200, 170));
        // Đang kéo nét mới: đứng sau mọi hình đã có.
        h.a.use_tool(Tool::Line, 96);
        h.down(pt(110, 110));
        h.drag(pt(210, 150), false);
        let layers = h.a.layers();
        assert_eq!(layers.len(), 2);
        assert!(matches!(layers[0], Layer::Run(r) if r.len() == 2));
        assert!(matches!(layers[1], Layer::Stroke(Shape::Line { .. })));
        drop(layers);
        h.up(pt(210, 150), false);
        // Thả chuột: thành hình cuối cùng trong sổ, thứ tự khớp với thứ tự đã thấy.
        assert_eq!(h.a.shapes().len(), 3);
        assert!(matches!(h.a.shapes()[2], Shape::Line { .. }));
        // Đang gõ chữ: chữ là lớp trên cùng.
        h.a.use_tool(Tool::Text, 96);
        h.down(pt(300, 250));
        h.a.text_insert("abc");
        let layers = h.a.layers();
        assert!(matches!(
            layers.last(),
            Some(Layer::Typing(Shape::Label { .. }))
        ));
        assert_eq!(layers.len(), 2);
        drop(layers);
        // Kết thúc nhập: vào CUỐI sổ.
        h.a.close_text(true);
        assert_eq!(h.a.shapes().len(), 4);
        assert!(matches!(h.a.shapes()[3], Shape::Label { .. }));
    }

    #[test]
    fn z_order_dragged_shape_preview_replaces_in_place() {
        let mut h = H::new();
        h.draw(Tool::Rect, pt(100, 100), pt(180, 160));
        h.draw(Tool::Rect, pt(120, 110), pt(200, 170));
        h.draw(Tool::Rect, pt(140, 120), pt(220, 180));
        h.a.use_tool(Tool::Select, 96);
        h.down(pt(120, 125));
        h.drag(pt(160, 135), false);
        // Bản xem trước thay hình thứ 2 đúng chỗ (một hình trước, một hình sau), không nhảy lên.
        let layers = h.a.layers();
        assert!(matches!(layers[0], Layer::Run(r) if r.len() == 1));
        assert!(matches!(layers[1], Layer::Preview(_)));
        assert!(matches!(layers[2], Layer::Run(r) if r.len() == 1));
        drop(layers);
        h.up(pt(160, 135), false);
        assert_eq!(h.a.layers().len(), 1);
    }

    // ---------------------------------------------------------------- công cụ và cử chỉ

    #[test]
    fn tool_keys_toggle_tools_and_sizes() {
        let mut a = Annot::new();
        let cases = [
            ("v", Tool::Select),
            ("p", Tool::Pen),
            ("l", Tool::Line),
            ("a", Tool::Arrow),
            ("r", Tool::Rect),
            ("e", Tool::Ellipse),
            ("h", Tool::Marker),
            ("b", Tool::Blur),
            ("t", Tool::Text),
            ("n", Tool::Step),
        ];
        for (k, t) in cases {
            assert!(a.tool_key(Some(k), 96), "{k}");
            assert_eq!(a.tool(), Some(t), "{k}");
            // Bấm lại tắt công cụ.
            assert!(a.tool_key(Some(k), 96));
            assert_eq!(a.tool(), None);
        }
        // Chữ hoa (Shift) cũng nhận.
        assert!(a.tool_key(Some("P"), 96));
        assert_eq!(a.tool(), Some(Tool::Pen));
        // Hình nhớ cho nút Hình.
        a.tool_key(Some("e"), 96);
        assert_eq!(a.shape_tool(), Tool::Ellipse);
        // Cỡ.
        assert_eq!(a.size(), SizeLevel::Medium);
        assert!(a.tool_key(Some("]"), 96));
        assert_eq!(a.size(), SizeLevel::Large);
        assert!(a.tool_key(Some("["), 96));
        assert!(a.tool_key(Some("["), 96));
        assert_eq!(a.size(), SizeLevel::Small);
        // Phím khác không dùng.
        assert!(!a.tool_key(Some("z"), 96));
        assert!(!a.tool_key(Some("1"), 96));
        assert!(!a.tool_key(None, 96));
        assert!(!a.tool_key(Some(""), 96));
        assert_eq!(vk_of(Some("[")), Some(VK_OEM_4));
        assert_eq!(vk_of(Some("]")), Some(VK_OEM_6));
    }

    #[test]
    fn each_tool_creates_the_right_shape() {
        let (from, to) = (pt(100, 100), pt(220, 180));
        for (tool, ok) in [
            (Tool::Line, 0),
            (Tool::Arrow, 1),
            (Tool::Rect, 2),
            (Tool::Ellipse, 3),
            (Tool::Blur, 4),
        ] {
            let mut h = H::new();
            h.draw(tool, from, to);
            assert_eq!(h.a.shapes().len(), 1, "{tool:?}");
            let good = matches!(
                (&h.a.shapes()[0], ok),
                (Shape::Line { .. }, 0)
                    | (Shape::Arrow { .. }, 1)
                    | (Shape::Box { oval: false, .. }, 2)
                    | (Shape::Box { oval: true, .. }, 3)
                    | (Shape::Blur { .. }, 4)
            );
            assert!(good, "{tool:?}: {:?}", h.a.shapes()[0]);
            assert!(!h.a.capturing());
        }
        // Bút và bút dạ quang: nhiều điểm theo đường kéo.
        for tool in [Tool::Pen, Tool::Marker] {
            let mut h = H::new();
            h.a.use_tool(tool, 96);
            h.down(pt(100, 100));
            h.drag(pt(130, 120), false);
            h.drag(pt(160, 110), false);
            h.up(pt(190, 140), false);
            let ok = match &h.a.shapes()[0] {
                Shape::Pen { points, .. } if tool == Tool::Pen => points.len() >= 4,
                Shape::Marker { points, .. } if tool == Tool::Marker => points.len() >= 4,
                _ => false,
            };
            assert!(ok, "{tool:?}: {:?}", h.a.shapes());
        }
        // Đánh số: bấm là có vòng, số tăng dần.
        let mut h = H::new();
        h.a.use_tool(Tool::Step, 96);
        h.down(pt(100, 100));
        h.down(pt(200, 100));
        let ns: Vec<u32> =
            h.a.shapes()
                .iter()
                .filter_map(|s| match s {
                    Shape::Step { n, .. } => Some(*n),
                    _ => None,
                })
                .collect();
        assert_eq!(ns, vec![1, 2]);
    }

    #[test]
    fn shift_constrains_lines_and_boxes() {
        let a = Pt { x: 100.0, y: 100.0 };
        let b = Pt { x: 200.0, y: 130.0 };
        let mut h = H::new();
        h.a.use_tool(Tool::Line, 96);
        h.down(pt(100, 100));
        h.drag(pt(200, 130), true);
        h.up(pt(200, 130), true);
        match &h.a.shapes()[0] {
            Shape::Line { b: end, .. } => assert_eq!(*end, constrain_45(a, b)),
            other => panic!("{other:?}"),
        }
        let mut h = H::new();
        h.a.use_tool(Tool::Rect, 96);
        h.down(pt(100, 100));
        h.drag(pt(200, 130), true);
        h.up(pt(200, 130), true);
        match &h.a.shapes()[0] {
            Shape::Box { b: end, .. } => assert_eq!(*end, constrain_square(a, b)),
            other => panic!("{other:?}"),
        }
        // Nhấn/nhả Shift giữa chừng cập nhật ngay không cần di chuột.
        let mut h = H::new();
        h.a.use_tool(Tool::Rect, 96);
        h.down(pt(100, 100));
        h.drag(pt(200, 130), false);
        let lay = h.layout();
        h.a.shift_changed(&lay.env(&h.img), true);
        match h.a.stroke().map(|s| &s.shape) {
            Some(Shape::Rect { b: end, .. }) => assert_eq!(*end, constrain_square(a, b)),
            other => panic!("{other:?}"),
        }
        h.a.shift_changed(&lay.env(&h.img), false);
        match h.a.stroke().map(|s| &s.shape) {
            Some(Shape::Rect { b: end, .. }) => assert_eq!(*end, b),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn drawing_outside_the_selection_creates_nothing() {
        let mut h = H::new();
        h.a.use_tool(Tool::Rect, 96);
        // Xa vùng chọn và xa tay nắm: bỏ qua.
        assert_eq!(h.down(pt(700, 500)), Down::Handled);
        assert!(!h.a.capturing());
        h.drag(pt(800, 600), false);
        h.up(pt(800, 600), false);
        assert!(h.a.shapes().is_empty());
        // Trên tay nắm vùng chọn: để vùng chọn xử lý (không tạo hình).
        assert_eq!(h.down(pt(50, 50)), Down::Selection);
        assert!(!h.a.capturing());
        assert!(h.a.shapes().is_empty());
        // Không công cụ: bấm trong vùng chọn là di chuyển vùng (Selection), không vẽ.
        let mut h = H::new();
        assert_eq!(h.down(pt(200, 200)), Down::Selection);
        // Có hình rồi mà không công cụ: bấm ngoài vùng không tạo vùng mới.
        h.draw(Tool::Rect, pt(100, 100), pt(180, 160));
        h.a.toggle_tool(Tool::Rect, 96);
        assert_eq!(h.a.tool(), None);
        assert_eq!(h.down(pt(700, 500)), Down::Handled);
    }

    #[test]
    fn very_short_strokes_are_dropped() {
        let mut h = H::new();
        h.draw(Tool::Line, pt(100, 100), pt(100, 101));
        h.draw(Tool::Rect, pt(100, 100), pt(101, 140));
        h.draw(Tool::Ellipse, pt(100, 100), pt(160, 100));
        assert!(h.a.shapes().is_empty());
        assert!(!h.a.can_undo());
    }

    #[test]
    fn select_move_and_resize_use_gestures() {
        let mut h = H::new();
        h.draw(Tool::Rect, pt(100, 100), pt(200, 160));
        h.a.use_tool(Tool::Select, 96);
        // Bấm cạnh trái (xa tay nắm): chọn (Capture, chưa ghi sổ nếu không kéo).
        assert_eq!(h.down(pt(100, 115)), Down::Capture);
        assert!(h.up(pt(100, 115), false));
        assert_eq!(h.a.selected(), Some(0));
        assert_eq!(h.a.shapes().len(), 1);
        // Kéo di chuyển (+30, +20).
        h.down(pt(100, 115));
        h.drag(pt(130, 135), false);
        h.up(pt(130, 135), false);
        match &h.a.shapes()[0] {
            Shape::Box { a, b, .. } => {
                assert_eq!((a.x, a.y, b.x, b.y), (130.0, 120.0, 230.0, 180.0));
            }
            other => panic!("{other:?}"),
        }
        // Kéo tay nắm góc dưới phải (230, 180) tới (260, 210).
        assert_eq!(h.down(pt(230, 180)), Down::Capture);
        h.drag(pt(260, 210), false);
        h.up(pt(260, 210), false);
        match &h.a.shapes()[0] {
            Shape::Box { a, b, .. } => {
                assert_eq!((a.x, a.y, b.x, b.y), (130.0, 120.0, 260.0, 210.0));
            }
            other => panic!("{other:?}"),
        }
        // Mỗi cử chỉ là một mục sổ: hoàn tác hai lần về hình gốc.
        h.a.undo(96);
        h.a.undo(96);
        match &h.a.shapes()[0] {
            Shape::Box { a, b, .. } => assert_eq!((a.x, b.x), (100.0, 200.0)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_gesture_can_be_cancelled_with_escape() {
        let mut h = H::new();
        h.draw(Tool::Rect, pt(100, 100), pt(200, 160));
        h.a.use_tool(Tool::Select, 96);
        h.down(pt(100, 130));
        h.drag(pt(130, 150), false);
        assert!(h.a.capturing());
        // Esc giữa cử chỉ: hủy cử chỉ, vẫn chọn hình, không ghi sổ.
        assert!(h.a.escape(96));
        assert!(!h.a.capturing());
        assert_eq!(h.a.selected(), Some(0));
        match &h.a.shapes()[0] {
            Shape::Box { a, .. } => assert_eq!(a.x, 100.0),
            other => panic!("{other:?}"),
        }
        // Esc tiếp: bỏ chọn; Esc nữa: hết việc để đóng (hủy lớp phủ).
        assert!(h.a.escape(96));
        assert_eq!(h.a.selected(), None);
        assert!(!h.a.escape(96));
    }

    #[test]
    fn changing_tools_deselects_and_closes_things() {
        let mut h = H::new();
        h.draw(Tool::Rect, pt(100, 100), pt(200, 160));
        h.a.use_tool(Tool::Select, 96);
        h.down(pt(100, 130));
        h.up(pt(100, 130), false);
        assert_eq!(h.a.selected(), Some(0));
        h.a.toggle_tool(Tool::Pen, 96);
        assert_eq!(h.a.selected(), None);
        // Đang gõ chữ mà đổi công cụ: chốt chữ vào cuối sổ.
        h.a.use_tool(Tool::Text, 96);
        h.down(pt(300, 250));
        h.a.text_insert("hi");
        h.a.toggle_tool(Tool::Line, 96);
        assert!(h.a.text().is_none());
        assert!(matches!(h.a.shapes().last(), Some(Shape::Label { .. })));
        // Mở bảng rồi đổi công cụ: bảng đóng.
        h.a.activate_side(SideItem::Size, 96);
        assert_eq!(h.a.flyout(), Flyout::Sizes);
        h.a.toggle_tool(Tool::Arrow, 96);
        assert_eq!(h.a.flyout(), Flyout::None);
    }

    #[test]
    fn undo_redo_many_steps_and_tool_state() {
        let mut h = H::new();
        for i in 0..5 {
            h.draw(Tool::Rect, pt(100 + i * 10, 100), pt(180 + i * 10, 160));
        }
        assert_eq!(h.a.shapes().len(), 5);
        assert!(h.a.can_undo() && !h.a.can_redo());
        for _ in 0..5 {
            h.a.undo(96);
        }
        assert!(h.a.shapes().is_empty());
        assert!(!h.a.can_undo() && h.a.can_redo());
        for _ in 0..3 {
            h.a.redo(96);
        }
        assert_eq!(rect_xs(&h.a), vec![100, 110, 120]);
        // Hoàn tác/làm lại khi đang kéo không làm gì.
        h.a.use_tool(Tool::Rect, 96);
        h.down(pt(300, 100));
        h.a.undo(96);
        h.a.redo(96);
        assert_eq!(h.a.shapes().len(), 3);
        h.up(pt(340, 140), false);
        assert_eq!(h.a.shapes().len(), 4);
    }

    #[test]
    fn delete_removes_only_the_selected_shape() {
        let mut h = H::new();
        h.draw(Tool::Rect, pt(100, 100), pt(180, 160));
        h.draw(Tool::Rect, pt(220, 100), pt(300, 160));
        h.a.delete_selected(96);
        assert_eq!(h.a.shapes().len(), 2, "không chọn gì thì không xóa");
        h.a.use_tool(Tool::Select, 96);
        h.down(pt(220, 130));
        h.up(pt(220, 130), false);
        h.a.delete_selected(96);
        assert_eq!(rect_xs(&h.a), vec![100]);
        assert_eq!(h.a.selected(), None);
    }

    // ---------------------------------------------------------------- chữ

    #[test]
    fn text_is_multiline_and_commits_at_the_end() {
        let mut h = H::new();
        h.a.use_tool(Tool::Text, 96);
        assert_eq!(h.down(pt(100, 100)), Down::Handled);
        assert!(h.a.text().is_some());
        h.a.text_insert("xin ch\u{e0}o");
        h.a.text_command(TextCmd::Newline);
        h.a.text_insert("vi\u{1ec7}t");
        assert_eq!(
            h.a.text().unwrap().edit.text(),
            "xin ch\u{e0}o\nvi\u{1ec7}t"
        );
        // Mũi tên, Home, End, xóa.
        h.a.text_command(TextCmd::Home);
        h.a.text_command(TextCmd::Backspace);
        assert_eq!(h.a.text().unwrap().edit.text(), "xin ch\u{e0}ovi\u{1ec7}t");
        h.a.text_command(TextCmd::Left);
        h.a.text_command(TextCmd::Right);
        h.a.text_command(TextCmd::End);
        h.a.text_command(TextCmd::Up);
        h.a.text_command(TextCmd::Down);
        h.a.text_command(TextCmd::Delete);
        // Dán qua `sanitize_paste` (CRLF thành LF, bỏ ký tự điều khiển).
        h.a.text_paste("a\r\nb\u{7}");
        assert!(h.a.text().unwrap().edit.text().ends_with("a\nb"));
        // Bấm chỗ khác trong vùng chọn (công cụ Chữ) chốt ô cũ rồi mở ô mới.
        h.down(pt(300, 250));
        assert_eq!(h.a.shapes().len(), 1);
        assert!(h.a.text().is_some());
        h.a.close_text(false);
        assert_eq!(h.a.shapes().len(), 1, "hủy thì không thêm");
    }

    #[test]
    fn empty_or_blank_text_is_dropped() {
        let mut h = H::new();
        h.a.use_tool(Tool::Text, 96);
        h.down(pt(100, 100));
        h.a.close_text(true);
        h.down(pt(100, 100));
        h.a.text_insert("  ");
        h.a.text_command(TextCmd::Newline);
        h.a.close_text(true);
        assert!(h.a.shapes().is_empty());
    }

    #[test]
    fn text_input_method_marked_text_is_replaced_not_duplicated() {
        let mut h = H::new();
        h.a.use_tool(Tool::Text, 96);
        h.down(pt(100, 100));
        // Telex: "a" rồi "aa" -> "â" rồi chốt.
        h.a.text_marked("a");
        assert!(h.a.text_composing());
        h.a.text_marked("\u{e2}");
        assert_eq!(h.a.text().unwrap().edit.text(), "\u{e2}");
        h.a.text_insert("\u{e2}");
        assert!(!h.a.text_composing());
        assert_eq!(h.a.text().unwrap().edit.text(), "\u{e2}");
        // Chuỗi dài hơn một ký tự và chốt bằng unmark.
        h.a.text_marked("vi\u{1ec7}");
        assert_eq!(h.a.text().unwrap().edit.text(), "\u{e2}vi\u{1ec7}");
        h.a.text_unmark();
        assert!(!h.a.text_composing());
        h.a.text_insert("t");
        assert_eq!(h.a.text().unwrap().edit.text(), "\u{e2}vi\u{1ec7}t");
        // Soạn dở rồi hủy ô: bỏ cả chuỗi.
        h.a.text_marked("zz");
        h.a.force_close_text();
        assert!(h.a.text().is_none());
        assert!(h.a.shapes().is_empty());
    }

    #[test]
    fn undo_while_typing_clears_the_box_first() {
        let mut h = H::new();
        h.draw(Tool::Rect, pt(100, 100), pt(180, 160));
        h.a.use_tool(Tool::Text, 96);
        h.down(pt(300, 250));
        // Ô rỗng: hoàn tác hủy ô rồi hoàn tác hình cuối.
        h.a.undo(96);
        assert!(h.a.text().is_none());
        assert!(h.a.shapes().is_empty());
        // Ô có chữ: chỉ xóa ô.
        h.a.redo(96);
        h.down(pt(300, 250));
        h.a.text_insert("x");
        h.a.undo(96);
        assert!(h.a.text().is_none());
        assert_eq!(h.a.shapes().len(), 1);
        // Làm lại khi ô mở: không làm gì.
        h.down(pt(300, 250));
        h.a.redo(96);
        assert!(h.a.text().is_some());
    }

    #[test]
    fn text_size_and_color_follow_choices_while_typing() {
        let mut h = H::new();
        h.a.use_tool(Tool::Text, 96);
        h.down(pt(100, 100));
        h.a.text_insert("ab");
        let before = h.a.text().unwrap().size;
        h.a.set_size(SizeLevel::Huge, 96);
        assert!(h.a.text().unwrap().size > before);
        assert_eq!(h.a.text().unwrap().edit.text(), "ab");
        h.a.choose_color(red(), 96);
        h.a.close_text(true);
        match &h.a.shapes()[0] {
            Shape::Label { color, size, .. } => {
                assert_eq!(*color, red());
                assert_eq!(*size, SizeLevel::Huge.text_dip());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn blink_toggles_only_while_typing_and_restarts_on_edit() {
        let mut h = H::new();
        h.a.blink();
        assert!(h.a.take_dirty().is_empty());
        h.a.use_tool(Tool::Text, 96);
        h.down(pt(100, 100));
        let e = h.a.text_epoch();
        h.a.take_dirty();
        assert!(h.a.caret_on());
        h.a.blink();
        assert!(!h.a.caret_on());
        assert!(!h.a.take_dirty().is_empty());
        h.a.text_insert("a");
        assert!(h.a.caret_on(), "gõ đưa con trỏ hiện lại");
        h.a.close_text(true);
        h.down(pt(100, 100));
        assert_ne!(h.a.text_epoch(), e, "mỗi ô mới có đời nháy riêng");
    }

    // ---------------------------------------------------------------- Esc, bảng, thanh dọc

    #[test]
    fn escape_unwinds_one_layer_at_a_time_and_never_blocks_exit() {
        let mut h = H::new();
        h.draw(Tool::Rect, pt(100, 100), pt(180, 160));
        h.a.use_tool(Tool::Select, 96);
        h.down(pt(100, 130));
        h.up(pt(100, 130), false);
        h.a.activate_side(SideItem::Color, 96);
        assert_eq!(h.a.flyout(), Flyout::Palette);
        // Bảng -> chọn hình -> hết; không bao giờ trả `true` mãi.
        assert!(h.a.escape(96));
        assert_eq!(h.a.flyout(), Flyout::None);
        assert!(h.a.escape(96));
        assert_eq!(h.a.selected(), None);
        assert!(!h.a.escape(96));
        // Ô chữ: một Esc hủy ô (không chốt).
        h.a.use_tool(Tool::Text, 96);
        h.down(pt(300, 250));
        h.a.text_insert("zzz");
        assert!(h.a.escape(96));
        assert!(h.a.text().is_none());
        assert_eq!(h.a.shapes().len(), 1);
        assert!(!h.a.escape(96));
    }

    #[test]
    fn side_bar_activation_toggles_tools_flyouts_and_remembers_shape() {
        let mut h = H::new();
        h.click_side(SideItem::Tool(Tool::Pen));
        assert_eq!(h.a.tool(), Some(Tool::Pen));
        h.click_side(SideItem::Tool(Tool::Pen));
        assert_eq!(h.a.tool(), None);
        // Nút Hình: chưa dùng hình -> bật hình nhớ (mặc định Mũi tên), không mở bảng.
        h.click_side(SideItem::Shapes);
        assert_eq!(h.a.tool(), Some(Tool::Arrow));
        assert_eq!(h.a.flyout(), Flyout::None);
        // Đang dùng hình -> mở bảng; chọn ô Elip.
        h.click_side(SideItem::Shapes);
        assert_eq!(h.a.flyout(), Flyout::Shapes);
        let lay = h.layout();
        let side = lay.side.unwrap();
        let fly = flyout_rect_for(Flyout::Shapes, side, lay.sel, lay.mon, 96).unwrap();
        let idx = SHAPE_TOOLS
            .iter()
            .position(|t| *t == Tool::Ellipse)
            .unwrap();
        let cell = darkshot_core::overlay_layout::shape_cell_rect(fly, 96, idx);
        let c = pt(cell.x + cell.w / 2, cell.y + cell.h / 2);
        assert_eq!(h.down(c), Down::Handled);
        assert!(h.up(c, false));
        assert_eq!(h.a.tool(), Some(Tool::Ellipse));
        assert_eq!(h.a.shape_tool(), Tool::Ellipse);
        assert_eq!(h.a.flyout(), Flyout::None);
        // Bảng Cỡ: chọn mức Lớn.
        h.click_side(SideItem::Size);
        assert_eq!(h.a.flyout(), Flyout::Sizes);
        let fly = flyout_rect_for(Flyout::Sizes, side, lay.sel, lay.mon, 96).unwrap();
        let cell = size_cell_rect(fly, 96, 2);
        let c = pt(cell.x + cell.w / 2, cell.y + cell.h / 2);
        h.down(c);
        h.up(c, false);
        assert_eq!(h.a.size(), SizeLevel::Large);
        assert_eq!(h.a.flyout(), Flyout::None);
        // Bảng màu: chọn ô 3.
        h.click_side(SideItem::Color);
        assert_eq!(h.a.flyout(), Flyout::Palette);
        let fly = flyout_rect_for(Flyout::Palette, side, lay.sel, lay.mon, 96).unwrap();
        let cell = palette_cell_rect(fly, 96, 3);
        let c = pt(cell.x + cell.w / 2, cell.y + cell.h / 2);
        h.down(c);
        h.up(c, false);
        assert_eq!(h.a.color(), PALETTE[3]);
        assert_eq!(h.a.flyout(), Flyout::None);
        // Lựa chọn nhớ qua lần mở sau.
        let p = h.a.prefs();
        assert_eq!(p.shape_tool, Tool::Ellipse);
        assert_eq!(p.size, SizeLevel::Large);
        assert_eq!(p.color, PALETTE[3]);
        assert_eq!(Annot::with_prefs(p).prefs(), p);
    }

    #[test]
    fn a_click_outside_an_open_flyout_only_closes_it() {
        let mut h = H::new();
        h.a.use_tool(Tool::Rect, 96);
        h.a.activate_side(SideItem::Size, 96);
        // Bấm trong vùng chọn (nơi sẽ vẽ): chỉ đóng bảng, không bắt đầu nét.
        assert_eq!(h.down(pt(200, 200)), Down::Handled);
        assert_eq!(h.a.flyout(), Flyout::None);
        assert!(!h.a.capturing());
        // Bảng màu: bấm lề của bảng không đóng; bấm ngoài thì đóng.
        h.a.activate_side(SideItem::Color, 96);
        let lay = h.layout();
        let fly =
            flyout_rect_for(Flyout::Palette, lay.side.unwrap(), lay.sel, lay.mon, 96).unwrap();
        assert_eq!(h.down(pt(fly.x + 1, fly.y + 1)), Down::Handled);
        assert_eq!(h.a.flyout(), Flyout::Palette);
        h.down(pt(200, 200));
        assert_eq!(h.a.flyout(), Flyout::None);
    }

    #[test]
    fn color_change_applies_to_future_shapes_and_blur_ignores_it() {
        let mut h = H::new();
        h.a.choose_color(red(), 96);
        h.draw(Tool::Line, pt(100, 100), pt(200, 160));
        match &h.a.shapes()[0] {
            Shape::Line { color, .. } => assert_eq!(*color, red()),
            other => panic!("{other:?}"),
        }
        // Làm mờ: màu che lấy từ ảnh gốc, không phải màu đang chọn.
        h.draw(Tool::Blur, pt(100, 200), pt(200, 260));
        match &h.a.shapes()[1] {
            Shape::Blur { color, .. } => assert_ne!(*color, red()),
            other => panic!("{other:?}"),
        }
        // Chọn Làm mờ rồi đổi màu: hình không đổi, không ghi sổ.
        h.a.use_tool(Tool::Select, 96);
        h.down(pt(150, 230));
        h.up(pt(150, 230), false);
        assert_eq!(h.a.selected(), Some(1));
        let before = h.a.shapes().to_vec();
        h.a.choose_color(PALETTE[5], 96);
        assert_eq!(h.a.shapes(), &before[..]);
    }

    #[test]
    fn hover_tracks_side_items_and_flyout_cells() {
        let mut h = H::new();
        let lay = h.layout();
        let side = lay.side.unwrap();
        let l = side_layout_for(lay.mon, lay.dpi);
        for (i, item) in darkshot_core::overlay_layout::SIDE_ITEMS.iter().enumerate() {
            let r = darkshot_core::overlay_layout::side_item_rect(side, &l, i).unwrap();
            h.hover(pt(r.x + 2, r.y + 2));
            assert_eq!(h.a.side_hover(), Some(*item));
        }
        h.hover(pt(0, 0));
        assert_eq!(h.a.side_hover(), None);
        h.a.activate_side(SideItem::Size, 96);
        let fly = flyout_rect_for(Flyout::Sizes, side, lay.sel, lay.mon, 96).unwrap();
        let cell = size_cell_rect(fly, 96, 1);
        h.hover(pt(cell.x + 1, cell.y + 1));
        assert_eq!(h.a.size_hover(), Some(SizeLevel::Medium));
        let before = h.a.chrome_sig();
        h.hover(pt(0, 0));
        assert_ne!(h.a.chrome_sig(), before, "rê chuột đổi chữ ký bố cục");
        assert_eq!(h.a.size_hover(), None);
    }

    #[test]
    fn contextual_cursor_follows_the_mode() {
        let mut h = H::new();
        let cur = |h: &H, p: Point| {
            let lay = h.layout();
            h.a.cursor_at(&lay.env(&h.img), p, h.hit(p))
        };
        // Không công cụ: chữ thập ngoài vùng (chọn mới), trong vùng: di chuyển.
        assert_eq!(cur(&h, pt(700, 500)), CursorKind::Cross);
        assert_eq!(cur(&h, pt(200, 200)), CursorKind::Move);
        h.a.use_tool(Tool::Rect, 96);
        assert_eq!(cur(&h, pt(200, 200)), CursorKind::Cross);
        assert_eq!(cur(&h, pt(700, 500)), CursorKind::Arrow);
        h.a.use_tool(Tool::Text, 96);
        assert_eq!(cur(&h, pt(200, 200)), CursorKind::IBeam);
        h.a.use_tool(Tool::Select, 96);
        assert_eq!(cur(&h, pt(200, 200)), CursorKind::Arrow);
        let lay = h.layout();
        let side = lay.side.unwrap();
        assert_eq!(cur(&h, pt(side.x + 2, side.y + 2)), CursorKind::Arrow);
    }

    // ---------------------------------------------------------------- bố cục

    fn corners(mon: Rect) -> Vec<Rect> {
        let (w, h) = (mon.w, mon.h);
        let r = |x, y, w, h| Rect { x, y, w, h };
        vec![
            r(0, 0, 120, 90),
            r(w - 120, 0, 120, 90),
            r(0, h - 90, 120, 90),
            r(w - 120, h - 90, 120, 90),
            r(0, 0, w, h),
            r(w / 2, h / 2, 3, 3),
            r(w / 3, h / 3, w / 3, h / 3),
            r(0, h / 2, w, 5),
        ]
    }

    fn inside(r: Rect, mon: Rect) -> bool {
        r.x >= mon.x && r.y >= mon.y && r.right() <= mon.right() && r.bottom() <= mon.bottom()
    }

    fn overlaps(a: Rect, b: Rect) -> bool {
        a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
    }

    fn mon_for(dpi: u32) -> Rect {
        let k = dpi as i32;
        Rect {
            x: 0,
            y: 0,
            w: 1500 * k / 96,
            h: 900 * k / 96,
        }
    }

    #[test]
    fn bars_and_flyouts_stay_on_screen_and_the_side_bar_clears_the_bar() {
        for dpi in [96u32, 144, 192, 288] {
            let mon = mon_for(dpi);
            for sel in corners(mon) {
                let h = H::with(sel, mon, dpi);
                let lay = h.layout();
                let (bar, side) = (lay.bar.unwrap(), lay.side.unwrap());
                assert!(inside(bar, mon), "{dpi} {sel:?} bar {bar:?}");
                assert!(inside(side, mon), "{dpi} {sel:?} side {side:?}");
                assert!(!overlaps(bar, side), "{dpi} {sel:?} {bar:?} {side:?}");
                for f in [Flyout::Sizes, Flyout::Shapes, Flyout::Palette] {
                    let fly = flyout_rect_for(f, side, sel, mon, dpi).unwrap();
                    assert!(inside(fly, mon), "{dpi} {sel:?} {f:?} {fly:?}");
                }
            }
            // Vùng chọn giữa màn hình: hai thanh không che vùng chọn.
            let sel = Rect {
                x: mon.w / 3,
                y: mon.h / 3,
                w: mon.w / 3,
                h: mon.h / 3,
            };
            let h = H::with(sel, mon, dpi);
            let lay = h.layout();
            assert!(!overlaps(lay.bar.unwrap(), sel), "{dpi}");
            assert!(!overlaps(lay.side.unwrap(), sel), "{dpi}");
        }
    }

    #[test]
    fn side_items_hit_test_at_every_dpi() {
        for dpi in [96u32, 144, 192, 288] {
            let mon = mon_for(dpi);
            let sel = Rect {
                x: mon.w / 4,
                y: mon.h / 4,
                w: mon.w / 3,
                h: mon.h / 3,
            };
            let h = H::with(sel, mon, dpi);
            let lay = h.layout();
            let side = lay.side.unwrap();
            let l = side_layout_for(mon, dpi);
            for (i, item) in darkshot_core::overlay_layout::SIDE_ITEMS.iter().enumerate() {
                let r = darkshot_core::overlay_layout::side_item_rect(side, &l, i).unwrap();
                assert_eq!(r.w, scaled(BUTTON, dpi));
                for p in [
                    pt(r.x, r.y),
                    pt(r.x + r.w / 2, r.y + r.h / 2),
                    pt(r.right() - 1, r.bottom() - 1),
                ] {
                    assert_eq!(
                        h.a.side_item_under(&lay.env(&h.img), p),
                        Some(*item),
                        "{dpi} {item:?}"
                    );
                }
            }
            // Ngoài thanh: không có mục.
            let env = lay.env(&h.img);
            assert_eq!(h.a.side_item_under(&env, pt(side.x - 1, side.y)), None);
            assert_eq!(h.a.side_item_under(&env, pt(side.right(), side.y)), None);
        }
    }

    #[test]
    fn palette_has_48_distinct_hit_cells_inside_the_panel() {
        for dpi in [96u32, 144, 192, 288] {
            let mon = mon_for(dpi);
            let sel = Rect {
                x: mon.w / 4,
                y: mon.h / 4,
                w: mon.w / 3,
                h: mon.h / 3,
            };
            let h = H::with(sel, mon, dpi);
            let side = h.layout().side.unwrap();
            let pal = palette_rect(side, sel, mon, dpi);
            assert!(inside(pal, mon));
            let cells: Vec<Rect> = (0..PALETTE.len())
                .map(|i| palette_cell_rect(pal, dpi, i))
                .collect();
            for (i, c) in cells.iter().enumerate() {
                assert!(inside(*c, pal), "{dpi} ô {i}: {c:?} ngoài {pal:?}");
                assert_eq!(
                    palette_cell_at(pal, dpi, pt(c.x + c.w / 2, c.y + c.h / 2)),
                    Some(i)
                );
                assert_eq!(palette_cell_at(pal, dpi, pt(c.x, c.y)), Some(i));
                for (j, d) in cells.iter().enumerate().skip(i + 1) {
                    assert!(!overlaps(*c, *d), "{dpi}: ô {i} chồng ô {j}");
                }
            }
            // Lề bảng không phải ô; ngoài bảng không có ô.
            assert_eq!(palette_cell_at(pal, dpi, pt(pal.x, pal.y)), None);
            assert_eq!(palette_cell_at(pal, dpi, pt(pal.x - 5, pal.y)), None);
        }
    }

    #[test]
    fn tips_stay_on_screen() {
        for dpi in [96u32, 192] {
            let mon = mon_for(dpi);
            let k = dpi as i32;
            let text = (200 * k / 96, 30 * k / 96);
            for sel in corners(mon) {
                let h = H::with(sel, mon, dpi);
                let lay = h.layout();
                let side = lay.side.unwrap();
                let l = side_layout_for(mon, dpi);
                for i in 0..darkshot_core::overlay_layout::SIDE_ITEMS.len() {
                    let btn = darkshot_core::overlay_layout::side_item_rect(side, &l, i).unwrap();
                    let t = side_tip_rect(side, btn, sel, mon, dpi, text);
                    assert!(inside(t, mon), "{dpi} {sel:?} {t:?}");
                }
                for f in [Flyout::Sizes, Flyout::Shapes, Flyout::Palette] {
                    let fly = flyout_rect_for(f, side, sel, mon, dpi).unwrap();
                    let t = flyout_tip_rect(fly, fly, mon, dpi, text);
                    assert!(inside(t, mon), "{dpi} {sel:?} {f:?} {t:?}");
                }
            }
        }
    }

    #[test]
    fn local_redraw_only_covers_the_stroke_not_the_whole_screen() {
        let mut h = H::new();
        h.a.use_tool(Tool::Pen, 96);
        h.a.take_dirty();
        h.down(pt(100, 100));
        h.a.take_dirty();
        let mut total = 0i64;
        for i in 1..200 {
            h.drag(pt(100 + i, 100 + (i % 7)), false);
            for r in h.a.take_dirty() {
                assert!(r.w < 60 && r.h < 60, "vùng bẩn nhỏ theo đoạn nét: {r:?}");
                total += i64::from(r.w) * i64::from(r.h);
            }
        }
        // Tổng diện tích vẽ lại nhỏ hơn nhiều so với 200 lần vẽ lại cả vùng chọn.
        assert!(total < i64::from(SEL.w) * i64::from(SEL.h) * 200 / 20);
        h.up(pt(300, 100), false);
        assert_eq!(h.a.shapes().len(), 1);
    }

    #[test]
    fn take_shapes_commits_text_last_and_empties_the_log() {
        let mut h = H::new();
        h.draw(Tool::Rect, pt(100, 100), pt(180, 160));
        h.a.use_tool(Tool::Text, 96);
        h.down(pt(300, 250));
        h.a.text_insert("cuối");
        let shapes = h.a.take_shapes(96);
        assert_eq!(shapes.len(), 2);
        assert!(matches!(shapes[0], Shape::Box { .. }));
        assert!(
            matches!(shapes[1], Shape::Label { .. }),
            "chữ đang gõ ở trên cùng"
        );
        assert!(h.a.shapes().is_empty());
        assert!(h.a.text().is_none());
    }

    #[test]
    fn burned_image_shows_later_shapes_in_front() {
        let mut h = H::new();
        h.draw(Tool::Blur, pt(100, 100), pt(200, 200));
        h.draw(Tool::Blur, pt(150, 150), pt(300, 300));
        let shapes = h.a.take_shapes(96);
        let colors: Vec<Rgb> = shapes
            .iter()
            .map(|s| match s {
                Shape::Blur { color, .. } => *color,
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(colors.len(), 2);
        assert_ne!(colors[0], colors[1]);
        let (w, hh) = (h.img.width as i32, h.img.height as i32);
        let mut out = h.img.clone();
        crate::annotate_draw::burn(
            &mut out,
            &shapes,
            Rect {
                x: 0,
                y: 0,
                w,
                h: hh,
            },
        )
        .unwrap();
        let px = |x: i32, y: i32| {
            let i = ((y * w + x) * 4) as usize;
            Rgb(out.bgra[i + 2], out.bgra[i + 1], out.bgra[i])
        };
        // Phần chỉ có hình dưới; phần chồng nhau thấy hình trên (vẽ sau); phần chỉ có hình trên.
        assert_eq!(px(120, 120), colors[0]);
        assert_eq!(
            px(175, 175),
            colors[1],
            "chỗ chồng nhau: hình vẽ sau ở phía trước"
        );
        assert_eq!(px(250, 250), colors[1]);
    }

    #[test]
    fn constants_match_core() {
        assert_eq!(GAP, 6);
        assert_eq!(shape_tol(192), dip_to_px(HIT_TOL_DIP, 192));
        assert!(grip_radius(192) > grip_radius(96));
    }

    // ---- Task 112: kiểu cho Hình và Chữ

    /// Bấm (nhấn rồi nhả) ô `cell` của bảng thuộc tính đang hiện.
    fn click_style(h: &mut H, cell: StyleCell) {
        let lay = h.layout();
        let env = lay.env(&h.img);
        let kind = h.a.style_kind().expect("bảng đang hiện");
        let rect = h.a.style_rect(&env).expect("có chỗ cho bảng");
        let l = style_layout(rect, kind, h.dpi);
        let r = match cell {
            StyleCell::Color(i) => l.cells[i],
            StyleCell::Target(StyleTarget::Fill) => l.fill_chip,
            StyleCell::Target(StyleTarget::Border) => l.border_chip,
            StyleCell::NoFill => l.no_fill,
            StyleCell::Mode(m) => {
                let i = HighlightMode::ALL.iter().position(|x| *x == m).unwrap();
                l.modes.expect("bảng Highlight có hàng chế độ")[i]
            }
            StyleCell::Width(d) if d < 0 => l.width_minus,
            StyleCell::Width(_) => l.width_plus,
            StyleCell::Opacity(d) if d < 0 => l.opacity_minus,
            StyleCell::Opacity(_) => l.opacity_plus,
            StyleCell::Align(a) => {
                let i = Align::ALL.iter().position(|x| *x == a).unwrap();
                l.aligns.expect("Chữ có hàng căn lề")[i]
            }
        };
        let c = pt(r.x + r.w / 2, r.y + r.h / 2);
        assert_eq!(h.down(c), Down::Handled, "{cell:?}");
        assert!(h.up(c, false), "{cell:?}");
    }

    fn box_style(a: &Annot, i: usize) -> Style {
        a.shapes()[i].style().expect("Hình có kiểu")
    }

    #[test]
    fn new_shapes_start_with_the_default_style() {
        let mut h = H::new();
        h.draw(Tool::Rect, pt(100, 100), pt(200, 160));
        h.draw(Tool::Ellipse, pt(220, 100), pt(300, 160));
        let st = box_style(&h.a, 0);
        assert_eq!(st.fill, None);
        assert_eq!(st.opacity, 1.0);
        assert_eq!(st.stroke, DEFAULT_COLOR);
        assert_eq!(st.stroke_width, STROKE_DIP);
        assert!(matches!(h.a.shapes()[1], Shape::Box { oval: true, .. }));
        h.a.use_tool(Tool::Text, 96);
        h.down(pt(100, 200));
        h.a.text_insert("ab");
        h.a.close_text(true);
        match &h.a.shapes()[2] {
            Shape::Label {
                style, align, size, ..
            } => {
                assert_eq!(style.fill, None);
                assert_eq!(style.stroke_width, 0.0);
                assert_eq!(style.opacity, 1.0);
                assert_eq!(*align, Align::Left);
                assert_eq!(*size, 18.0);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn style_panel_shows_for_box_and_text_tools_or_a_selected_such_shape() {
        let mut h = H::new();
        assert_eq!(h.a.style_kind(), None);
        for (tool, kind) in [
            (Tool::Rect, Some(StyleKind::Box)),
            (Tool::Ellipse, Some(StyleKind::Box)),
            (Tool::Text, Some(StyleKind::Label)),
            (Tool::Line, None),
            (Tool::Arrow, None),
            (Tool::Pen, None),
            (Tool::Marker, Some(StyleKind::MarkerFree)),
            (Tool::Blur, None),
            (Tool::Step, None),
            (Tool::Select, None),
        ] {
            h.a.use_tool(tool, 96);
            assert_eq!(h.a.style_kind(), kind, "{tool:?}");
            assert_eq!(h.a.style_panel_open(), kind.is_some(), "{tool:?}");
        }
        // Đang chọn một Hình bằng công cụ Chọn: bảng hiện; chọn nét thẳng thì không.
        h.draw(Tool::Rect, pt(100, 100), pt(200, 160));
        h.draw(Tool::Line, pt(100, 300), pt(300, 300));
        h.a.use_tool(Tool::Select, 96);
        h.down(pt(100, 130));
        h.up(pt(100, 130), false);
        assert_eq!(h.a.style_kind(), Some(StyleKind::Box));
        h.a.deselect(96);
        h.down(pt(200, 300));
        h.up(pt(200, 300), false);
        assert_eq!(h.a.selected(), Some(1));
        assert_eq!(h.a.style_kind(), None);
        // Một bảng bật ra từ thanh dọc thì bảng thuộc tính nhường chỗ.
        h.a.use_tool(Tool::Rect, 96);
        assert!(h.a.style_panel_open());
        h.click_side(SideItem::Size);
        assert!(!h.a.style_panel_open());
        assert!(h.a.escape(96));
        assert!(h.a.style_panel_open());
    }

    #[test]
    fn applying_a_style_to_the_selection_is_one_undo_step_and_keeps_the_layer() {
        let mut h = H::new();
        h.draw(Tool::Rect, pt(100, 100), pt(200, 160));
        h.draw(Tool::Rect, pt(120, 110), pt(220, 170));
        h.draw(Tool::Rect, pt(140, 120), pt(240, 180));
        h.a.use_tool(Tool::Select, 96);
        h.down(pt(120, 125));
        h.up(pt(120, 125), false);
        assert_eq!(h.a.selected(), Some(1));
        let before: Vec<Shape> = h.a.shapes().to_vec();
        click_style(&mut h, StyleCell::Color(4));
        // Ô màu mặc định áp cho Nền.
        assert_eq!(box_style(&h.a, 1).fill, Some(PALETTE[4]));
        assert_eq!(h.a.shapes().len(), 3);
        assert_eq!(h.a.selected(), Some(1), "giữ chọn và giữ lớp");
        assert_eq!(h.a.shapes()[0], before[0]);
        assert_eq!(h.a.shapes()[2], before[2]);
        // Một lần hoàn tác khôi phục đúng kiểu cũ, làm lại đưa về kiểu mới.
        h.a.undo(96);
        assert_eq!(h.a.shapes(), before.as_slice());
        h.a.redo(96);
        assert_eq!(box_style(&h.a, 1).fill, Some(PALETTE[4]));
        // Hoàn tác/làm lại bỏ chọn: chọn lại. Không nền: một bước nữa; áp lại y hệt thì không ghi sổ.
        h.down(pt(120, 125));
        h.up(pt(120, 125), false);
        assert_eq!(h.a.selected(), Some(1));
        click_style(&mut h, StyleCell::NoFill);
        assert_eq!(box_style(&h.a, 1).fill, None);
        click_style(&mut h, StyleCell::NoFill);
        h.a.undo(96);
        assert_eq!(box_style(&h.a, 1).fill, Some(PALETTE[4]));
        assert!(h.a.can_undo());
    }

    #[test]
    fn style_choices_become_the_default_for_the_next_shape() {
        let mut h = H::new();
        h.a.use_tool(Tool::Rect, 96);
        click_style(&mut h, StyleCell::Color(10));
        click_style(&mut h, StyleCell::Opacity(-1));
        click_style(&mut h, StyleCell::Opacity(-1));
        click_style(&mut h, StyleCell::Width(1));
        click_style(&mut h, StyleCell::Target(StyleTarget::Border));
        click_style(&mut h, StyleCell::Color(20));
        assert_eq!(h.a.color(), PALETTE[20], "viền Hình là màu của nút Màu");
        h.gesture(pt(100, 100), pt(200, 160));
        let st = box_style(&h.a, 0);
        assert_eq!(st.fill, Some(PALETTE[10]));
        assert!((st.opacity - 0.8).abs() < 1e-6, "{}", st.opacity);
        assert_eq!(st.stroke, PALETTE[20]);
        assert_eq!(st.stroke_width, STROKE_DIP + 1.0);
        // Hình xem trước khi kéo đã mang kiểu.
        h.down(pt(250, 100));
        h.drag(pt(330, 160), false);
        assert!(h
            .a
            .layers()
            .iter()
            .any(|l| matches!(l, Layer::Boxed(Shape::Box { style, .. }) if style.fill == Some(PALETTE[10]))));
        h.up(pt(330, 160), false);
        // Kiểu nhớ qua các lần mở lớp phủ.
        let again = Annot::with_prefs(h.a.prefs());
        assert_eq!(again.prefs().box_style.fill, Some(PALETTE[10]));
        assert_eq!(
            again.prefs().box_style.opacity,
            h.a.prefs().box_style.opacity
        );
        // Mức cỡ đặt lại độ dày viền mặc định.
        h.a.set_size(SizeLevel::Huge, 96);
        assert_eq!(
            h.a.prefs().box_style.width_dip,
            SizeLevel::Huge.stroke_dip()
        );
    }

    #[test]
    fn opacity_and_width_stay_in_range() {
        let mut h = H::new();
        h.a.use_tool(Tool::Rect, 96);
        for _ in 0..30 {
            click_style(&mut h, StyleCell::Opacity(-1));
            click_style(&mut h, StyleCell::Width(1));
        }
        let v = h.a.style_view(96).unwrap();
        assert!(v.opacity >= 0.1 && v.opacity <= 1.0, "{}", v.opacity);
        assert_eq!(v.width_dip, WIDTH_MAX_DIP);
        for _ in 0..30 {
            click_style(&mut h, StyleCell::Opacity(1));
            click_style(&mut h, StyleCell::Width(-1));
        }
        let v = h.a.style_view(96).unwrap();
        assert_eq!((v.opacity, v.width_dip), (1.0, 0.0));
    }

    #[test]
    fn a_filled_box_is_selected_by_its_inside() {
        let mut h = H::new();
        h.draw(Tool::Rect, pt(100, 100), pt(200, 160));
        h.a.use_tool(Tool::Select, 96);
        let lay = h.layout();
        assert_eq!(h.a.topmost_at(&lay.env(&h.img), pt(150, 130)), None);
        click_style_with_select(&mut h);
        let lay = h.layout();
        assert_eq!(h.a.topmost_at(&lay.env(&h.img), pt(150, 130)), Some(0));
    }

    // ---- Task 113: Highlight hai chế độ

    fn pick_mode(h: &mut H, m: HighlightMode) {
        h.a.use_tool(Tool::Marker, 96);
        click_style(h, StyleCell::Mode(m));
    }

    fn select_at(h: &mut H, p: Point) {
        h.a.use_tool(Tool::Select, 96);
        h.down(p);
        h.up(p, false);
    }

    #[test]
    fn highlight_defaults_to_freehand_and_the_marker_panel_shows() {
        let mut h = H::new();
        h.a.use_tool(Tool::Marker, 96);
        assert_eq!(h.a.style_kind(), Some(StyleKind::MarkerFree));
        let v = h.a.style_view(96).unwrap();
        assert_eq!(v.mode, Some(HighlightMode::Freehand));
        assert_eq!(v.width_dip, MARKER_DIP);
        h.gesture(pt(100, 100), pt(200, 100));
        assert!(matches!(h.a.shapes()[0], Shape::Marker { .. }));
        pick_mode(&mut h, HighlightMode::Rect);
        assert_eq!(h.a.style_kind(), Some(StyleKind::MarkerRect));
        assert_eq!(h.a.style_view(96).unwrap().opacity, HIGHLIGHT_OPACITY);
    }

    #[test]
    fn switching_mode_never_changes_drawn_shapes_and_is_remembered() {
        let mut h = H::new();
        h.a.use_tool(Tool::Marker, 96);
        h.gesture(pt(100, 100), pt(200, 100));
        let before = h.a.shapes().to_vec();
        let steps = h.a.can_undo();
        pick_mode(&mut h, HighlightMode::Rect);
        assert_eq!(h.a.shapes(), before.as_slice());
        assert_eq!(h.a.can_undo(), steps);
        assert_eq!(h.a.prefs().highlight_mode, HighlightMode::Rect);
        // Nhớ qua các lần mở lớp phủ.
        let again = Annot::with_prefs(h.a.prefs());
        assert_eq!(again.prefs().highlight_mode, HighlightMode::Rect);
        h.gesture(pt(100, 200), pt(300, 230));
        assert!(matches!(h.a.shapes()[1], Shape::Highlight { .. }));
        pick_mode(&mut h, HighlightMode::Freehand);
        assert_eq!(h.a.shapes().len(), 2);
    }

    #[test]
    fn rect_mode_drag_makes_a_normalized_borderless_highlight() {
        let mut h = H::new();
        pick_mode(&mut h, HighlightMode::Rect);
        // Kéo ngược chiều.
        h.down(pt(300, 230));
        h.drag(pt(100, 200), false);
        assert!(h
            .a
            .layers()
            .iter()
            .any(|l| matches!(l, Layer::Boxed(Shape::Highlight { .. }))));
        h.up(pt(100, 200), false);
        match &h.a.shapes()[0] {
            Shape::Highlight {
                a,
                b,
                color,
                opacity,
            } => {
                assert_eq!((a.x, a.y, b.x, b.y), (100.0, 200.0, 300.0, 230.0));
                assert_eq!(*color, h.a.color());
                assert_eq!(*opacity, HIGHLIGHT_OPACITY);
            }
            other => panic!("{other:?}"),
        }
        // Kích thước 0 hoặc quá nhỏ bị bỏ, không ghi sổ.
        let n = h.a.shapes().len();
        h.gesture(pt(150, 300), pt(150, 300));
        h.gesture(pt(150, 300), pt(250, 301));
        assert_eq!(h.a.shapes().len(), n);
    }

    #[test]
    fn highlight_rect_selects_moves_and_undoes() {
        let mut h = H::new();
        pick_mode(&mut h, HighlightMode::Rect);
        h.gesture(pt(100, 200), pt(300, 230));
        select_at(&mut h, pt(200, 215));
        assert_eq!(h.a.selected(), Some(0));
        assert_eq!(h.a.style_kind(), Some(StyleKind::MarkerRect));
        h.down(pt(200, 215));
        h.drag(pt(220, 235), false);
        h.up(pt(220, 235), false);
        match &h.a.shapes()[0] {
            Shape::Highlight { a, b, .. } => {
                assert_eq!((a.x, a.y, b.x, b.y), (120.0, 220.0, 320.0, 250.0))
            }
            other => panic!("{other:?}"),
        }
        h.a.undo(96);
        match &h.a.shapes()[0] {
            Shape::Highlight { a, .. } => assert_eq!((a.x, a.y), (100.0, 200.0)),
            other => panic!("{other:?}"),
        }
        h.a.redo(96);
        assert_eq!(h.a.shapes().len(), 1);
    }

    #[test]
    fn highlight_opacity_and_color_changes_are_one_undo_step_each() {
        let mut h = H::new();
        pick_mode(&mut h, HighlightMode::Rect);
        h.gesture(pt(100, 200), pt(300, 230));
        select_at(&mut h, pt(200, 215));
        let before = h.a.shapes().to_vec();
        click_style(&mut h, StyleCell::Opacity(1));
        match &h.a.shapes()[0] {
            Shape::Highlight { opacity, .. } => assert!((opacity - 0.5).abs() < 1e-6, "{opacity}"),
            other => panic!("{other:?}"),
        }
        h.a.undo(96);
        assert_eq!(h.a.shapes(), before.as_slice(), "một lần hoàn tác trả đủ");
        h.a.redo(96);
        select_at(&mut h, pt(200, 215));
        click_style(&mut h, StyleCell::Color(7));
        match &h.a.shapes()[0] {
            Shape::Highlight { color, opacity, .. } => {
                assert_eq!(*color, PALETTE[7]);
                assert!((opacity - 0.5).abs() < 1e-6);
            }
            other => panic!("{other:?}"),
        }
        // Độ trong suốt mặc định cho lần vẽ sau.
        assert!((h.a.prefs().highlight_opacity - 0.5).abs() < 1e-6);
        h.a.use_tool(Tool::Marker, 96);
        h.gesture(pt(100, 300), pt(300, 330));
        match &h.a.shapes()[1] {
            Shape::Highlight { opacity, .. } => assert!((opacity - 0.5).abs() < 1e-6),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn freehand_width_applies_to_selected_stroke_and_next_ones() {
        let mut h = H::new();
        h.a.use_tool(Tool::Marker, 96);
        click_style(&mut h, StyleCell::Width(1));
        assert_eq!(h.a.prefs().marker_width_dip, MARKER_DIP + 2.0);
        h.gesture(pt(100, 100), pt(200, 100));
        match &h.a.shapes()[0] {
            Shape::Marker { width, .. } => assert_eq!(*width, MARKER_DIP + 2.0),
            other => panic!("{other:?}"),
        }
        select_at(&mut h, pt(150, 100));
        assert_eq!(h.a.selected(), Some(0));
        let before = h.a.shapes().to_vec();
        click_style(&mut h, StyleCell::Width(-1));
        click_style(&mut h, StyleCell::Width(-1));
        match &h.a.shapes()[0] {
            Shape::Marker { width, .. } => assert_eq!(*width, MARKER_DIP - 2.0),
            other => panic!("{other:?}"),
        }
        h.a.undo(96);
        h.a.undo(96);
        assert_eq!(h.a.shapes(), before.as_slice());
        select_at(&mut h, pt(150, 100));
        for _ in 0..40 {
            click_style(&mut h, StyleCell::Width(1));
        }
        assert_eq!(h.a.style_view(96).unwrap().width_dip, MARKER_WIDTH_MAX_DIP);
        h.a.set_size(SizeLevel::Huge, 96);
        assert_eq!(h.a.prefs().marker_width_dip, SizeLevel::Huge.marker_dip());
    }

    #[test]
    fn highlight_rect_is_burned_where_it_was_drawn() {
        let mut h = H::new();
        pick_mode(&mut h, HighlightMode::Rect);
        h.gesture(pt(100, 200), pt(300, 230));
        let shapes = h.a.take_shapes(96);
        let (w, hh) = (h.img.width as i32, h.img.height as i32);
        let mut out = h.img.clone();
        crate::annotate_draw::burn(
            &mut out,
            &shapes,
            Rect {
                x: 0,
                y: 0,
                w,
                h: hh,
            },
        )
        .unwrap();
        let at = |im: &Image, x: i32, y: i32| {
            let i = ((y * w + x) * 4) as usize;
            [im.bgra[i], im.bgra[i + 1], im.bgra[i + 2]]
        };
        assert_ne!(
            at(&out, 200, 215),
            at(&h.img, 200, 215),
            "trong hình đã trộn màu"
        );
        for (x, y) in [(200, 190), (200, 240), (90, 215), (310, 215)] {
            assert_eq!(at(&out, x, y), at(&h.img, x, y), "ngoài hình: {x},{y}");
        }
    }

    /// Chọn hình đầu (bấm cạnh trái) rồi tô nền.
    fn click_style_with_select(h: &mut H) {
        h.down(pt(100, 130));
        h.up(pt(100, 130), false);
        click_style(h, StyleCell::Color(3));
    }

    #[test]
    fn width_change_grows_the_shape_bounds_and_marks_it_dirty() {
        let mut h = H::new();
        h.draw(Tool::Rect, pt(100, 100), pt(200, 160));
        h.a.use_tool(Tool::Select, 96);
        h.down(pt(100, 130));
        h.up(pt(100, 130), false);
        let _ = h.a.take_dirty();
        let before = h.a.bounds[0];
        for _ in 0..10 {
            click_style(&mut h, StyleCell::Width(1));
        }
        assert!(
            h.a.bounds[0].w > before.w,
            "{:?} {:?}",
            h.a.bounds[0],
            before
        );
        let dirty = h.a.take_dirty();
        assert!(!dirty.is_empty());
        // Vùng vẽ lại phủ hộp mới (chứa cả hộp cũ).
        let area = dirty
            .iter()
            .fold(None, |acc, r| union(acc, Some(*r)))
            .unwrap();
        assert!(area.x <= h.a.bounds[0].x && area.right() >= h.a.bounds[0].right());
    }

    #[test]
    fn text_style_applies_while_typing_and_on_commit() {
        let mut h = H::new();
        h.a.use_tool(Tool::Text, 96);
        h.down(pt(100, 100));
        h.a.text_insert("ab\ncdef");
        let narrow = h.a.text().unwrap().frame;
        click_style(&mut h, StyleCell::Color(2));
        click_style(&mut h, StyleCell::Width(1));
        click_style(&mut h, StyleCell::Align(Align::Right));
        assert!(h.a.text().is_some(), "bấm bảng không chốt ô chữ");
        let b = h.a.text().unwrap();
        assert_eq!(b.align, Align::Right);
        assert_eq!(b.style.fill, Some(PALETTE[2]));
        assert_eq!(b.style.stroke_width, 1.0);
        assert_eq!(b.edit.text(), "ab\ncdef");
        let _ = narrow;
        // Hình xem trước khi gõ cũng mang kiểu.
        let typing = h.a.layers().into_iter().last().unwrap();
        assert!(matches!(
            typing,
            Layer::Typing(Shape::Label {
                align: Align::Right,
                ..
            })
        ));
        h.a.close_text(true);
        match &h.a.shapes()[0] {
            Shape::Label { align, style, .. } => {
                assert_eq!(*align, Align::Right);
                assert_eq!(style.fill, Some(PALETTE[2]));
            }
            other => panic!("{other:?}"),
        }
        // Mặc định cho ô chữ kế tiếp.
        h.down(pt(100, 250));
        let b = h.a.text().unwrap();
        assert_eq!((b.align, b.style.fill), (Align::Right, Some(PALETTE[2])));
    }

    #[test]
    fn selected_text_changes_alignment_in_one_undo_step() {
        let mut h = H::new();
        h.a.use_tool(Tool::Text, 96);
        h.down(pt(100, 100));
        h.a.text_insert("short\nmuch longer line");
        h.a.close_text(true);
        h.a.use_tool(Tool::Select, 96);
        let b = h.a.bounds[0];
        h.down(pt(b.x + b.w / 2, b.y + b.h / 2));
        h.up(pt(b.x + b.w / 2, b.y + b.h / 2), false);
        assert_eq!(h.a.selected(), Some(0));
        assert_eq!(h.a.style_kind(), Some(StyleKind::Label));
        click_style(&mut h, StyleCell::Align(Align::Center));
        assert_eq!(h.a.shapes()[0].align(), Some(Align::Center));
        h.a.undo(96);
        assert_eq!(h.a.shapes()[0].align(), Some(Align::Left));
        assert!(!h.a.can_undo() || h.a.shapes().len() == 1);
    }

    #[test]
    fn clicking_the_panel_never_draws_and_margins_are_swallowed() {
        let mut h = H::new();
        h.a.use_tool(Tool::Rect, 96);
        let lay = h.layout();
        let rect = h.a.style_rect(&lay.env(&h.img)).unwrap();
        // Lề trên cùng của bảng: nuốt cú bấm, không tạo hình, không bắt chuột.
        let p = pt(rect.x + 1, rect.y + 1);
        assert_eq!(h.down(p), Down::Handled);
        assert!(!h.a.capturing());
        assert!(h.up(p, false) || !h.a.press_pending());
        assert!(h.a.shapes().is_empty());
        let hit = h.hit(p);
        assert_eq!(h.a.cursor_at(&lay.env(&h.img), p, hit), CursorKind::Arrow);
    }

    #[test]
    fn escape_still_unwinds_to_cancel_with_the_panel_open() {
        let mut h = H::new();
        h.a.use_tool(Tool::Rect, 96);
        assert!(h.a.style_panel_open());
        // Không còn gì để đóng: Esc trả false để bên gọi hủy cả lớp phủ.
        assert!(!h.a.escape(96));
        h.a.use_tool(Tool::Text, 96);
        h.down(pt(100, 100));
        h.a.text_insert("x");
        assert!(h.a.escape(96), "đóng ô chữ");
        assert!(!h.a.escape(96));
    }

    #[test]
    fn style_panel_stays_on_screen_and_outside_the_selection_when_it_fits() {
        for dpi in [96u32, 144, 192] {
            for kind in [StyleKind::Box, StyleKind::Label] {
                let mon = mon_for(dpi);
                let sel = Rect {
                    x: mon.x + mon.w / 4,
                    y: mon.y + mon.h / 4,
                    w: mon.w / 3,
                    h: mon.h / 3,
                };
                let bar = bar_rect(sel, dpi, mon);
                let side = side_rect(sel, bar, dpi, mon);
                let r = style_rect_for(kind, side, sel, mon, dpi).unwrap();
                assert!(inside(r, mon), "{r:?} {mon:?}");
                assert!(!overlaps(r, sel), "ưu tiên phía ngoài: {r:?} {sel:?}");
                assert!(!overlaps(r, side));
            }
        }
        // Vùng chọn sát mép phải: không còn chỗ phía ngoài, bảng vào phía vùng chọn nhưng vẫn trong màn hình.
        let mon = mon_for(96);
        let sel = Rect {
            x: mon.x + 100,
            y: mon.y + 100,
            w: mon.w - 140,
            h: 300,
        };
        let bar = bar_rect(sel, 96, mon);
        let side = side_rect(sel, bar, 96, mon);
        let r = style_rect_for(StyleKind::Label, side, sel, mon, 96).unwrap();
        assert!(inside(r, mon));
    }
}
