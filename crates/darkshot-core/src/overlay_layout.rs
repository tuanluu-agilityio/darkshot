//! Bố cục thuần của lớp phủ chọn vùng: thanh ngang, thanh dọc, bảng Cỡ và bảng Hình, tính theo
//! pixel logic ở 96 DPI nhân với `dpi` (96 = 100%). Dùng chung cho mọi giao diện.

use crate::actions::Action;
use crate::annotate::{Pt, Shape, SizeLevel, Tool};
use crate::color::picker_layout;
use crate::edit::{frame, handles};
use crate::geometry::{place_side_toolbar, place_toolbar, side_layout, Point, Rect, SideLayout};
use crate::i18n::Msg;

/// Mục của thanh ngang (cạnh vùng chọn).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarItem {
    Undo,
    Redo,
    Act(Action),
}

impl BarItem {
    pub fn tip(self) -> Msg {
        match self {
            BarItem::Undo => Msg::ToolUndo,
            BarItem::Redo => Msg::ToolRedo,
            BarItem::Act(Action::Upload) => Msg::OverlayUpload,
            BarItem::Act(Action::Save) => Msg::OverlaySave,
            BarItem::Act(Action::Copy) => Msg::OverlayCopy,
            BarItem::Act(Action::Cancel) => Msg::OverlayCancel,
            BarItem::Act(Action::Pin) => Msg::OverlayPin,
            BarItem::Act(Action::Ocr) => Msg::OverlayOcr,
            BarItem::Act(Action::AddStep) => Msg::OverlayAddStep,
        }
    }
}

/// Các ô của thanh ngang theo thứ tự: Hoàn tác, Làm lại | Tải lên, Lưu, Chép, Ghim, Nhận dạng chữ,
/// Hủy; `None` là vạch ngăn.
pub const BAR_CELLS: [Option<BarItem>; 9] = [
    Some(BarItem::Undo),
    Some(BarItem::Redo),
    None,
    Some(BarItem::Act(Action::Upload)),
    Some(BarItem::Act(Action::Save)),
    Some(BarItem::Act(Action::Copy)),
    Some(BarItem::Act(Action::Pin)),
    Some(BarItem::Act(Action::Ocr)),
    Some(BarItem::Act(Action::Cancel)),
];

// Kích thước ở 96 DPI (100%); nhân theo DPI của màn hình chứa phần tử.
pub const BUTTON: i32 = 36;
pub const GAP: i32 = 6;
pub const HANDLE_SIZE: i32 = 9;
pub const HANDLE_HIT: i32 = 6;
pub const ICON: i32 = 20;
/// Ô vạch ngăn của thanh dọc (vạch 1 px ở giữa, phần còn lại là lề).
pub const SIDE_SEP: i32 = 9;
/// Tam giác chỉ độ sáng: rộng (DIP).
pub const LUM_ARROW: i32 = 8;
/// Dấu chữ thập trong ô sắc độ: nửa chiều dài, khe giữa, độ dày (DIP).
pub const CROSS_ARM: i32 = 9;
pub const CROSS_GAP: i32 = 3;
pub const CROSS_THICK: i32 = 2;
/// Ô vuông màu hiện tại trên nút Màu.
pub const SWATCH: i32 = 16;
/// Vạch đánh dấu công cụ đang chọn ở mép trái nút, và viền ô màu đang chọn.
pub const MARK: i32 = 2;

/// Khung nét đứt quanh ô chữ: cách chữ, rộng tối thiểu khi ô rỗng, nét và khe (DIP).
pub const TEXT_FRAME_PAD: i32 = 3;
pub const TEXT_EMPTY_W: i32 = 40;
pub const TEXT_DASH: i32 = 4;
pub const TEXT_DASH_GAP: i32 = 3;

/// Mục của thanh công cụ vẽ (thanh dọc).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideItem {
    Tool(Tool),
    /// Nút Hình: bật hình nhớ, hoặc mở bảng Hình khi đang dùng một hình.
    Shapes,
    Size,
    Color,
}

impl SideItem {
    pub fn tip(self) -> Msg {
        match self {
            SideItem::Tool(Tool::Select) => Msg::ToolSelect,
            SideItem::Tool(Tool::Pen) => Msg::ToolPen,
            SideItem::Tool(Tool::Line) => Msg::ToolLine,
            SideItem::Tool(Tool::Arrow) => Msg::ToolArrow,
            SideItem::Tool(Tool::Rect) => Msg::ToolRect,
            SideItem::Tool(Tool::Ellipse) => Msg::ToolEllipse,
            SideItem::Tool(Tool::Marker) => Msg::ToolMarker,
            SideItem::Tool(Tool::Blur) => Msg::ToolBlur,
            SideItem::Tool(Tool::Text) => Msg::ToolText,
            SideItem::Tool(Tool::Step) => Msg::ToolStep,
            SideItem::Shapes => Msg::ToolShapes,
            SideItem::Size => Msg::ToolSize,
            SideItem::Color => Msg::ToolColor,
        }
    }
}

/// Thứ tự trên thanh dọc: Chọn | Bút, Hình, Bút dạ quang, Làm mờ, Chữ, Đánh số | Cỡ, Màu.
pub const SIDE_ITEMS: [SideItem; 9] = [
    SideItem::Tool(Tool::Select),
    SideItem::Tool(Tool::Pen),
    SideItem::Shapes,
    SideItem::Tool(Tool::Marker),
    SideItem::Tool(Tool::Blur),
    SideItem::Tool(Tool::Text),
    SideItem::Tool(Tool::Step),
    SideItem::Size,
    SideItem::Color,
];
/// Vạch ngăn nằm trước các mục này của `SIDE_ITEMS`.
pub const SIDE_SEPS: [usize; 2] = [1, 7];

/// Nhóm Hình (nút Hình và bảng Hình), theo thứ tự trong bảng.
pub const SHAPE_TOOLS: [Tool; 4] = [Tool::Line, Tool::Arrow, Tool::Rect, Tool::Ellipse];

pub fn is_shape(t: Tool) -> bool {
    SHAPE_TOOLS.contains(&t)
}

/// Gợi ý của ô công cụ `t` trong bảng Hình.
pub fn shape_tip(t: Tool) -> Msg {
    SideItem::Tool(t).tip()
}

/// Các ô của thanh ngang: không có phiên chụp từng bước thì đúng `BAR_CELLS`; có phiên thì thêm
/// Thêm bước ngay trước Hủy.
pub fn bar_cells(steps: bool) -> Vec<Option<BarItem>> {
    let mut v = BAR_CELLS.to_vec();
    if steps {
        let cancel = v
            .iter()
            .position(|c| *c == Some(BarItem::Act(Action::Cancel)))
            .unwrap_or(v.len());
        v.insert(cancel, Some(BarItem::Act(Action::AddStep)));
    }
    v
}

/// Bố cục thanh ngang ở một DPI: cỡ thanh và ô của từng phần (tương đối góc trên trái thanh),
/// theo thứ tự `bar_cells`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BarLayout {
    pub size: (i32, i32),
    pub cells: Vec<(Option<BarItem>, Rect)>,
}

/// Thanh ngang khi không có phiên chụp từng bước.
pub fn bar_layout(dpi: u32) -> BarLayout {
    bar_layout_with(dpi, false)
}

/// Thanh ngang: các nút vuông `BUTTON` xếp liền, vạch ngăn rộng `SIDE_SEP`; `steps`: có phiên.
pub fn bar_layout_with(dpi: u32, steps: bool) -> BarLayout {
    let (b, sep) = (scaled(BUTTON, dpi), scaled(SIDE_SEP, dpi));
    let mut x = 0;
    let cells = bar_cells(steps)
        .into_iter()
        .map(|item| {
            let w = if item.is_some() { b } else { sep };
            let r = Rect { x, y: 0, w, h: b };
            x += w;
            (item, r)
        })
        .collect();
    BarLayout {
        size: (x, b),
        cells,
    }
}

/// Ô của mục `item` trên thanh ngang `bar` khi không có phiên.
pub fn bar_item_rect(bar: Rect, dpi: u32, item: BarItem) -> Option<Rect> {
    bar_item_rect_with(bar, dpi, false, item)
}

/// Ô của mục `item` trên thanh ngang `bar`.
pub fn bar_item_rect_with(bar: Rect, dpi: u32, steps: bool, item: BarItem) -> Option<Rect> {
    bar_layout_with(dpi, steps)
        .cells
        .iter()
        .find(|(i, _)| *i == Some(item))
        .map(|(_, r)| Rect {
            x: bar.x + r.x,
            y: bar.y + r.y,
            ..*r
        })
}

/// Mục của thanh ngang `bar` (không phiên) dưới điểm `p`.
pub fn bar_item_at(bar: Rect, p: Point, dpi: u32) -> Option<BarItem> {
    bar_item_at_with(bar, p, dpi, false)
}

/// Mục của thanh ngang `bar` dưới điểm `p`; vạch ngăn và ngoài thanh trả `None`.
pub fn bar_item_at_with(bar: Rect, p: Point, dpi: u32, steps: bool) -> Option<BarItem> {
    if bar.w <= 0 || !bar.contains(p) {
        return None;
    }
    bar_layout_with(dpi, steps)
        .cells
        .iter()
        .find(|(_, r)| {
            Rect {
                x: bar.x + r.x,
                y: bar.y + r.y,
                ..*r
            }
            .contains(p)
        })
        .and_then(|(i, _)| *i)
}

/// Vị trí của `item` trong `SIDE_ITEMS` (mọi mục đều có mặt).
pub fn side_index(item: SideItem) -> usize {
    SIDE_ITEMS.iter().position(|x| *x == item).unwrap_or(0)
}

/// Bố cục thanh dọc trên màn hình `mon` ở `dpi`: một cột, hoặc hai cột khi một cột cao hơn màn
/// hình trừ lề `GAP` hai đầu.
pub fn side_layout_for(mon: Rect, dpi: u32) -> SideLayout {
    side_layout(
        SIDE_ITEMS.len(),
        &SIDE_SEPS,
        scaled(BUTTON, dpi),
        scaled(SIDE_SEP, dpi),
        mon.h - 2 * scaled(GAP, dpi),
    )
}

/// Ô của mục thứ `i` trên thanh dọc đặt tại `side` với bố cục `l`.
pub fn side_item_rect(side: Rect, l: &SideLayout, i: usize) -> Option<Rect> {
    l.items.get(i).map(|r| Rect {
        x: side.x + r.x,
        y: side.y + r.y,
        ..*r
    })
}

/// Mục của thanh dọc dưới điểm `p`; vạch ngăn và lề của nó trả `None`.
pub fn side_item_at(side: Rect, l: &SideLayout, p: Point) -> Option<SideItem> {
    if side.w <= 0 || !side.contains(p) {
        return None;
    }
    (0..SIDE_ITEMS.len())
        .find(|i| side_item_rect(side, l, *i).is_some_and(|r| r.contains(p)))
        .and_then(|i| SIDE_ITEMS.get(i).copied())
}

/// Gợi ý của ô mức cỡ.
pub fn size_tip(level: SizeLevel) -> Msg {
    match level {
        SizeLevel::Small => Msg::SizeSmall,
        SizeLevel::Medium => Msg::SizeMedium,
        SizeLevel::Large => Msg::SizeLarge,
        SizeLevel::Huge => Msg::SizeHuge,
    }
}

/// Thanh ngang và thanh dọc (không phiên chụp từng bước).
pub fn bars_for(sel: Rect, mon: Rect, dpi: u32) -> (Rect, Rect) {
    bars_for_with(sel, mon, dpi, false)
}

/// Thanh ngang và thanh dọc cho vùng chọn `sel`, màn hình chứa góc dưới phải `mon` ở `dpi`;
/// `steps`: có phiên chụp từng bước (thanh ngang thêm một ô).
pub fn bars_for_with(sel: Rect, mon: Rect, dpi: u32, steps: bool) -> (Rect, Rect) {
    let gap = scaled(GAP, dpi);
    let bar = place_toolbar(sel, bar_layout_with(dpi, steps).size, gap, mon);
    let size = side_layout_for(mon, dpi).size;
    let side = place_side_toolbar(sel, size, gap, mon, Some(bar));
    (bar, side)
}

/// Thanh dọc nằm ở nửa phải vùng chọn (ngoài bên phải, hoặc trong vùng sát mép phải).
pub fn side_on_right(side: Rect, sel: Rect) -> bool {
    2 * side.x + side.w > 2 * sel.x + sel.w
}

/// Đặt `r` vào trong `m` theo cả hai chiều.
pub fn clamp_into(mut r: Rect, m: Rect) -> Rect {
    r.x = r.x.min(m.right() - r.w).max(m.x);
    r.y = r.y.min(m.bottom() - r.h).max(m.y);
    r
}

/// Bảng cỡ `size` cạnh nút `btn` của thanh dọc `side`: bên trái thanh nếu thanh nằm nửa phải vùng
/// chọn, ngược lại bên phải; giữa theo chiều dọc nút; kẹp vào màn hình `mon` (màn hình hẹp hơn bảng
/// thì bảng sát mép trái màn hình).
pub fn flyout_rect(
    side: Rect,
    btn: Rect,
    sel: Rect,
    mon: Rect,
    dpi: u32,
    size: (i32, i32),
) -> Rect {
    let gap = scaled(GAP, dpi);
    let (w, h) = size;
    let x = if side_on_right(side, sel) {
        side.x - gap - w
    } else {
        side.right() + gap
    };
    clamp_into(
        Rect {
            x,
            y: btn.y + btn.h / 2 - h / 2,
            w,
            h,
        },
        mon,
    )
}

/// Ô của mục `item` trên thanh dọc `side` (bố cục theo màn hình `mon`).
pub fn side_button(side: Rect, mon: Rect, dpi: u32, item: SideItem) -> Rect {
    let l = side_layout_for(mon, dpi);
    side_item_rect(side, &l, side_index(item)).unwrap_or(side)
}

/// Bảng màu (dạng gọn hoặc đủ theo `expanded`) cạnh nút Màu của thanh dọc `side`.
pub fn palette_rect_for(side: Rect, sel: Rect, mon: Rect, dpi: u32, expanded: bool) -> Rect {
    let btn = side_button(side, mon, dpi, SideItem::Color);
    flyout_rect(side, btn, sel, mon, dpi, picker_layout(dpi, expanded).size)
}

/// Bảng Cỡ cạnh nút Cỡ `btn`: bốn ô vuông `BUTTON` ngang, lề `GAP`.
pub fn size_flyout_rect(side: Rect, btn: Rect, sel: Rect, mon: Rect, dpi: u32) -> Rect {
    let (b, gap) = (scaled(BUTTON, dpi), scaled(GAP, dpi));
    let n = SizeLevel::ALL.len() as i32;
    flyout_rect(side, btn, sel, mon, dpi, (n * b + 2 * gap, b + 2 * gap))
}

/// Bảng Hình cạnh nút Hình `btn`: bốn ô vuông `BUTTON` ngang, lề `GAP` (như bảng Cỡ).
pub fn shapes_flyout_rect(side: Rect, btn: Rect, sel: Rect, mon: Rect, dpi: u32) -> Rect {
    let (b, gap) = (scaled(BUTTON, dpi), scaled(GAP, dpi));
    let n = SHAPE_TOOLS.len() as i32;
    flyout_rect(side, btn, sel, mon, dpi, (n * b + 2 * gap, b + 2 * gap))
}

/// Bảng Hình cạnh nút Hình của thanh dọc `side`.
pub fn shapes_rect_for(side: Rect, sel: Rect, mon: Rect, dpi: u32) -> Rect {
    let btn = side_button(side, mon, dpi, SideItem::Shapes);
    shapes_flyout_rect(side, btn, sel, mon, dpi)
}

/// Ô thứ `i` (theo `SHAPE_TOOLS`) của bảng Hình `fly`; cùng hình học với ô bảng Cỡ.
pub fn shape_cell_rect(fly: Rect, dpi: u32, i: usize) -> Rect {
    size_cell_rect(fly, dpi, i)
}

/// Công cụ dưới điểm `p` của bảng Hình `fly`; lề trả `None`.
pub fn shape_cell_at(fly: Rect, dpi: u32, p: Point) -> Option<Tool> {
    SHAPE_TOOLS
        .iter()
        .enumerate()
        .find(|(i, _)| shape_cell_rect(fly, dpi, *i).contains(p))
        .map(|(_, t)| *t)
}

/// Bảng Cỡ cạnh nút Cỡ của thanh dọc `side`.
pub fn size_rect_for(side: Rect, sel: Rect, mon: Rect, dpi: u32) -> Rect {
    let btn = side_button(side, mon, dpi, SideItem::Size);
    size_flyout_rect(side, btn, sel, mon, dpi)
}

/// Ô thứ `i` (theo `SizeLevel::ALL`) của bảng Cỡ `fly`.
pub fn size_cell_rect(fly: Rect, dpi: u32, i: usize) -> Rect {
    let (b, gap) = (scaled(BUTTON, dpi), scaled(GAP, dpi));
    Rect {
        x: fly.x + gap + b * i as i32,
        y: fly.y + gap,
        w: b,
        h: b,
    }
}

/// Mức cỡ dưới điểm `p` của bảng Cỡ `fly`; lề trả `None`.
pub fn size_cell_at(fly: Rect, dpi: u32, p: Point) -> Option<SizeLevel> {
    SizeLevel::ALL
        .iter()
        .enumerate()
        .find(|(i, _)| size_cell_rect(fly, dpi, *i).contains(p))
        .map(|(_, l)| *l)
}

pub fn union(a: Option<Rect>, b: Option<Rect>) -> Option<Rect> {
    match (a, b) {
        (Some(a), Some(b)) => {
            let x0 = a.x.min(b.x);
            let y0 = a.y.min(b.y);
            let x1 = a.right().max(b.right());
            let y1 = a.bottom().max(b.bottom());
            Some(Rect {
                x: x0,
                y: y0,
                w: x1 - x0,
                h: y1 - y0,
            })
        }
        (a, None) => a,
        (None, b) => b,
    }
}

/// Vùng `r` nằm trọn trong ảnh BGRA `img_w`×`img_h` và bộ đệm dài `len` đủ chứa ảnh.
pub fn blit_in_bounds(r: Rect, img_w: i32, img_h: i32, len: usize) -> bool {
    let (x, y, w, h) = (
        i64::from(r.x),
        i64::from(r.y),
        i64::from(r.w),
        i64::from(r.h),
    );
    let (iw, ih) = (i64::from(img_w), i64::from(img_h));
    let need = iw.checked_mul(ih).and_then(|n| n.checked_mul(4));
    w > 0
        && h > 0
        && x >= 0
        && y >= 0
        && x + w <= iw
        && y + h <= ih
        && need.is_some_and(|n| n <= i64::try_from(len).unwrap_or(i64::MAX))
}

/// `v` pixel ở 96 DPI đổi sang pixel vật lý ở `dpi`, làm tròn, tối thiểu 1.
pub fn scaled(v: i32, dpi: u32) -> i32 {
    let dpi = i64::from(dpi.max(1));
    ((i64::from(v) * dpi + 48) / 96).clamp(1, i64::from(i32::MAX)) as i32
}

pub fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = a.right().min(b.right());
    let y1 = a.bottom().min(b.bottom());
    (x1 > x0 && y1 > y0).then_some(Rect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    })
}

pub fn inflate(r: Rect, d: i32) -> Rect {
    Rect {
        x: r.x - d,
        y: r.y - d,
        w: r.w + 2 * d,
        h: r.h + 2 * d,
    }
}

/// Đặt `r` vào trong `m` theo chiều ngang.
pub fn clamp_x(mut r: Rect, m: Rect) -> Rect {
    r.x = r.x.min(m.right() - r.w).max(m.x);
    r
}

pub fn to_pt(p: Point) -> Pt {
    Pt {
        x: p.x as f32,
        y: p.y as f32,
    }
}

/// Vùng màn hình của hình `shape` (hộp bao `bounds`) cùng khung chọn và tay nắm của nó, nới `pad`.
pub fn selection_dirty(shape: &Shape, bounds: Rect, pad: i32) -> Rect {
    let mut r = union(Some(bounds), frame(shape, bounds));
    for (_, c) in handles(shape, bounds) {
        let dot = Rect {
            x: c.x.floor() as i32,
            y: c.y.floor() as i32,
            w: 1,
            h: 1,
        };
        r = union(r, Some(dot));
    }
    inflate(r.unwrap_or(bounds), pad)
}

/// Hộp bao ước lượng của Chữ `new` (biến đổi từ `old` có hộp đo `old_bounds`): phóng hộp đo theo
/// cùng hệ số cỡ quanh gốc chữ; đổi cỡ thì nới thêm 2 px (đo thật lúc chốt).
pub fn text_preview_bounds(old: &Shape, old_bounds: Rect, new: &Shape) -> Option<Rect> {
    let (
        Shape::Text {
            origin: o, size: s, ..
        },
        Shape::Text {
            origin: n, size: t, ..
        },
    ) = (old, new)
    else {
        return None;
    };
    let k = t / s;
    if !k.is_finite() || k <= 0.0 {
        return None;
    }
    let map = |v: i32, from: f32, to: f32| to + (v as f32 - from) * k;
    let x0 = map(old_bounds.x, o.x, n.x).floor() as i32;
    let y0 = map(old_bounds.y, o.y, n.y).floor() as i32;
    let x1 = map(old_bounds.right(), o.x, n.x).ceil() as i32;
    let y1 = map(old_bounds.bottom(), o.y, n.y).ceil() as i32;
    let r = Rect {
        x: x0,
        y: y0,
        w: x1.saturating_sub(x0),
        h: y1.saturating_sub(y0),
    };
    Some(if k == 1.0 { r } else { inflate(r, 2) })
}

/// Trả chuột khi nhả nút: lần kéo vừa xong (`release`), hoặc lớp phủ còn giữ chuột mà không còn
/// kéo gì (bảng màu bị đóng bằng Esc/Enter giữa lúc kéo ô sắc độ hay thanh độ sáng).
pub fn should_release(release: bool, has_capture: bool, dragging: bool) -> bool {
    release || (has_capture && !dragging)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon_overlap(a: Rect, b: Rect) -> bool {
        a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
    }

    #[test]
    fn toolbar_button_hit_testing() {
        // 96 DPI: Hoàn tác, Làm lại (36 mỗi ô), vạch ngăn 9, rồi sáu nút hành động.
        let bar = Rect {
            x: 100,
            y: 100,
            w: 8 * 36 + 9,
            h: 36,
        };
        let at = |x, y| bar_item_at(bar, Point { x, y }, 96);
        assert_eq!(at(110, 110), Some(BarItem::Undo));
        assert_eq!(at(150, 110), Some(BarItem::Redo));
        assert_eq!(at(172, 110), None);
        assert_eq!(at(180, 110), None);
        assert_eq!(at(190, 110), Some(BarItem::Act(Action::Upload)));
        assert_eq!(at(230, 110), Some(BarItem::Act(Action::Save)));
        assert_eq!(at(260, 110), Some(BarItem::Act(Action::Copy)));
        assert_eq!(at(300, 110), Some(BarItem::Act(Action::Pin)));
        assert_eq!(at(325, 110), Some(BarItem::Act(Action::Ocr)));
        assert_eq!(at(340, 110), Some(BarItem::Act(Action::Ocr)));
        assert_eq!(at(360, 110), Some(BarItem::Act(Action::Ocr)));
        assert_eq!(at(361, 110), Some(BarItem::Act(Action::Cancel)));
        assert_eq!(at(396, 110), Some(BarItem::Act(Action::Cancel)));
        assert_eq!(at(397, 110), None);
        assert_eq!(at(99, 110), None);
        assert_eq!(at(110, 136), None);
    }

    /// Thanh ngang: Hoàn tác, Làm lại, vạch ngăn, Tải lên, Lưu, Chép, Ghim, Nhận dạng chữ, Hủy; các ô
    /// liền nhau, không
    /// chồng, nằm trong thanh; `bar_item_at` đúng ở tâm ô và `None` trên vạch ngăn.
    #[test]
    fn bar_layout_order() {
        assert_eq!(
            BAR_CELLS,
            [
                Some(BarItem::Undo),
                Some(BarItem::Redo),
                None,
                Some(BarItem::Act(Action::Upload)),
                Some(BarItem::Act(Action::Save)),
                Some(BarItem::Act(Action::Copy)),
                Some(BarItem::Act(Action::Pin)),
                Some(BarItem::Act(Action::Ocr)),
                Some(BarItem::Act(Action::Cancel)),
            ]
        );
        for dpi in [96, 120, 144, 192] {
            let (b, sep) = (scaled(BUTTON, dpi), scaled(SIDE_SEP, dpi));
            let l = bar_layout(dpi);
            // 8 nút và 1 vạch ngăn.
            assert_eq!(l.size, (8 * b + sep, b), "dpi {dpi}");
            assert_eq!(l.cells.len(), 9);
            let items: Vec<Option<BarItem>> = l.cells.iter().map(|(i, _)| *i).collect();
            assert_eq!(items, BAR_CELLS.to_vec());
            let mut x = 0;
            for (item, r) in &l.cells {
                assert_eq!((r.x, r.y, r.h), (x, 0, b), "dpi {dpi} {item:?}");
                assert_eq!(r.w, if item.is_some() { b } else { sep }, "dpi {dpi}");
                x = r.right();
            }
            assert_eq!(x, l.size.0);
            let bar = Rect {
                x: 300,
                y: 200,
                w: l.size.0,
                h: l.size.1,
            };
            for (item, r) in &l.cells {
                let abs = Rect {
                    x: bar.x + r.x,
                    y: bar.y + r.y,
                    ..*r
                };
                match item {
                    Some(it) => {
                        let c = Point {
                            x: abs.x + abs.w / 2,
                            y: abs.y + abs.h / 2,
                        };
                        assert_eq!(bar_item_at(bar, c, dpi), Some(*it), "dpi {dpi}");
                        assert_eq!(bar_item_rect(bar, dpi, *it), Some(abs));
                    }
                    None => {
                        for dx in 0..abs.w {
                            for y in [abs.y, abs.y + abs.h / 2, abs.bottom() - 1] {
                                let p = Point { x: abs.x + dx, y };
                                assert_eq!(bar_item_at(bar, p, dpi), None, "dpi {dpi} {p:?}");
                            }
                        }
                    }
                }
            }
            // Thanh đặt bởi `bars_for` lấy đúng cỡ của bố cục.
            let mon = Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            };
            let sel = Rect {
                x: 600,
                y: 300,
                w: 500,
                h: 300,
            };
            let (placed, _) = bars_for(sel, mon, dpi);
            assert_eq!((placed.w, placed.h), l.size, "dpi {dpi}");
        }
    }

    /// Không phiên: đúng `BAR_CELLS` (9 ô) như cũ. Có phiên: thêm Thêm bước ngay trước Hủy (10 ô);
    /// bố cục và dò ô theo đúng danh sách ô.
    #[test]
    fn bar_cells_with_and_without_steps() {
        assert_eq!(bar_cells(false), BAR_CELLS.to_vec());
        let with = bar_cells(true);
        assert_eq!(with.len(), 10);
        assert_eq!(
            with,
            vec![
                Some(BarItem::Undo),
                Some(BarItem::Redo),
                None,
                Some(BarItem::Act(Action::Upload)),
                Some(BarItem::Act(Action::Save)),
                Some(BarItem::Act(Action::Copy)),
                Some(BarItem::Act(Action::Pin)),
                Some(BarItem::Act(Action::Ocr)),
                Some(BarItem::Act(Action::AddStep)),
                Some(BarItem::Act(Action::Cancel)),
            ]
        );
        for dpi in [96, 144, 192] {
            let (b, sep) = (scaled(BUTTON, dpi), scaled(SIDE_SEP, dpi));
            assert_eq!(bar_layout(dpi), bar_layout_with(dpi, false));
            let l = bar_layout_with(dpi, true);
            assert_eq!(l.size, (9 * b + sep, b), "dpi {dpi}");
            let items: Vec<Option<BarItem>> = l.cells.iter().map(|(i, _)| *i).collect();
            assert_eq!(items, with);
            let bar = Rect {
                x: 50,
                y: 40,
                w: l.size.0,
                h: l.size.1,
            };
            let step = bar_item_rect_with(bar, dpi, true, BarItem::Act(Action::AddStep)).unwrap();
            let cancel = bar_item_rect_with(bar, dpi, true, BarItem::Act(Action::Cancel)).unwrap();
            assert_eq!(step.right(), cancel.x);
            assert_eq!(cancel.right(), bar.right());
            let c = Point {
                x: step.x + step.w / 2,
                y: step.y + step.h / 2,
            };
            assert_eq!(
                bar_item_at_with(bar, c, dpi, true),
                Some(BarItem::Act(Action::AddStep))
            );
            // Không phiên: không có ô Thêm bước.
            assert_eq!(
                bar_item_rect_with(bar, dpi, false, BarItem::Act(Action::AddStep)),
                None
            );
            let mon = Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            };
            let sel = Rect {
                x: 600,
                y: 300,
                w: 500,
                h: 300,
            };
            let (placed, _) = bars_for_with(sel, mon, dpi, true);
            assert_eq!((placed.w, placed.h), l.size, "dpi {dpi}");
            let (plain, _) = bars_for(sel, mon, dpi);
            assert_eq!((plain.w, plain.h), bar_layout(dpi).size, "dpi {dpi}");
        }
    }

    #[test]
    fn blit_bounds_are_checked() {
        let r = |x, y, w, h| Rect { x, y, w, h };
        let full = 100 * 50 * 4;
        assert!(blit_in_bounds(r(0, 0, 100, 50), 100, 50, full));
        assert!(blit_in_bounds(r(10, 20, 90, 30), 100, 50, full));
        // vượt phải, vượt dưới
        assert!(!blit_in_bounds(r(10, 0, 91, 10), 100, 50, full));
        assert!(!blit_in_bounds(r(0, 45, 10, 6), 100, 50, full));
        // âm
        assert!(!blit_in_bounds(r(-1, 0, 10, 10), 100, 50, full));
        assert!(!blit_in_bounds(r(0, -1, 10, 10), 100, 50, full));
        assert!(!blit_in_bounds(r(0, 0, -5, 10), 100, 50, full));
        // rỗng
        assert!(!blit_in_bounds(r(0, 0, 0, 10), 100, 50, full));
        assert!(!blit_in_bounds(r(0, 0, 10, 0), 100, 50, full));
        // bộ đệm ngắn
        assert!(!blit_in_bounds(r(0, 0, 10, 10), 100, 50, full - 1));
        // tràn số khi cộng
        assert!(!blit_in_bounds(r(i32::MAX, 0, 10, 10), 100, 50, full));
    }

    #[test]
    fn side_items_order() {
        use crate::i18n::{t, Lang};
        assert_eq!(SIDE_ITEMS.len(), 9);
        assert_eq!(
            SIDE_ITEMS,
            [
                SideItem::Tool(Tool::Select),
                SideItem::Tool(Tool::Pen),
                SideItem::Shapes,
                SideItem::Tool(Tool::Marker),
                SideItem::Tool(Tool::Blur),
                SideItem::Tool(Tool::Text),
                SideItem::Tool(Tool::Step),
                SideItem::Size,
                SideItem::Color,
            ]
        );
        assert_eq!(SIDE_SEPS, [1, 7]);
        assert_eq!(
            SHAPE_TOOLS,
            [Tool::Line, Tool::Arrow, Tool::Rect, Tool::Ellipse]
        );
        let tips: Vec<Msg> = SIDE_ITEMS.iter().map(|i| i.tip()).collect();
        assert_eq!(
            tips,
            vec![
                Msg::ToolSelect,
                Msg::ToolPen,
                Msg::ToolShapes,
                Msg::ToolMarker,
                Msg::ToolBlur,
                Msg::ToolText,
                Msg::ToolStep,
                Msg::ToolSize,
                Msg::ToolColor,
            ]
        );
        assert_eq!(t(Lang::Vi, SideItem::Shapes.tip()), "Hình (L, A, R, E)");
        assert_eq!(t(Lang::En, SideItem::Shapes.tip()), "Shapes (L, A, R, E)");
        assert_eq!(side_index(SideItem::Shapes), 2);
        assert_eq!(side_index(SideItem::Size), 7);
        assert_eq!(side_index(SideItem::Color), 8);
        // Gợi ý của Hoàn tác/Làm lại (nay trên thanh ngang) và của bốn ô bảng Hình.
        assert_eq!(BarItem::Undo.tip(), Msg::ToolUndo);
        assert_eq!(BarItem::Redo.tip(), Msg::ToolRedo);
        assert_eq!(t(Lang::Vi, BarItem::Redo.tip()), "Làm lại (Ctrl+Y)");
        assert_eq!(BarItem::Act(Action::Save).tip(), Msg::OverlaySave);
        assert_eq!(BarItem::Act(Action::Pin).tip(), Msg::OverlayPin);
        assert_eq!(BarItem::Act(Action::Ocr).tip(), Msg::OverlayOcr);
        let tips: Vec<Msg> = SHAPE_TOOLS.iter().map(|t| shape_tip(*t)).collect();
        assert_eq!(
            tips,
            vec![
                Msg::ToolLine,
                Msg::ToolArrow,
                Msg::ToolRect,
                Msg::ToolEllipse
            ]
        );
        let tips: Vec<Msg> = SizeLevel::ALL.iter().map(|l| size_tip(*l)).collect();
        assert_eq!(
            tips,
            vec![
                Msg::SizeSmall,
                Msg::SizeMedium,
                Msg::SizeLarge,
                Msg::SizeHuge
            ]
        );
    }

    /// Chiều cao một cột của thanh dọc ở 96 DPI: 9 nút và 2 vạch ngăn, trong giới hạn của gói.
    #[test]
    fn side_bar_column_height() {
        let hd = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        let l = side_layout_for(hd, 96);
        assert_eq!(l.size.0, 36);
        assert_eq!(l.size.1, 9 * 36 + 2 * 9);
        assert!(l.size.1 <= 9 * 36 + 8 * 6 + 2 * 9);
    }

    #[test]
    fn side_bar_button_hit_testing() {
        let mon = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        for dpi in [96, 192] {
            let l = side_layout_for(mon, dpi);
            let side = Rect {
                x: 500,
                y: 100,
                w: l.size.0,
                h: l.size.1,
            };
            let b = scaled(BUTTON, dpi);
            for (i, item) in SIDE_ITEMS.iter().enumerate() {
                let r = side_item_rect(side, &l, i).unwrap();
                assert_eq!((r.w, r.h), (b, b));
                let c = Point {
                    x: r.x + r.w / 2,
                    y: r.y + r.h / 2,
                };
                assert_eq!(side_item_at(side, &l, c), Some(*item), "dpi {dpi} mục {i}");
            }
            assert_eq!(side_item_rect(side, &l, SIDE_ITEMS.len()), None);
            // Vạch ngăn và lề của nó không trúng mục nào.
            for s in &l.seps {
                for dy in 0..s.h {
                    let p = Point {
                        x: side.x + s.x + s.w / 2,
                        y: side.y + s.y + dy,
                    };
                    assert_eq!(side_item_at(side, &l, p), None, "dpi {dpi} {p:?}");
                }
            }
            // Ngoài thanh.
            for p in [
                Point { x: 499, y: 120 },
                Point {
                    x: side.right(),
                    y: 120,
                },
                Point { x: 510, y: 99 },
                Point {
                    x: 510,
                    y: side.bottom(),
                },
            ] {
                assert_eq!(side_item_at(side, &l, p), None);
            }
        }
        // Một cột ở 96 DPI: vạch trước Pen và trước Cỡ.
        let l = side_layout_for(mon, 96);
        let side = Rect {
            x: 500,
            y: 100,
            w: l.size.0,
            h: l.size.1,
        };
        let at = |y| side_item_at(side, &l, Point { x: 518, y });
        assert_eq!(at(118), Some(SideItem::Tool(Tool::Select)));
        assert_eq!(at(100 + 36 + 9 + 18), Some(SideItem::Tool(Tool::Pen)));
        assert_eq!(at(100 + 36), None);
        assert_eq!(at(100 + 2 * 36 + 9 + 18), Some(SideItem::Shapes));
        assert_eq!(at(100 + 7 * 36 + 9), None);
        assert_eq!(at(100 + 7 * 36 + 9 + 9 + 18), Some(SideItem::Size));
        assert_eq!(at(100 + 8 * 36 + 18 + 18), Some(SideItem::Color));
        assert_eq!(at(100 + 9 * 36 + 18), None);
    }

    /// Thanh dọc 9 nút: 1920×1080 ở 100% và 200% một cột; 1366×768 ở 250% cần hai cột. Thanh nằm
    /// trọn trong màn hình và không chồng thanh ngang. Màn hình thấp 1366×768 ở 100%, 150% và 250%
    /// vẫn vừa.
    #[test]
    fn side_bar_fits_low_screen() {
        let inside = |r: Rect, mon: Rect| {
            r.x >= mon.x && r.y >= mon.y && r.right() <= mon.right() && r.bottom() <= mon.bottom()
        };
        let hd = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        let one = side_layout_for(hd, 96);
        assert_eq!(one.size.0, scaled(BUTTON, 96));
        let tall = side_layout_for(hd, 192);
        assert_eq!(tall.size.0, scaled(BUTTON, 192));
        assert!(tall.size.1 <= hd.h - 2 * scaled(GAP, 192));
        let low = Rect {
            x: 0,
            y: 0,
            w: 1366,
            h: 768,
        };
        let two = side_layout_for(low, 240);
        assert_eq!(two.size.0, 2 * scaled(BUTTON, 240));
        assert!(two.size.1 <= low.h - 2 * scaled(GAP, 240));
        for dpi in [96, 192] {
            let l = side_layout_for(hd, dpi);
            for sel in [
                // Giữa màn hình.
                Rect {
                    x: 660,
                    y: 340,
                    w: 600,
                    h: 400,
                },
                // Sát góc trên trái.
                Rect {
                    x: 0,
                    y: 0,
                    w: 300,
                    h: 200,
                },
                Rect {
                    x: 0,
                    y: 0,
                    w: 120,
                    h: 600,
                },
                hd,
            ] {
                let (bar, side) = bars_for(sel, hd, dpi);
                assert_eq!((side.w, side.h), l.size, "dpi {dpi} sel {sel:?}");
                assert!(inside(side, hd), "dpi {dpi} sel {sel:?}: {side:?}");
                assert!(inside(bar, hd), "dpi {dpi} sel {sel:?}: {bar:?}");
                assert!(
                    !mon_overlap(bar, side),
                    "dpi {dpi} sel {sel:?}: {bar:?} {side:?}"
                );
            }
        }
        let mon = Rect {
            x: 0,
            y: 0,
            w: 1366,
            h: 768,
        };
        for dpi in [96, 144, 240] {
            let l = side_layout_for(mon, dpi);
            assert!(l.size.1 <= mon.h, "dpi {dpi}: thanh cao {}", l.size.1);
            for sel in [
                mon,
                Rect {
                    x: 100,
                    y: 100,
                    w: 300,
                    h: 200,
                },
                Rect {
                    x: 1000,
                    y: 500,
                    w: 366,
                    h: 268,
                },
                Rect {
                    x: 0,
                    y: 0,
                    w: 400,
                    h: 120,
                },
                Rect {
                    x: 500,
                    y: 600,
                    w: 400,
                    h: 168,
                },
            ] {
                let (bar, side) = bars_for(sel, mon, dpi);
                assert_eq!((side.w, side.h), l.size);
                assert!(inside(side, mon), "dpi {dpi} sel {sel:?}: {side:?}");
                assert!(inside(bar, mon), "dpi {dpi} sel {sel:?}: {bar:?}");
                assert!(
                    !mon_overlap(bar, side),
                    "dpi {dpi} sel {sel:?}: {bar:?} {side:?}"
                );
            }
        }
    }

    /// Hai trường hợp rà soát Task 42 tìm ra: vùng chọn hẹp sát góc trên trái, thanh dọc từng che
    /// nút Màu của thanh bốn nút.
    #[test]
    fn side_bar_never_covers_action_bar_near_top_left() {
        let mon = Rect {
            x: 0,
            y: 0,
            w: 1366,
            h: 768,
        };
        for (dpi, sel) in [
            (
                144,
                Rect {
                    x: 0,
                    y: 0,
                    w: 100,
                    h: 380,
                },
            ),
            (
                96,
                Rect {
                    x: 0,
                    y: 0,
                    w: 100,
                    h: 300,
                },
            ),
        ] {
            let (bar, side) = bars_for(sel, mon, dpi);
            let disjoint = bar.right() <= side.x
                || side.right() <= bar.x
                || bar.bottom() <= side.y
                || side.bottom() <= bar.y;
            assert!(disjoint, "dpi {dpi}: {bar:?} {side:?}");
            assert!(side.x >= 0 && side.y >= 0 && side.right() <= 1366 && side.bottom() <= 768);
            // Nút Màu và nút Cỡ không bị che.
            let l = side_layout_for(mon, dpi);
            for item in [SideItem::Color, SideItem::Size] {
                let r = side_item_rect(side, &l, side_index(item)).unwrap();
                assert!(!mon_overlap(r, bar));
            }
        }
    }

    /// Bảng Hình: bốn ô vuông ngang, lề GAP, theo thứ tự `SHAPE_TOOLS`; đặt cạnh nút Hình, trong
    /// màn hình, không đè thanh dọc.
    #[test]
    fn shapes_flyout_geometry() {
        let mon = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        for dpi in [96, 144, 192] {
            let (b, gap) = (scaled(BUTTON, dpi), scaled(GAP, dpi));
            for sel in [
                Rect {
                    x: 660,
                    y: 340,
                    w: 600,
                    h: 400,
                },
                Rect {
                    x: 1500,
                    y: 600,
                    w: 420,
                    h: 480,
                },
                Rect {
                    x: 0,
                    y: 0,
                    w: 300,
                    h: 200,
                },
                mon,
            ] {
                let (_, side) = bars_for(sel, mon, dpi);
                let l = side_layout_for(mon, dpi);
                let btn = side_item_rect(side, &l, side_index(SideItem::Shapes)).unwrap();
                let fly = shapes_flyout_rect(side, btn, sel, mon, dpi);
                assert_eq!(fly, shapes_rect_for(side, sel, mon, dpi));
                assert_eq!((fly.w, fly.h), (4 * b + 2 * gap, b + 2 * gap));
                assert!(fly.x >= 0 && fly.y >= 0 && fly.right() <= 1920 && fly.bottom() <= 1080);
                assert!(!mon_overlap(fly, side) || sel == mon, "{fly:?} {side:?}");
                for (i, tool) in SHAPE_TOOLS.iter().enumerate() {
                    let c = shape_cell_rect(fly, dpi, i);
                    assert_eq!((c.w, c.h), (b, b));
                    assert!(c.x >= fly.x + gap && c.right() <= fly.right() - gap);
                    let p = Point {
                        x: c.x + b / 2,
                        y: c.y + b / 2,
                    };
                    assert_eq!(shape_cell_at(fly, dpi, p), Some(*tool));
                }
                let edge = Point {
                    x: fly.x + 1,
                    y: fly.y + 1,
                };
                assert_eq!(shape_cell_at(fly, dpi, edge), None);
                let right = Point {
                    x: fly.right(),
                    y: fly.y + gap + 1,
                };
                assert_eq!(shape_cell_at(fly, dpi, right), None);
            }
        }
    }

    /// Bảng Cỡ: bốn ô vuông ngang, lề GAP; mục dưới điểm; đặt cạnh nút Cỡ trong màn hình.
    #[test]
    fn size_flyout_geometry() {
        let mon = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        for dpi in [96, 144, 192] {
            let (b, gap) = (scaled(BUTTON, dpi), scaled(GAP, dpi));
            for sel in [
                Rect {
                    x: 660,
                    y: 340,
                    w: 600,
                    h: 400,
                },
                Rect {
                    x: 1500,
                    y: 600,
                    w: 420,
                    h: 480,
                },
                mon,
            ] {
                let (_, side) = bars_for(sel, mon, dpi);
                let l = side_layout_for(mon, dpi);
                let btn = side_item_rect(side, &l, side_index(SideItem::Size)).unwrap();
                let fly = size_flyout_rect(side, btn, sel, mon, dpi);
                assert_eq!((fly.w, fly.h), (4 * b + 2 * gap, b + 2 * gap));
                assert!(fly.x >= 0 && fly.y >= 0 && fly.right() <= 1920 && fly.bottom() <= 1080);
                assert!(!mon_overlap(fly, side) || sel == mon, "{fly:?} {side:?}");
                for (i, level) in SizeLevel::ALL.iter().enumerate() {
                    let c = size_cell_rect(fly, dpi, i);
                    assert_eq!((c.w, c.h), (b, b));
                    let p = Point {
                        x: c.x + b / 2,
                        y: c.y + b / 2,
                    };
                    assert_eq!(size_cell_at(fly, dpi, p), Some(*level));
                }
                assert_eq!(
                    size_cell_at(
                        fly,
                        dpi,
                        Point {
                            x: fly.x + 1,
                            y: fly.y + 1
                        }
                    ),
                    None
                );
                assert_eq!(
                    size_cell_at(
                        fly,
                        dpi,
                        Point {
                            x: fly.right(),
                            y: fly.y + gap + 1
                        }
                    ),
                    None
                );
            }
        }
    }

    /// Thuộc tính: trên lưới cỡ và vị trí vùng chọn ở màn hình 1366×768 (100% và 150%), hai thanh
    /// không bao giờ giao nhau và cả hai nằm trọn trong màn hình.
    #[test]
    fn bars_never_overlap_on_low_screen_grid() {
        let mon = Rect {
            x: 0,
            y: 0,
            w: 1366,
            h: 768,
        };
        let inside = |r: Rect| r.x >= 0 && r.y >= 0 && r.right() <= mon.w && r.bottom() <= mon.h;
        let sizes = [1, 20, 60, 100, 150, 216, 300, 450, 700, 1000, 1366];
        let heights = [1, 20, 60, 100, 200, 300, 380, 500, 650, 768];
        for dpi in [96, 144] {
            for &w in &sizes {
                for &h in &heights {
                    for x in (0..=mon.w - w).step_by(37).chain([mon.w - w]) {
                        for y in (0..=mon.h - h).step_by(29).chain([mon.h - h]) {
                            let sel = Rect { x, y, w, h };
                            let (bar, side) = bars_for(sel, mon, dpi);
                            assert!(inside(bar), "dpi {dpi} sel {sel:?}: bar {bar:?}");
                            assert!(inside(side), "dpi {dpi} sel {sel:?}: side {side:?}");
                            assert!(
                                !mon_overlap(bar, side),
                                "dpi {dpi} sel {sel:?}: {bar:?} {side:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn capture_released_when_nothing_drags() {
        assert!(should_release(true, true, false));
        assert!(should_release(true, false, false));
        // Bảng đóng giữa lúc kéo: còn giữ chuột, không kéo gì -> trả.
        assert!(should_release(false, true, false));
        // Còn đang kéo (nét, vùng) -> giữ.
        assert!(!should_release(false, true, true));
        assert!(!should_release(false, false, false));
    }
}
