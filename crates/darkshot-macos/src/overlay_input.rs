//! Phần thuần của lớp phủ chọn vùng macOS (Task 105): đổi điểm của sự kiện chuột sang pixel,
//! khớp ảnh chụp với `NSScreen`, máy trạng thái "màn hình đang chọn" bọc `Selection` của core,
//! thanh Lưu/Chép/Hủy, bảng phím, nhãn kích thước, lưới an toàn khi không có tương tác.
//!
//! Không gọi Objective-C: kiểm thử chạy được trên máy không có GUI. Mọi toạ độ của mô hình là
//! pixel của ảnh màn hình đang xét, gốc trên trái (như core).

use crate::annotate_ui::{Annot, CursorKind, Down, Layout, Prefs};
use crate::coords::{cocoa_to_top_left, pt_to_px, RectF};
use darkshot_core::actions::Action;
use darkshot_core::annotate::{press_mode, PressMode, Shape};
use darkshot_core::geometry::{place_side_toolbar, place_toolbar, Hit, Point, Rect, Selection};
use darkshot_core::i18n::Msg;
use darkshot_core::image::Image;
use darkshot_core::overlay_layout::{
    clamp_x, scaled, side_layout_for, BarItem, BarLayout, BUTTON, GAP, HANDLE_HIT, SIDE_SEP,
};
use darkshot_core::snap;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Lưới an toàn: không có tương tác (chuột, phím) trong chừng này thì lớp phủ tự hủy, để một lỗi
/// sự kiện không bao giờ khóa màn hình vô thời hạn.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(120);
/// Chu kỳ kiểm lưới an toàn.
pub const IDLE_CHECK_EVERY: Duration = Duration::from_secs(5);
/// `kVK_Escape`.
pub const KEY_ESCAPE: u16 = 53;
/// `kVK_Delete` (phím Xóa lùi) và `kVK_ForwardDelete`.
pub const KEY_DELETE: u16 = 51;
pub const KEY_FORWARD_DELETE: u16 = 117;
/// Các nút hành động của thanh ngang, trái sang phải.
///
/// Thứ tự như `bar_cells`/`BAR_CELLS` của core (Tải lên, Lưu, Chép, Hủy; Mac chưa có Ghim, Nhận
/// dạng chữ). Nút Tải lên luôn hiện, như Windows: chưa cấu hình R2 thì bấm vào sẽ lưu ảnh và mở
/// Tùy chọn tab R2 (`upload::route`), không ẩn nút. Đứng sau Hoàn tác, Làm lại và một vạch ngăn.
pub const MAC_BAR: [Action; 4] = [Action::Upload, Action::Save, Action::Copy, Action::Cancel];
/// Khoảng chừa thêm (điểm ở 96 DPI) dưới mép trên cho viên nhắc, để không nằm dưới tai thỏ.
pub const HINT_NOTCH_CLEAR: i32 = 40;
/// Đệm ngang và dọc (96 DPI) của nhãn kích thước, như bản Windows.
const LABEL_PAD_X: i32 = 7;
const LABEL_PAD_Y: i32 = 2;
/// Dung sai (điểm) khi khớp khung ảnh chụp với khung `NSScreen`.
const FRAME_EPS: f64 = 0.5;

/// Phím bổ trợ của một sự kiện phím.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub cmd: bool,
    pub ctrl: bool,
    pub opt: bool,
    pub shift: bool,
}

/// DPI tương đương của hệ số `scale` (96 ở 1x, 192 ở Retina 2x) để dùng hằng bố cục của core.
pub fn dpi_of(scale: f64) -> u32 {
    if !(scale.is_finite() && scale > 0.0) {
        return 96;
    }
    // `as` bão hòa, không panic.
    ((96.0 * scale).round() as u32).max(1)
}

/// Điểm toàn cục Cocoa (gốc dưới trái màn hình chính, Y lên) của một sự kiện sang pixel của màn
/// hình có khung Cocoa `screen` và hệ số `scale` (gốc trên trái màn hình đó).
pub fn event_to_px(global: (f64, f64), screen: RectF, scale: f64) -> Point {
    let x = global.0 - screen.x;
    // Cạnh trên của màn hình trong Cocoa là `y + h`; Y lật về hướng xuống.
    let y = (screen.y + screen.h) - global.1;
    Point {
        x: pt_to_px(x, scale),
        y: pt_to_px(y, scale),
    }
}

/// Bán kính bắt tay nắm (pixel) ở hệ số `scale`.
pub fn handle_radius(scale: f64) -> i32 {
    pt_to_px(f64::from(HANDLE_HIT), scale).max(1)
}

/// Ghép ảnh chụp với `NSScreen`. `shots` là (khung gốc trên trái theo điểm, số pixel của ảnh);
/// `screens` là khung Cocoa theo thứ tự `NSScreen.screens` (phần tử đầu là màn hình chính).
/// Trả các cặp (chỉ số ảnh, chỉ số màn hình), mỗi màn hình đúng một cặp, theo thứ tự ảnh.
///
/// Khi phản chiếu màn hình (Mirror Displays, AirPlay, Sidecar gương) ScreenCaptureKit có thể liệt
/// kê nhiều display hơn `NSScreen`, có display cùng khung: mỗi màn hình giữ ảnh khớp có nhiều
/// pixel nhất (bằng nhau thì ảnh đầu), ảnh dư hay ảnh không khớp màn hình nào bị bỏ. Một màn hình
/// không có ảnh nào thì `None` (cấu hình màn hình đã đổi sau khi chụp).
pub fn match_screens(shots: &[(RectF, u64)], screens: &[RectF]) -> Option<Vec<(usize, usize)>> {
    let primary_h = screens.first()?.h;
    let same = |a: &RectF, b: &RectF| {
        (a.x - b.x).abs() <= FRAME_EPS
            && (a.y - b.y).abs() <= FRAME_EPS
            && (a.w - b.w).abs() <= FRAME_EPS
            && (a.h - b.h).abs() <= FRAME_EPS
    };
    let mut pairs = Vec::with_capacity(screens.len());
    for (si, screen) in screens.iter().enumerate() {
        let tl = cocoa_to_top_left(*screen, primary_h);
        let best = shots
            .iter()
            .enumerate()
            .filter(|(_, (frame, _))| same(frame, &tl))
            // `max_by_key` giữ phần tử cuối khi bằng nhau; duyệt ngược để giữ ảnh đầu.
            .rev()
            .max_by_key(|(_, (_, px))| *px)
            .map(|(i, _)| i)?;
        pairs.push((best, si));
    }
    pairs.sort_unstable();
    // Hai màn hình không thể cùng một ảnh (khung màn hình khác nhau, trừ khi chính `NSScreen`
    // trùng khung): coi như cấu hình lạ.
    if pairs.windows(2).any(|w| w[0].0 == w[1].0) {
        return None;
    }
    Some(pairs)
}

/// Quyết định sau khi một cửa sổ lớp phủ mất key (kiểm ở lượt kế của hàng đợi chính): `keys` là
/// trạng thái key của mọi cửa sổ của phiên. Bấm sang màn hình khác chỉ chuyển key giữa các cửa sổ
/// của cùng phiên (không hủy); không cửa sổ nào còn key (ứng dụng khác lấy bàn phím) thì hủy.
pub fn cancel_after_key_loss(keys: &[bool]) -> bool {
    !keys.iter().any(|k| *k)
}

/// Chữ của nhãn kích thước (pixel thật của ảnh sẽ lưu).
pub fn size_label(r: Rect) -> String {
    format!("{} \u{d7} {}", r.w, r.h)
}

/// Ô nhãn kích thước cỡ chữ `text` (pixel): trên vùng chọn nếu đủ chỗ, không thì trong góc trên
/// trái vùng chọn; kẹp ngang vào màn hình.
pub fn label_rect(sel: Rect, text: (i32, i32), dpi: u32, mon: Rect) -> Rect {
    let w = text.0.max(0).saturating_add(2 * scaled(LABEL_PAD_X, dpi));
    let h = text.1.max(0).saturating_add(2 * scaled(LABEL_PAD_Y, dpi));
    let gap = scaled(GAP, dpi);
    let above = sel.y.saturating_sub(gap).saturating_sub(h);
    let (x, y) = if above >= mon.y {
        (sel.x, above)
    } else {
        (sel.x.saturating_add(gap), sel.y.saturating_add(gap))
    };
    clamp_x(Rect { x, y, w, h }, mon)
}

/// Viên nhắc "Kéo để chọn vùng · Esc để thoát" cỡ chữ `text`: giữa mép trên, chừa tai thỏ.
pub fn hint_rect(mon: Rect, text: (i32, i32), dpi: u32) -> Rect {
    let mut r = snap::hint_rect(mon, text, dpi);
    r.y = r.y.saturating_add(scaled(HINT_NOTCH_CLEAR, dpi));
    r
}

/// Cạnh một nút của thanh ngang (pixel).
pub fn bar_button_size(dpi: u32) -> i32 {
    scaled(BUTTON, dpi)
}

/// Các ô của thanh ngang Mac: Hoàn tác, Làm lại | các nút hành động `MAC_BAR`; `None` là vạch ngăn.
pub fn bar_cells() -> Vec<Option<BarItem>> {
    let mut v = vec![Some(BarItem::Undo), Some(BarItem::Redo), None];
    v.extend(MAC_BAR.iter().map(|a| Some(BarItem::Act(*a))));
    v
}

/// Bố cục thanh ngang ở `dpi`: nút vuông `BUTTON` xếp liền, vạch ngăn rộng `SIDE_SEP`.
pub fn bar_layout(dpi: u32) -> BarLayout {
    let (b, sep) = (bar_button_size(dpi), scaled(SIDE_SEP, dpi));
    let mut x = 0i32;
    let cells = bar_cells()
        .into_iter()
        .map(|item| {
            let w = if item.is_some() { b } else { sep };
            let r = Rect { x, y: 0, w, h: b };
            x = x.saturating_add(w);
            (item, r)
        })
        .collect();
    BarLayout {
        size: (x, b),
        cells,
    }
}

/// Thanh ngang cạnh vùng chọn `sel`, luôn nằm trong màn hình `mon`.
pub fn bar_rect(sel: Rect, dpi: u32, mon: Rect) -> Rect {
    place_toolbar(sel, bar_layout(dpi).size, scaled(GAP, dpi), mon)
}

/// Thanh dọc cạnh vùng chọn `sel`, tránh thanh ngang `bar`.
pub fn side_rect(sel: Rect, bar: Rect, dpi: u32, mon: Rect) -> Rect {
    let size = side_layout_for(mon, dpi).size;
    place_side_toolbar(sel, size, scaled(GAP, dpi), mon, Some(bar))
}

/// Ô của mục `item` trên thanh `bar`.
pub fn bar_button_rect(bar: Rect, dpi: u32, item: BarItem) -> Option<Rect> {
    bar_layout(dpi)
        .cells
        .iter()
        .find(|(i, _)| *i == Some(item))
        .map(|(_, r)| Rect {
            x: bar.x.saturating_add(r.x),
            y: bar.y.saturating_add(r.y),
            ..*r
        })
}

/// Mục của thanh `bar` dưới điểm `p`; vạch ngăn và ngoài thanh trả `None`.
pub fn bar_item_at(bar: Rect, p: Point, dpi: u32) -> Option<BarItem> {
    if bar.w <= 0 || !bar.contains(p) {
        return None;
    }
    bar_layout(dpi)
        .cells
        .iter()
        .find(|(_, r)| {
            Rect {
                x: bar.x.saturating_add(r.x),
                y: bar.y.saturating_add(r.y),
                ..*r
            }
            .contains(p)
        })
        .and_then(|(i, _)| *i)
}

/// Gợi ý của một mục thanh ngang (phím tắt kiểu Mac).
pub fn bar_tip(item: BarItem) -> Msg {
    match item {
        BarItem::Undo => Msg::ToolUndoMac,
        BarItem::Redo => Msg::ToolRedoMac,
        BarItem::Act(Action::Upload) => Msg::OverlayUploadMac,
        BarItem::Act(Action::Save) => Msg::OverlaySaveMac,
        BarItem::Act(Action::Copy) => Msg::OverlayCopyMac,
        BarItem::Act(_) => Msg::OverlayCancelMac,
    }
}

/// Ô gợi ý của nút đang trỏ: dưới thanh nếu đủ chỗ, không thì trên; trong màn hình.
pub fn tip_rect(bar: Rect, text: (i32, i32), dpi: u32, mon: Rect) -> Rect {
    let w = text.0.max(0).saturating_add(2 * scaled(LABEL_PAD_X, dpi));
    let h = text.1.max(0).saturating_add(2 * scaled(LABEL_PAD_Y, dpi));
    place_toolbar(bar, (w.min(mon.w), h.min(mon.h)), scaled(GAP, dpi), mon)
}

/// Hành động của một phím: `Esc` luôn hủy; `⌘D` tải lên, `⌘S` lưu, `⌘C` chép khi có vùng chọn và không đang
/// kéo (như `Ctrl+S`/`Ctrl+C` của bản Windows). `chars`: `charactersIgnoringModifiers`.
pub fn key_action(
    key_code: u16,
    chars: Option<&str>,
    mods: Mods,
    has_sel: bool,
    dragging: bool,
) -> Option<Action> {
    if key_code == KEY_ESCAPE || chars == Some("\u{1b}") {
        return Some(Action::Cancel);
    }
    if !mods.cmd || mods.ctrl || mods.opt || !has_sel || dragging {
        return None;
    }
    // Theo chữ của phím (bố cục bàn phím), không theo vị trí phím.
    let c = chars?.chars().next()?.to_ascii_lowercase();
    match c {
        'd' => Some(Action::Upload),
        's' => Some(Action::Save),
        'c' => Some(Action::Copy),
        _ => None,
    }
}

/// Kích thước pixel và hệ số của một màn hình trong lớp phủ.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenPx {
    pub w: i32,
    pub h: i32,
    pub scale: f64,
}

/// Những gì một màn hình cần vẽ; so sánh trước/sau một sự kiện để biết màn hình nào cần vẽ lại.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewState {
    /// Vùng chọn trên màn hình này (pixel).
    pub rect: Option<Rect>,
    /// Thanh ngang (khi có vùng chọn và không đang kéo).
    pub bar: Option<Rect>,
    /// Thanh dọc công cụ (cùng điều kiện).
    pub side: Option<Rect>,
    pub hover: Option<BarItem>,
    pub pressed: Option<BarItem>,
    /// Viên nhắc (khi chưa có vùng chọn ở đâu cả).
    pub hint: bool,
}

impl ViewState {
    /// Không vẽ gì ngoài ảnh mờ (màn hình không tồn tại).
    pub fn default_hidden() -> ViewState {
        ViewState {
            rect: None,
            bar: None,
            side: None,
            hover: None,
            pressed: None,
            hint: false,
        }
    }

    /// Cùng vùng chọn, thanh và viên nhắc (chỉ khác nút đang trỏ/nhấn): vẽ lại cục bộ là đủ.
    pub fn same_layout(&self, other: &ViewState) -> bool {
        self.rect == other.rect
            && self.bar == other.bar
            && self.side == other.side
            && self.hint == other.hint
    }
}

/// Kết quả của một phím: hành động (hủy, lưu...), có chuyển cho bộ gõ (`interpretKeyEvents:`) và
/// có cần dán chữ từ clipboard vào ô chữ không.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyOutcome {
    pub action: Option<Action>,
    pub ime: bool,
    pub paste: bool,
}

impl KeyOutcome {
    fn act(a: Option<Action>) -> KeyOutcome {
        KeyOutcome {
            action: a,
            ..KeyOutcome::default()
        }
    }
}

/// Trạng thái lớp phủ trên mọi màn hình. Chỉ một màn hình "đang chọn" giữ `Selection`; bấm sang
/// màn hình khác thì bỏ vùng cũ (vùng chọn không vắt qua màn hình, quyết định 135). Công cụ đánh
/// dấu (`Annot`) gắn với màn hình đang chọn.
pub struct Model {
    screens: Vec<ScreenPx>,
    /// Ảnh gốc của từng màn hình (cùng thứ tự `screens`); chia sẻ với bên vẽ, trả lại khi đóng.
    images: Vec<Rc<Image>>,
    active: Option<usize>,
    sel: Selection,
    annot: Annot,
    pressed: Option<BarItem>,
    hover: Option<BarItem>,
    shift: bool,
    /// Esc đã chuyển cho bộ gõ khi đang soạn dở; lần Esc kế tiếp mà vẫn còn soạn thì ép đóng.
    esc_marked: bool,
    last_input: Instant,
}

const EMPTY: Rect = Rect {
    x: 0,
    y: 0,
    w: 0,
    h: 0,
};

/// Ảnh giữ chỗ (không có điểm ảnh) khi chưa có ảnh thật: `redact_color` trả `None` trên ảnh không
/// hợp lệ nên vùng Làm mờ rơi về màu xám dự phòng.
#[cfg(test)]
fn placeholder(s: &ScreenPx) -> Rc<Image> {
    Rc::new(Image {
        width: u32::try_from(s.w).unwrap_or(0),
        height: u32::try_from(s.h).unwrap_or(0),
        bgra: Vec::new(),
    })
}

impl Model {
    #[cfg(test)]
    pub fn new(screens: Vec<ScreenPx>, now: Instant) -> Model {
        let images = screens.iter().map(placeholder).collect();
        Model::with_images(screens, images, Prefs::default(), now)
    }

    /// Mô hình với ảnh gốc thật của từng màn hình và lựa chọn công cụ nhớ từ lần trước.
    pub fn with_images(
        screens: Vec<ScreenPx>,
        images: Vec<Rc<Image>>,
        prefs: Prefs,
        now: Instant,
    ) -> Model {
        Model {
            screens,
            images,
            active: None,
            sel: Selection::new(EMPTY),
            annot: Annot::with_prefs(prefs),
            pressed: None,
            hover: None,
            shift: false,
            esc_marked: false,
            last_input: now,
        }
    }

    pub fn active(&self) -> Option<usize> {
        self.active
    }

    /// Vùng chọn trên màn hình đang chọn (pixel).
    pub fn rect(&self) -> Option<Rect> {
        self.active.and(self.sel.rect())
    }

    pub fn is_dragging(&self) -> bool {
        self.sel.is_dragging()
    }

    pub fn annot(&self) -> &Annot {
        &self.annot
    }

    pub fn annot_mut(&mut self) -> &mut Annot {
        &mut self.annot
    }

    /// Khung pixel của màn hình `i` (gốc 0, 0).
    pub fn bounds(&self, i: usize) -> Option<Rect> {
        self.screens.get(i).map(|s| Rect {
            x: 0,
            y: 0,
            w: s.w,
            h: s.h,
        })
    }

    pub fn dpi(&self, i: usize) -> u32 {
        self.screens.get(i).map_or(96, |s| dpi_of(s.scale))
    }

    /// Thanh ngang trên màn hình đang chọn.
    pub fn bar(&self) -> Option<(usize, Rect)> {
        if self.is_dragging() {
            return None;
        }
        let i = self.active?;
        let sel = self.rect()?;
        let mon = self.bounds(i)?;
        Some((i, bar_rect(sel, self.dpi(i), mon)))
    }

    /// Thanh dọc công cụ trên màn hình đang chọn (tránh thanh ngang).
    pub fn side(&self) -> Option<(usize, Rect)> {
        let (i, bar) = self.bar()?;
        let sel = self.rect()?;
        let mon = self.bounds(i)?;
        Some((i, side_rect(sel, bar, self.dpi(i), mon)))
    }

    /// Bố cục của ngữ cảnh một sự kiện trên màn hình đang chọn `screen`: vùng chọn, màn hình, DPI,
    /// thanh ngang và thanh dọc (hai thanh `None` khi đang kéo vùng chọn).
    pub fn layout(&self, screen: usize) -> Option<Layout> {
        if self.active != Some(screen) {
            return None;
        }
        let sel = self.rect()?;
        let mon = self.bounds(screen)?;
        Some(Layout {
            sel,
            mon,
            dpi: self.dpi(screen),
            bar: self.bar().map(|(_, b)| b),
            side: self.side().map(|(_, s)| s),
        })
    }

    /// Vùng thanh dọc, thanh ngang và bảng cần vẽ lại khi bố cục của chúng đổi (rê chuột, chọn
    /// công cụ, mở bảng).
    pub fn chrome_rects(&self, screen: usize) -> Vec<Rect> {
        let (Some(lay), Some(img)) = (self.layout(screen), self.images.get(screen)) else {
            return Vec::new();
        };
        self.annot.chrome_rects(&lay.env(img))
    }

    pub fn view_state(&self, i: usize) -> ViewState {
        if i >= self.screens.len() {
            return ViewState::default_hidden();
        }
        let mine = self.active == Some(i);
        ViewState {
            rect: if mine { self.rect() } else { None },
            bar: self.bar().filter(|(s, _)| *s == i).map(|(_, b)| b),
            side: self.side().filter(|(s, _)| *s == i).map(|(_, b)| b),
            hover: if mine { self.hover } else { None },
            pressed: if mine { self.pressed } else { None },
            hint: self.rect().is_none(),
        }
    }

    /// Nhấn chuột trái ở `p` (pixel của màn hình `screen`).
    pub fn press(&mut self, screen: usize, p: Point, now: Instant) {
        self.last_input = now;
        // Một cú nhấn mới khi nét hay cử chỉ vẫn đang giữ nghĩa là đã mất `mouseUp` (nhả ngoài cửa
        // sổ, mất key): chốt cái cũ trước, để lớp phủ không bao giờ kẹt ở trạng thái "đang kéo".
        if self.annot.capturing() || self.annot.press_pending() {
            self.lost_capture(now);
        }
        let Some(bounds) = self.bounds(screen) else {
            return;
        };
        if self.active == Some(screen) && self.rect().is_some() {
            // Vùng công cụ: chữ, bảng, thanh, hình, rồi mới tới vùng chọn (`press_mode`).
            let rad = self
                .screens
                .get(screen)
                .map_or(1, |s| handle_radius(s.scale));
            let hit = self.sel.hit_test(p, rad);
            let Some(lay) = self.layout(screen) else {
                return;
            };
            let Some(img) = self.images.get(screen) else {
                return;
            };
            let env = lay.env(img);
            match self.annot.down(&env, p, hit) {
                Down::Bar(item) => {
                    self.pressed = Some(item);
                    return;
                }
                Down::Handled | Down::Capture => {
                    self.pressed = None;
                    return;
                }
                Down::Selection => {}
            }
        } else if self.active.is_some_and(|a| a != screen) {
            // Màn hình khác: hình đã vẽ ở màn hình cũ giữ vùng cũ (như `press_mode`: ngoài vùng
            // khi đã có hình, hoặc đang có công cụ, thì bỏ qua); không thì chuyển vùng chọn.
            let dpi = self.active.map_or(96, |a| self.dpi(a));
            self.annot.close_text(true);
            let _ = dpi;
            if press_mode(self.annot.tool(), self.annot.has_marks(), Hit::Outside)
                != PressMode::Selection
            {
                return;
            }
        }
        self.pressed = None;
        if self.active != Some(screen) {
            self.active = Some(screen);
            self.sel = Selection::new(bounds);
            self.hover = None;
            self.annot = Annot::with_prefs(self.annot.prefs());
        }
        let rad = self
            .screens
            .get(screen)
            .map_or(1, |s| handle_radius(s.scale));
        self.sel.press(p, rad);
    }

    /// Kéo tới `p` (pixel của màn hình đang chọn; ngoài màn hình thì `Selection` kẹp lại).
    pub fn drag(&mut self, p: Point, now: Instant) {
        self.last_input = now;
        if self.pressed.is_some() {
            return;
        }
        if self.annot.capturing() {
            let shift = self.shift;
            if let Some(screen) = self.active {
                if let Some(lay) = self.layout(screen) {
                    if let Some(img) = self.images.get(screen) {
                        let env = lay.env(img);
                        self.annot.moved(&env, p, shift);
                    }
                }
            }
            return;
        }
        if self.annot.press_pending() {
            return;
        }
        if self.sel.is_dragging() {
            self.sel.drag_to(p);
        }
    }

    /// Nhả chuột ở `p`; trả hành động khi nhấn và nhả trên cùng một nút của thanh ngang.
    pub fn release(&mut self, p: Point, now: Instant) -> Option<Action> {
        self.last_input = now;
        if let Some(item) = self.pressed.take() {
            let (s, bar) = self.bar()?;
            let dpi = self.dpi(s);
            if bar_item_at(bar, p, dpi) != Some(item) {
                return None;
            }
            return match item {
                BarItem::Undo => {
                    self.annot.undo(dpi);
                    None
                }
                BarItem::Redo => {
                    self.annot.redo(dpi);
                    None
                }
                BarItem::Act(a) => Some(a),
            };
        }
        if self.annot.capturing() || self.annot.press_pending() {
            let shift = self.shift;
            if let Some(screen) = self.active {
                if let Some(lay) = self.layout(screen) {
                    if let Some(img) = self.images.get(screen) {
                        let env = lay.env(img);
                        self.annot.up(&env, p, shift);
                    }
                }
            }
            return None;
        }
        if self.sel.is_dragging() {
            self.sel.drag_to(p);
        }
        self.sel.release();
        None
    }

    /// Chuột di chuyển (không nhấn) trên màn hình `screen`.
    pub fn hover(&mut self, screen: usize, p: Point, now: Instant) {
        self.last_input = now;
        let bar = self.bar().filter(|(s, _)| *s == screen);
        self.hover = bar.and_then(|(s, bar)| bar_item_at(bar, p, self.dpi(s)));
        match (self.layout(screen), self.images.get(screen)) {
            (Some(lay), Some(img)) => self.annot.hover(&lay.env(img), p),
            // Chuột ở màn hình khác (hoặc chưa có vùng chọn): không mục nào được trỏ.
            _ => self.annot.clear_hover(),
        }
    }

    /// Con trỏ chuột ở `p` trên màn hình `screen` (không có vùng chọn: chữ thập chọn vùng).
    pub fn cursor_at(&self, screen: usize, p: Point) -> CursorKind {
        let Some(lay) = self.layout(screen) else {
            return CursorKind::Cross;
        };
        let Some(img) = self.images.get(screen) else {
            return CursorKind::Cross;
        };
        let env = lay.env(img);
        let rad = self
            .screens
            .get(screen)
            .map_or(1, |s| handle_radius(s.scale));
        let hit = self.sel.hit_test(p, rad);
        self.annot.cursor_at(&env, p, hit)
    }

    /// Shift đang giữ (đọc từ sự kiện chuột/phím) để kéo nét và hình có ràng buộc.
    pub fn set_shift(&mut self, shift: bool) {
        self.shift = shift;
    }

    /// Nhấn hoặc nhả Shift: cập nhật nét hay hình đang kéo ngay theo điểm chuột cuối.
    pub fn shift_changed(&mut self, shift: bool, now: Instant) {
        self.last_input = now;
        self.shift = shift;
        let Some(screen) = self.active else {
            return;
        };
        if let Some(lay) = self.layout(screen) {
            if let Some(img) = self.images.get(screen) {
                let env = lay.env(img);
                self.annot.shift_changed(&env, shift);
            }
        }
    }

    /// Mất chuột giữa chừng (cửa sổ mất key): chốt nét, cử chỉ và kéo vùng đang dở.
    pub fn lost_capture(&mut self, now: Instant) {
        self.last_input = now;
        self.pressed = None;
        if let Some(screen) = self.active {
            if let Some(lay) = self.layout(screen) {
                if let Some(img) = self.images.get(screen) {
                    let env = lay.env(img);
                    self.annot.lost_capture(&env);
                }
            }
        }
        if self.sel.is_dragging() {
            self.sel.release();
        }
    }

    /// Một phím xuống; `repeat`: lần lặp của phím đang giữ. `Esc` luôn thoát được: bỏ lần lượt
    /// bảng, ô chữ, cử chỉ, hình đang chọn rồi hủy lớp phủ (mỗi lần `Esc` bỏ đúng một lớp).
    pub fn key(
        &mut self,
        key_code: u16,
        chars: Option<&str>,
        mods: Mods,
        repeat: bool,
        now: Instant,
    ) -> KeyOutcome {
        self.last_input = now;
        let esc = key_code == KEY_ESCAPE || chars == Some("\u{1b}");
        let has_sel = self.rect().is_some();
        let dragging = self.is_dragging();
        let dpi = self.active.map_or(96, |a| self.dpi(a));
        if esc {
            if dragging {
                return KeyOutcome::act(Some(Action::Cancel));
            }
            if self.annot.text_composing() {
                // Bộ gõ giữ chuỗi soạn dở: Esc của bộ gõ hủy chuỗi. Nếu lần Esc kế tiếp mà vẫn
                // còn soạn (bộ gõ không trả Esc) thì ép đóng ô chữ, không bao giờ kẹt.
                if self.esc_marked {
                    self.esc_marked = false;
                    self.annot.force_close_text();
                    return KeyOutcome::default();
                }
                self.esc_marked = true;
                return KeyOutcome {
                    ime: true,
                    ..KeyOutcome::default()
                };
            }
            self.esc_marked = false;
            if has_sel && self.annot.escape(dpi) {
                return KeyOutcome::default();
            }
            return KeyOutcome::act(Some(Action::Cancel));
        }
        self.esc_marked = false;
        if !has_sel || dragging || self.annot.capturing() {
            return KeyOutcome::default();
        }
        let cmd_only = mods.cmd && !mods.ctrl && !mods.opt;
        let lower = chars
            .and_then(|c| c.chars().next())
            .map(|c| c.to_ascii_lowercase());
        if self.annot.text().is_some() {
            if !mods.cmd {
                return KeyOutcome {
                    ime: true,
                    ..KeyOutcome::default()
                };
            }
            if cmd_only {
                match lower {
                    Some('z') if !mods.shift => self.annot.undo(dpi),
                    Some('v') => {
                        return KeyOutcome {
                            paste: true,
                            ..KeyOutcome::default()
                        }
                    }
                    _ => {}
                }
            }
            return KeyOutcome::act(key_action(key_code, chars, mods, has_sel, dragging));
        }
        if cmd_only {
            match lower {
                Some('z') => {
                    if mods.shift {
                        self.annot.redo(dpi);
                    } else {
                        self.annot.undo(dpi);
                    }
                    return KeyOutcome::default();
                }
                _ => return KeyOutcome::act(key_action(key_code, chars, mods, has_sel, dragging)),
            }
        }
        if mods.cmd || mods.ctrl || mods.opt {
            return KeyOutcome::act(key_action(key_code, chars, mods, has_sel, dragging));
        }
        // Phím trơn: Xóa bỏ hình đang chọn; chữ cái chọn công cụ, `[` `]` đổi cỡ.
        if key_code == KEY_DELETE || key_code == KEY_FORWARD_DELETE {
            self.annot.delete_selected(dpi);
            return KeyOutcome::default();
        }
        if !repeat {
            self.annot.tool_key(chars, dpi);
        }
        KeyOutcome::default()
    }

    /// Chốt mọi thứ đang dở và lấy hình đánh dấu để đốt vào ảnh (ô chữ vào CUỐI sổ, trên cùng).
    pub fn take_shapes(&mut self) -> Vec<Shape> {
        let dpi = self.active.map_or(96, |a| self.dpi(a));
        self.annot.take_shapes(dpi)
    }

    /// Lựa chọn công cụ để nhớ cho lần mở sau.
    pub fn prefs(&self) -> Prefs {
        self.annot.prefs()
    }

    /// Lấy ảnh gốc của màn hình `i` ra khỏi mô hình (gọi sau khi đã bỏ các bên chia sẻ khác).
    pub fn take_image(self, i: usize) -> Option<Image> {
        self.images
            .into_iter()
            .nth(i)
            .map(|rc| Rc::try_unwrap(rc).unwrap_or_else(|shared| (*shared).clone()))
    }

    /// Đã quá `IDLE_TIMEOUT` kể từ tương tác cuối.
    pub fn idle_expired(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.last_input) >= IDLE_TIMEOUT
    }

    /// Ghi nhận một tương tác không đi qua hàm trên (chữ từ bộ gõ, dán, nháy không tính).
    pub fn touch(&mut self, now: Instant) {
        self.last_input = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords::selection_to_pixels;
    use darkshot_core::i18n::{t, Lang};
    use darkshot_core::image::Image;

    fn r(x: f64, y: f64, w: f64, h: f64) -> RectF {
        RectF { x, y, w, h }
    }

    fn px(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    fn pt(x: i32, y: i32) -> Point {
        Point { x, y }
    }

    fn image(w: u32, h: u32) -> Image {
        let mut bgra = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                bgra.extend_from_slice(&[(x % 256) as u8, (y % 256) as u8, 7, 255]);
            }
        }
        Image {
            width: w,
            height: h,
            bgra,
        }
    }

    /// Hành động của một phím qua mô hình (bỏ cờ bộ gõ, dán).
    fn act(
        m: &mut Model,
        code: u16,
        chars: Option<&str>,
        mods: Mods,
        now: Instant,
    ) -> Option<Action> {
        m.key(code, chars, mods, false, now).action
    }

    const RETINA: ScreenPx = ScreenPx {
        w: 3024,
        h: 1964,
        scale: 2.0,
    };
    const EXT: ScreenPx = ScreenPx {
        w: 1920,
        h: 1080,
        scale: 1.0,
    };

    fn two_screens(t0: Instant) -> Model {
        Model::new(vec![RETINA, EXT], t0)
    }

    #[test]
    fn dpi_follows_scale() {
        assert_eq!(dpi_of(1.0), 96);
        assert_eq!(dpi_of(2.0), 192);
        assert_eq!(dpi_of(1.5), 144);
        assert_eq!(dpi_of(0.0), 96);
        assert_eq!(dpi_of(f64::NAN), 96);
    }

    #[test]
    fn event_point_on_retina_primary() {
        // Màn hình chính 1512×982 điểm ở 2x: điểm (100, 50) tính từ góc trên trái.
        let screen = r(0.0, 0.0, 1512.0, 982.0);
        assert_eq!(
            event_to_px((100.0, 982.0 - 50.0), screen, 2.0),
            pt(200, 100)
        );
        assert_eq!(event_to_px((0.0, 982.0), screen, 2.0), pt(0, 0));
        assert_eq!(event_to_px((1512.0, 0.0), screen, 2.0), pt(3024, 1964));
    }

    #[test]
    fn event_point_on_secondary_with_negative_origin() {
        // Màn hình trái (gốc Cocoa x âm), 1x.
        let left = r(-1440.0, 0.0, 1440.0, 900.0);
        assert_eq!(event_to_px((-1430.0, 880.0), left, 1.0), pt(10, 20));
        // Màn hình dưới màn hình chính (gốc Cocoa y âm), 2x.
        let below = r(100.0, -900.0, 1440.0, 900.0);
        assert_eq!(event_to_px((101.0, -1.0), below, 2.0), pt(2, 2));
        // Màn hình trên (y Cocoa = chiều cao màn hình chính), 2x.
        let above = r(0.0, 982.0, 2560.0, 1440.0);
        assert_eq!(
            event_to_px((5.0, 982.0 + 1440.0 - 7.0), above, 2.0),
            pt(10, 14)
        );
        // Kéo ra ngoài màn hình: toạ độ âm hoặc quá cạnh, `Selection` kẹp sau.
        assert_eq!(event_to_px((-1450.0, 910.0), left, 1.0), pt(-10, -10));
    }

    #[test]
    fn handle_radius_scales() {
        assert_eq!(handle_radius(1.0), HANDLE_HIT);
        assert_eq!(handle_radius(2.0), HANDLE_HIT * 2);
    }

    #[test]
    fn selection_200x100_points_at_2x_crops_400x200_pixels() {
        let img = image(3024, 1964);
        // Qua coords::selection_to_pixels.
        let rect = selection_to_pixels(r(10.0, 20.0, 200.0, 100.0), 2.0, 3024, 1964).unwrap();
        assert_eq!(rect, px(20, 40, 400, 200));
        let out = img.crop(rect);
        assert_eq!((out.width, out.height), (400, 200));
        // Qua đường sự kiện thật: kéo 200×100 điểm trên màn hình Retina.
        let t0 = Instant::now();
        let screen = r(0.0, 0.0, 1512.0, 982.0);
        let mut m = Model::new(vec![RETINA], t0);
        m.press(0, event_to_px((10.0, 982.0 - 20.0), screen, 2.0), t0);
        m.drag(event_to_px((210.0, 982.0 - 120.0), screen, 2.0), t0);
        assert_eq!(
            m.release(event_to_px((210.0, 982.0 - 120.0), screen, 2.0), t0),
            None
        );
        let sel = m.rect().unwrap();
        assert_eq!(sel, px(20, 40, 400, 200));
        let out = img.crop(sel);
        assert_eq!((out.width, out.height), (400, 200));
        // Đúng nội dung: pixel đầu là (20, 40) của ảnh gốc, không lật.
        assert_eq!(&out.bgra[..4], &[20, 40, 7, 255]);
        let last = out.bgra.len() - 4;
        assert_eq!(&out.bgra[last..], &[(419 % 256) as u8, 239, 7, 255]);
    }

    #[test]
    fn match_screens_by_frame() {
        // Màn hình chính 1512×982; màn hình phụ bên trái thấp hơn (gốc âm).
        let screens = [
            r(0.0, 0.0, 1512.0, 982.0),
            r(-1920.0, -98.0, 1920.0, 1080.0),
        ];
        let main_tl = (r(0.0, 0.0, 1512.0, 982.0), 3024 * 1964);
        let left_tl = (r(-1920.0, 0.0, 1920.0, 1080.0), 1920 * 1080);
        // Cặp (ảnh, màn hình) theo thứ tự ảnh, đúng cả khi thứ tự hai bên khác nhau.
        assert_eq!(
            match_screens(&[main_tl, left_tl], &screens),
            Some(vec![(0, 0), (1, 1)])
        );
        assert_eq!(
            match_screens(&[left_tl, main_tl], &screens),
            Some(vec![(0, 1), (1, 0)])
        );
        // Lệch nhỏ hơn dung sai vẫn khớp.
        let near = (r(0.2, -0.2, 1512.0, 982.0), 1);
        assert_eq!(match_screens(&[near], &screens[..1]), Some(vec![(0, 0)]));
        // Màn hình không có ảnh: không khớp (cấu hình đã đổi).
        assert_eq!(match_screens(&[main_tl], &screens), None);
        assert_eq!(
            match_screens(&[(r(0.0, 0.0, 1920.0, 1080.0), 1), left_tl], &screens),
            None
        );
        assert_eq!(match_screens(&[main_tl, main_tl], &screens), None);
        assert_eq!(match_screens(&[], &[]), None);
        assert_eq!(match_screens(&[], &screens), None);
        assert_eq!(match_screens(&[main_tl], &[]), None);
    }

    #[test]
    fn mirrored_displays_keep_the_best_image_per_screen() {
        // Phản chiếu màn hình: ScreenCaptureKit liệt kê hai display cùng khung với màn hình chính.
        let screens = [r(0.0, 0.0, 1512.0, 982.0)];
        let mirror_small = (r(0.0, 0.0, 1512.0, 982.0), 1512 * 982);
        let mirror_big = (r(0.0, 0.0, 1512.0, 982.0), 3024 * 1964);
        assert_eq!(
            match_screens(&[mirror_small, mirror_big], &screens),
            Some(vec![(1, 0)])
        );
        assert_eq!(
            match_screens(&[mirror_big, mirror_small], &screens),
            Some(vec![(0, 0)])
        );
        // Bằng nhau: giữ ảnh đầu.
        assert_eq!(
            match_screens(&[mirror_big, mirror_big], &screens),
            Some(vec![(0, 0)])
        );
        // Nhiều display hơn NSScreen: ảnh không khớp màn hình nào (display gương khác cỡ) bị bỏ,
        // mỗi màn hình vẫn có một ảnh.
        let odd = (r(0.0, 0.0, 1920.0, 1080.0), 1920 * 1080);
        assert_eq!(
            match_screens(&[odd, mirror_big], &screens),
            Some(vec![(1, 0)])
        );
    }

    #[test]
    fn mirrored_pair_plus_secondary_with_negative_origin() {
        let screens = [
            r(0.0, 0.0, 1512.0, 982.0),
            r(-1920.0, -98.0, 1920.0, 1080.0),
        ];
        let left = (r(-1920.0, 0.0, 1920.0, 1080.0), 1920 * 1080);
        let main_a = (r(0.0, 0.0, 1512.0, 982.0), 1512 * 982);
        let main_b = (r(0.0, 0.0, 1512.0, 982.0), 3024 * 1964);
        assert_eq!(
            match_screens(&[left, main_a, main_b], &screens),
            Some(vec![(0, 1), (2, 0)])
        );
        assert_eq!(
            match_screens(&[main_b, left, main_a], &screens),
            Some(vec![(0, 0), (1, 1)])
        );
    }

    #[test]
    fn size_label_is_real_pixels() {
        assert_eq!(size_label(px(20, 40, 400, 200)), "400 \u{d7} 200");
        // Nhãn của vùng kéo 200×100 điểm ở 2x là cỡ pixel thật.
        let t0 = Instant::now();
        let mut m = Model::new(vec![RETINA], t0);
        m.press(0, pt(20, 40), t0);
        m.drag(pt(420, 240), t0);
        m.release(pt(420, 240), t0);
        assert_eq!(size_label(m.rect().unwrap()), "400 \u{d7} 200");
    }

    #[test]
    fn label_sits_above_or_inside_and_stays_on_screen() {
        let mon = px(0, 0, 3024, 1964);
        let dpi = 192;
        // Đủ chỗ phía trên: nằm trên vùng, thẳng mép trái.
        let l = label_rect(px(100, 200, 400, 200), (80, 30), dpi, mon);
        assert_eq!(l.x, 100);
        assert_eq!(l.bottom(), 200 - scaled(GAP, dpi));
        // Sát mép trên: vào trong góc trên trái vùng.
        let l = label_rect(px(100, 0, 400, 200), (80, 30), dpi, mon);
        assert_eq!((l.x, l.y), (100 + scaled(GAP, dpi), scaled(GAP, dpi)));
        // Sát mép phải: kẹp vào màn hình.
        let l = label_rect(px(3020, 500, 4, 4), (80, 30), dpi, mon);
        assert!(l.right() <= mon.right());
    }

    #[test]
    fn hint_is_centered_below_the_notch() {
        let mon = px(0, 0, 3024, 1964);
        let h = hint_rect(mon, (400, 30), 192);
        assert_eq!(h.x + h.w / 2, 1512);
        assert!(h.y >= scaled(HINT_NOTCH_CLEAR, 192));
    }

    #[test]
    fn bar_is_undo_redo_separator_then_four_actions() {
        assert_eq!(bar_button_size(96), BUTTON);
        assert_eq!(bar_button_size(192), BUTTON * 2);
        // Hoàn tác, Làm lại, vạch ngăn, rồi bốn nút hành động.
        let cells = bar_cells();
        assert_eq!(cells.len(), 7);
        assert_eq!(cells[0], Some(BarItem::Undo));
        assert_eq!(cells[1], Some(BarItem::Redo));
        assert_eq!(cells[2], None);
        let mon = px(0, 0, 3024, 1964);
        let bar = bar_rect(px(100, 100, 400, 200), 192, mon);
        let sep = scaled(SIDE_SEP, 192);
        assert_eq!((bar.w, bar.h), (6 * BUTTON * 2 + sep, BUTTON * 2));
        // Đủ chỗ: dưới vùng, cách GAP, mép phải thẳng mép phải vùng.
        assert_eq!(bar.y, 300 + GAP * 2);
        assert_eq!(bar.right(), 500);
    }

    #[test]
    fn bar_stays_on_screen_at_corners_and_edges() {
        for (scale, mon) in [
            (2.0, px(0, 0, 3024, 1964)),
            (1.0, px(0, 0, 1920, 1080)),
            (1.0, px(0, 0, 260, 150)),
        ] {
            let dpi = dpi_of(scale);
            let (w, h) = (mon.w, mon.h);
            let sels = [
                px(0, 0, 100, 100),
                px(w - 100, 0, 100, 100),
                px(0, h - 100, 100, 100),
                px(w - 100, h - 100, 100, 100),
                px(0, 0, w, h),
                px(0, 0, 3, 3),
                px(w - 3, h - 3, 3, 3),
                px(w / 2, 0, 50, h),
                px(0, h / 2, w, 5),
            ];
            for sel in sels {
                let bar = bar_rect(sel, dpi, mon);
                assert!(
                    bar.x >= mon.x
                        && bar.y >= mon.y
                        && bar.right() <= mon.right()
                        && bar.bottom() <= mon.bottom(),
                    "{sel:?} @ {mon:?} -> {bar:?}"
                );
                let tip = tip_rect(bar, (300, 30), dpi, mon);
                assert!(tip.x >= mon.x && tip.right() <= mon.right(), "{tip:?}");
                assert!(tip.y >= mon.y && tip.bottom() <= mon.bottom(), "{tip:?}");
            }
        }
    }

    #[test]
    fn bar_hit_test() {
        let dpi = 192;
        let b = bar_button_size(dpi);
        let l = bar_layout(dpi);
        let bar = px(1000, 500, l.size.0, b);
        let items: Vec<BarItem> = l.cells.iter().filter_map(|(i, _)| *i).collect();
        assert_eq!(items.len(), 6);
        for it in &items {
            let cell = bar_button_rect(bar, dpi, *it).unwrap();
            assert_eq!((cell.w, cell.h), (b, b));
            assert_eq!(
                bar_item_at(bar, pt(cell.x + b / 2, cell.y + b / 2), dpi),
                Some(*it)
            );
            assert_eq!(bar_item_at(bar, pt(cell.x, cell.y), dpi), Some(*it));
            assert_eq!(
                bar_item_at(bar, pt(cell.right() - 1, cell.bottom() - 1), dpi),
                Some(*it)
            );
        }
        // Vạch ngăn không phải nút; ngoài thanh không có nút.
        let undo = bar_button_rect(bar, dpi, BarItem::Undo).unwrap();
        let redo = bar_button_rect(bar, dpi, BarItem::Redo).unwrap();
        let first_act = bar_button_rect(bar, dpi, BarItem::Act(Action::Upload)).unwrap();
        assert_eq!(redo.x, undo.x + b);
        assert!(first_act.x > redo.right());
        assert_eq!(bar_item_at(bar, pt(redo.right() + 1, 510), dpi), None);
        assert_eq!(bar_item_at(bar, pt(999, 510), dpi), None);
        assert_eq!(bar_item_at(bar, pt(bar.right(), 510), dpi), None);
        assert_eq!(bar_item_at(bar, pt(1010, 500 + b), dpi), None);
    }

    #[test]
    fn bar_order_follows_the_core_bar_cells() {
        use darkshot_core::overlay_layout::bar_cells as core_cells;
        // Các nút Mac có, theo thứ tự của thanh ngang Windows (bỏ ô Mac chưa làm).
        let from_core: Vec<Action> = core_cells(false)
            .into_iter()
            .flatten()
            .filter_map(|c| match c {
                BarItem::Act(a) if MAC_BAR.contains(&a) => Some(a),
                _ => None,
            })
            .collect();
        assert_eq!(from_core, MAC_BAR.to_vec());
        assert_eq!(
            MAC_BAR,
            [Action::Upload, Action::Save, Action::Copy, Action::Cancel]
        );
        // Hoàn tác và Làm lại đứng đầu, như thanh ngang của Windows.
        assert_eq!(
            &core_cells(false)[..3],
            &[Some(BarItem::Undo), Some(BarItem::Redo), None]
        );
        assert_eq!(&bar_cells()[..3], &core_cells(false)[..3]);
    }

    #[test]
    fn bar_hit_test_at_retina_and_at_the_edges() {
        for dpi in [96, 144, 192, 288] {
            let l = bar_layout(dpi);
            let b = bar_button_size(dpi);
            let bar = px(40, 40, l.size.0, b);
            let items: Vec<BarItem> = l.cells.iter().filter_map(|(i, _)| *i).collect();
            for it in &items {
                let c = bar_button_rect(bar, dpi, *it).unwrap();
                // Pixel đầu và cuối của ô thuộc ô này.
                assert_eq!(bar_item_at(bar, pt(c.x, c.y), dpi), Some(*it), "dpi {dpi}");
                assert_eq!(
                    bar_item_at(bar, pt(c.right() - 1, c.bottom() - 1), dpi),
                    Some(*it),
                    "dpi {dpi}"
                );
            }
            // Hai nút hành động liền nhau: pixel liền kề thuộc ô bên cạnh.
            let up = bar_button_rect(bar, dpi, BarItem::Act(Action::Upload)).unwrap();
            assert_eq!(
                bar_item_at(bar, pt(up.right(), up.y), dpi),
                Some(BarItem::Act(Action::Save))
            );
            assert_eq!(bar_item_at(bar, pt(bar.x - 1, bar.y), dpi), None);
            assert_eq!(bar_item_at(bar, pt(bar.right(), bar.y), dpi), None);
            assert_eq!(bar_item_at(bar, pt(bar.x, bar.bottom()), dpi), None);
        }
        // Vùng sát mép màn hình: thanh vẫn đủ nút trong màn hình và mỗi nút bấm được.
        let mon = px(0, 0, 3024, 1964);
        for sel in [
            px(0, 0, 30, 30),
            px(2994, 1934, 30, 30),
            px(0, 1934, 3024, 30),
            px(1000, 1000, 2024, 964),
        ] {
            let bar = bar_rect(sel, 192, mon);
            assert!(bar.x >= 0 && bar.right() <= mon.right() && bar.bottom() <= mon.bottom());
            assert_eq!(bar.w, bar_layout(192).size.0);
            for (it, _) in bar_layout(192).cells.iter().filter(|(i, _)| i.is_some()) {
                let it = it.unwrap();
                let c = bar_button_rect(bar, 192, it).unwrap();
                assert_eq!(
                    bar_item_at(bar, pt(c.x + c.w / 2, c.y + c.h / 2), 192),
                    Some(it),
                    "{sel:?} {it:?}"
                );
            }
        }
    }

    #[test]
    fn mac_tips_use_mac_shortcuts() {
        assert_eq!(bar_tip(BarItem::Act(Action::Upload)), Msg::OverlayUploadMac);
        assert_eq!(bar_tip(BarItem::Act(Action::Save)), Msg::OverlaySaveMac);
        assert_eq!(bar_tip(BarItem::Act(Action::Copy)), Msg::OverlayCopyMac);
        assert_eq!(bar_tip(BarItem::Act(Action::Cancel)), Msg::OverlayCancelMac);
        assert_eq!(bar_tip(BarItem::Undo), Msg::ToolUndoMac);
        assert_eq!(bar_tip(BarItem::Redo), Msg::ToolRedoMac);
        for l in Lang::ALL {
            assert!(t(l, Msg::OverlayUploadMac).contains("(\u{2318}D)"));
            assert!(t(l, Msg::OverlaySaveMac).contains("(\u{2318}S)"));
            assert!(t(l, Msg::OverlayCopyMac).contains("(\u{2318}C)"));
            assert!(t(l, Msg::OverlayCancelMac).contains("(Esc)"));
            assert!(t(l, Msg::ToolUndoMac).contains("(\u{2318}Z)"));
            assert!(t(l, Msg::ToolRedoMac).contains("(\u{21e7}\u{2318}Z)"));
            for m in [
                Msg::OverlayUploadMac,
                Msg::OverlaySaveMac,
                Msg::OverlayCopyMac,
                Msg::OverlayCancelMac,
                Msg::ToolUndoMac,
                Msg::ToolRedoMac,
            ] {
                assert!(!t(l, m).contains("Ctrl"), "{m:?}");
            }
        }
        assert_eq!(t(Lang::Vi, Msg::OverlaySaveMac), "Lưu vào máy (⌘S)");
        assert_eq!(t(Lang::En, Msg::OverlaySaveMac), "Save to disk (⌘S)");
        assert_eq!(t(Lang::Vi, Msg::OverlayCopyMac), "Chép ảnh (⌘C)");
        assert_eq!(t(Lang::En, Msg::OverlayCopyMac), "Copy image (⌘C)");
        assert_eq!(t(Lang::Vi, Msg::OverlayCancelMac), "Hủy (Esc)");
        assert_eq!(t(Lang::En, Msg::OverlayCancelMac), "Cancel (Esc)");
    }

    #[test]
    fn key_table() {
        let none = Mods::default();
        let cmd = Mods {
            cmd: true,
            ..Mods::default()
        };
        let cmd_shift = Mods {
            cmd: true,
            shift: true,
            ..Mods::default()
        };
        let cmd_opt = Mods {
            cmd: true,
            opt: true,
            ..Mods::default()
        };
        let ctrl = Mods {
            ctrl: true,
            ..Mods::default()
        };
        // Esc hủy ở mọi trạng thái (kể cả đang kéo, chưa có vùng, có phím bổ trợ).
        for (has, drag) in [(false, false), (true, false), (true, true), (false, true)] {
            for m in [none, cmd, ctrl, cmd_opt] {
                assert_eq!(
                    key_action(KEY_ESCAPE, Some("\u{1b}"), m, has, drag),
                    Some(Action::Cancel)
                );
                assert_eq!(
                    key_action(KEY_ESCAPE, None, m, has, drag),
                    Some(Action::Cancel)
                );
            }
        }
        // ⌘S, ⌘C khi có vùng và không đang kéo; Shift không cản (như Ctrl+S của Windows).
        assert_eq!(
            key_action(1, Some("s"), cmd, true, false),
            Some(Action::Save)
        );
        assert_eq!(
            key_action(8, Some("c"), cmd, true, false),
            Some(Action::Copy)
        );
        assert_eq!(
            key_action(1, Some("S"), cmd_shift, true, false),
            Some(Action::Save)
        );
        // Bố cục bàn phím khác: theo chữ, không theo vị trí phím.
        assert_eq!(
            key_action(31, Some("s"), cmd, true, false),
            Some(Action::Save)
        );
        assert_eq!(
            key_action(2, Some("d"), cmd, true, false),
            Some(Action::Upload)
        );
        assert_eq!(
            key_action(2, Some("D"), cmd_shift, true, false),
            Some(Action::Upload)
        );
        assert_eq!(key_action(2, Some("d"), cmd, false, false), None);
        assert_eq!(key_action(2, Some("d"), cmd, true, true), None);
        assert_eq!(key_action(2, Some("d"), none, true, false), None);
        assert_eq!(key_action(2, Some("d"), cmd_opt, true, false), None);
        assert_eq!(key_action(1, Some("o"), cmd, true, false), None);
        // Không có vùng, đang kéo, thiếu ⌘, thêm Option/Control: không làm gì.
        assert_eq!(key_action(1, Some("s"), cmd, false, false), None);
        assert_eq!(key_action(1, Some("s"), cmd, true, true), None);
        assert_eq!(key_action(1, Some("s"), none, true, false), None);
        assert_eq!(key_action(1, Some("s"), ctrl, true, false), None);
        assert_eq!(key_action(1, Some("s"), cmd_opt, true, false), None);
        // Enter không có tương đương trên Windows (chỉ chọn ứng viên bắt cửa sổ).
        assert_eq!(key_action(36, Some("\r"), none, true, false), None);
    }

    #[test]
    fn pressing_another_screen_moves_the_selection_there() {
        let t0 = Instant::now();
        let mut m = two_screens(t0);
        assert_eq!(m.active(), None);
        m.press(0, pt(100, 100), t0);
        m.drag(pt(500, 300), t0);
        m.release(pt(500, 300), t0);
        assert_eq!(m.active(), Some(0));
        assert_eq!(m.view_state(0).rect, Some(px(100, 100, 400, 200)));
        assert!(m.view_state(0).bar.is_some());
        // Bấm ở màn hình kia: màn hình đó thành màn hình đang chọn, vùng cũ bị bỏ.
        m.press(1, pt(10, 10), t0);
        assert_eq!(m.active(), Some(1));
        assert_eq!(m.view_state(0).rect, None);
        assert_eq!(m.view_state(0).bar, None);
        m.drag(pt(60, 40), t0);
        m.release(pt(60, 40), t0);
        assert_eq!(m.rect(), Some(px(10, 10, 50, 30)));
        assert_eq!(m.view_state(1).rect, Some(px(10, 10, 50, 30)));
        // Bấm lại màn hình đầu: bắt đầu vùng mới, không giữ vùng cũ của nó.
        m.press(0, pt(7, 7), t0);
        m.release(pt(7, 7), t0);
        assert_eq!(m.active(), Some(0));
        assert_eq!(m.rect(), None);
        assert_eq!(m.view_state(1).rect, None);
        // Màn hình không tồn tại: bỏ qua.
        m.press(5, pt(1, 1), t0);
        assert_eq!(m.active(), Some(0));
    }

    #[test]
    fn selection_never_spans_screens() {
        let t0 = Instant::now();
        let mut m = two_screens(t0);
        m.press(1, pt(1800, 1000), t0);
        // Kéo sang màn hình khác (toạ độ ngoài màn hình 1): kẹp vào màn hình 1.
        m.drag(pt(5000, -300), t0);
        m.release(pt(5000, -300), t0);
        assert_eq!(m.rect(), Some(px(1800, 0, 120, 1000)));
        assert!(m.bounds(1).unwrap().right() >= m.rect().unwrap().right());
    }

    #[test]
    fn handle_radius_follows_screen_scale() {
        let t0 = Instant::now();
        let mut m = two_screens(t0);
        // Retina: 10 px ngoài góc dưới phải vẫn trong bán kính 12 px → đổi cỡ.
        m.press(0, pt(100, 100), t0);
        m.drag(pt(500, 300), t0);
        m.release(pt(500, 300), t0);
        m.press(0, pt(510, 310), t0);
        m.drag(pt(600, 400), t0);
        m.release(pt(600, 400), t0);
        assert_eq!(m.rect(), Some(px(100, 100, 500, 300)));
        // 1x: cùng khoảng cách ngoài bán kính 6 px → bắt đầu vùng mới (điểm bấm chọn xa hai thanh
        // công cụ, nằm dưới và bên phải vùng chọn).
        m.press(1, pt(100, 100), t0);
        m.drag(pt(500, 300), t0);
        m.release(pt(500, 300), t0);
        m.press(1, pt(90, 90), t0);
        m.drag(pt(200, 150), t0);
        m.release(pt(200, 150), t0);
        assert_eq!(m.rect(), Some(px(90, 90, 110, 60)));
        // Cùng 1x nhưng sát góc (trong bán kính): đổi cỡ vùng cũ, không tạo vùng mới.
        m.press(1, pt(205, 155), t0);
        m.drag(pt(300, 200), t0);
        m.release(pt(300, 200), t0);
        assert_eq!(m.rect(), Some(px(90, 90, 210, 110)));
    }

    #[test]
    fn move_inside_selection() {
        let t0 = Instant::now();
        let mut m = two_screens(t0);
        m.press(0, pt(100, 100), t0);
        m.drag(pt(500, 300), t0);
        m.release(pt(500, 300), t0);
        m.press(0, pt(300, 200), t0);
        m.drag(pt(350, 260), t0);
        assert!(m.is_dragging());
        assert_eq!(m.view_state(0).bar, None, "không hiện thanh khi đang kéo");
        m.release(pt(350, 260), t0);
        assert_eq!(m.rect(), Some(px(150, 160, 400, 200)));
    }

    #[test]
    fn bar_buttons_act_on_press_and_release_on_the_same_button() {
        let t0 = Instant::now();
        let mut m = two_screens(t0);
        m.press(0, pt(100, 100), t0);
        m.drag(pt(500, 300), t0);
        m.release(pt(500, 300), t0);
        let (screen, bar) = m.bar().unwrap();
        assert_eq!(screen, 0);
        let dpi = m.dpi(0);
        let center = |it: BarItem| {
            let c = bar_button_rect(bar, dpi, it).unwrap();
            pt(c.x + c.w / 2, c.y + c.h / 2)
        };
        for a in MAC_BAR {
            let it = BarItem::Act(a);
            m.press(0, center(it), t0);
            assert_eq!(m.view_state(0).pressed, Some(it));
            // Nhấn nút không đổi vùng chọn.
            assert_eq!(m.rect(), Some(px(100, 100, 400, 200)));
            m.drag(pt(0, 0), t0);
            assert_eq!(m.rect(), Some(px(100, 100, 400, 200)));
            assert_eq!(m.release(center(it), t0), Some(a));
            assert_eq!(m.view_state(0).pressed, None);
        }
        // Hoàn tác và Làm lại không đóng lớp phủ (không trả hành động).
        for it in [BarItem::Undo, BarItem::Redo] {
            m.press(0, center(it), t0);
            assert_eq!(m.view_state(0).pressed, Some(it));
            assert_eq!(m.release(center(it), t0), None);
        }
        // Nhả ở nút khác hoặc ngoài thanh: không làm gì.
        let (save, copy) = (BarItem::Act(Action::Save), BarItem::Act(Action::Copy));
        m.press(0, center(save), t0);
        assert_eq!(m.release(center(copy), t0), None);
        m.press(0, center(copy), t0);
        assert_eq!(m.release(pt(5, 5), t0), None);
        assert_eq!(m.rect(), Some(px(100, 100, 400, 200)));
    }

    #[test]
    fn hover_tracks_bar_buttons_on_the_active_screen() {
        let t0 = Instant::now();
        let mut m = two_screens(t0);
        m.press(0, pt(100, 100), t0);
        m.drag(pt(500, 300), t0);
        m.release(pt(500, 300), t0);
        let (_, bar) = m.bar().unwrap();
        let c = bar_button_rect(bar, m.dpi(0), BarItem::Act(Action::Copy)).unwrap();
        m.hover(0, pt(c.x + 1, c.y + 1), t0);
        assert_eq!(m.view_state(0).hover, Some(BarItem::Act(Action::Copy)));
        // Trỏ ở màn hình khác: không có nút nào được trỏ.
        m.hover(1, pt(c.x + 1, c.y + 1), t0);
        assert_eq!(m.view_state(0).hover, None);
        m.hover(0, pt(1, 1), t0);
        assert_eq!(m.view_state(0).hover, None);
    }

    #[test]
    fn hint_shows_until_a_selection_exists() {
        let t0 = Instant::now();
        let mut m = two_screens(t0);
        assert!(m.view_state(0).hint && m.view_state(1).hint);
        m.press(0, pt(100, 100), t0);
        m.drag(pt(200, 200), t0);
        assert!(!m.view_state(0).hint && !m.view_state(1).hint);
        m.release(pt(200, 200), t0);
        assert!(!m.view_state(1).hint);
        // Vùng quá nhỏ bị bỏ khi nhả: nhắc lại.
        m.press(1, pt(10, 10), t0);
        m.release(pt(11, 11), t0);
        assert_eq!(m.rect(), None);
        assert!(m.view_state(0).hint && m.view_state(1).hint);
        assert_eq!(m.view_state(7), ViewState::default_hidden());
    }

    #[test]
    fn keys_through_the_model() {
        let t0 = Instant::now();
        let mut m = two_screens(t0);
        let cmd = Mods {
            cmd: true,
            ..Mods::default()
        };
        // Chưa có vùng: ⌘S không làm gì, Esc hủy.
        assert_eq!(act(&mut m, 1, Some("s"), cmd, t0), None);
        assert_eq!(
            act(&mut m, KEY_ESCAPE, None, Mods::default(), t0),
            Some(Action::Cancel)
        );
        m.press(0, pt(100, 100), t0);
        m.drag(pt(500, 300), t0);
        // Đang kéo: ⌘C không làm gì, Esc vẫn hủy.
        assert_eq!(act(&mut m, 8, Some("c"), cmd, t0), None);
        assert_eq!(
            act(&mut m, KEY_ESCAPE, None, Mods::default(), t0),
            Some(Action::Cancel)
        );
        m.release(pt(500, 300), t0);
        assert_eq!(act(&mut m, 8, Some("c"), cmd, t0), Some(Action::Copy));
        assert_eq!(act(&mut m, 1, Some("s"), cmd, t0), Some(Action::Save));
    }

    #[test]
    fn key_loss_cancels_only_when_no_overlay_window_is_key() {
        // Bấm sang màn hình khác: cửa sổ kia nhận key, không hủy.
        assert!(!cancel_after_key_loss(&[false, true]));
        assert!(!cancel_after_key_loss(&[true, false]));
        assert!(!cancel_after_key_loss(&[true]));
        // Ứng dụng khác lấy bàn phím (⌘Tab, bấm ngoài): không cửa sổ nào còn key, hủy.
        assert!(cancel_after_key_loss(&[false, false]));
        assert!(cancel_after_key_loss(&[false]));
        // Không còn cửa sổ nào: hủy (không bao giờ để phiên treo).
        assert!(cancel_after_key_loss(&[]));
    }

    #[test]
    fn idle_safety_net() {
        assert_eq!(IDLE_TIMEOUT, Duration::from_secs(120));
        assert!(IDLE_CHECK_EVERY < IDLE_TIMEOUT);
        let t0 = Instant::now();
        let mut m = two_screens(t0);
        assert!(!m.idle_expired(t0));
        assert!(!m.idle_expired(t0 + Duration::from_secs(119)));
        assert!(m.idle_expired(t0 + IDLE_TIMEOUT));
        // Mọi tương tác dời hạn: trỏ chuột, nhấn, kéo, nhả, phím.
        let t1 = t0 + Duration::from_secs(100);
        m.hover(0, pt(1, 1), t1);
        assert!(!m.idle_expired(t1 + Duration::from_secs(119)));
        assert!(m.idle_expired(t1 + IDLE_TIMEOUT));
        let t2 = t1 + Duration::from_secs(100);
        m.press(0, pt(1, 1), t2);
        assert!(!m.idle_expired(t2 + Duration::from_secs(119)));
        let t3 = t2 + Duration::from_secs(100);
        m.drag(pt(50, 50), t3);
        assert!(!m.idle_expired(t3 + Duration::from_secs(119)));
        let t4 = t3 + Duration::from_secs(100);
        m.release(pt(50, 50), t4);
        assert!(!m.idle_expired(t4 + Duration::from_secs(119)));
        let t5 = t4 + Duration::from_secs(100);
        m.key(0, Some("a"), Mods::default(), false, t5);
        assert!(!m.idle_expired(t5 + Duration::from_secs(119)));
        assert!(m.idle_expired(t5 + IDLE_TIMEOUT));
        // Đồng hồ lùi (không thể với `Instant`, nhưng phòng hờ): không tính là hết hạn.
        assert!(!m.idle_expired(t0));
    }

    // ------------------------------------------------------------ công cụ đánh dấu qua mô hình

    use crate::annotate_ui::{Flyout, TextCmd};
    use darkshot_core::annotate::{Shape, Tool};
    use darkshot_core::overlay_layout::{side_index, side_item_rect, SideItem};

    /// Mô hình hai màn hình với vùng chọn (100, 100, 400, 200) trên màn hình Retina.
    fn selected_model(t0: Instant) -> Model {
        let mut m = two_screens(t0);
        m.press(0, pt(100, 100), t0);
        m.drag(pt(500, 300), t0);
        m.release(pt(500, 300), t0);
        assert_eq!(m.rect(), Some(px(100, 100, 400, 200)));
        m
    }

    fn click(m: &mut Model, p: Point, t0: Instant) {
        m.press(0, p, t0);
        m.release(p, t0);
    }

    fn click_side(m: &mut Model, item: SideItem, t0: Instant) {
        let (_, side) = m.side().unwrap();
        let l = side_layout_for(m.bounds(0).unwrap(), m.dpi(0));
        let r = side_item_rect(side, &l, side_index(item)).unwrap();
        click(m, pt(r.x + r.w / 2, r.y + r.h / 2), t0);
    }

    /// Bật công cụ `t` (hình Đường, Mũi tên, Khung, Elip nằm trong bảng Hình nên chọn thẳng).
    fn use_tool(m: &mut Model, t: Tool) {
        let dpi = m.dpi(0);
        m.annot_mut().use_tool(t, dpi);
    }

    fn draw(m: &mut Model, from: Point, to: Point, t0: Instant) {
        m.press(0, from, t0);
        m.drag(to, t0);
        m.release(to, t0);
    }

    fn key(m: &mut Model, code: u16, chars: &str, mods: Mods, t0: Instant) -> KeyOutcome {
        m.key(code, Some(chars), mods, false, t0)
    }

    const CMD: Mods = Mods {
        cmd: true,
        ctrl: false,
        opt: false,
        shift: false,
    };
    const CMD_SHIFT: Mods = Mods {
        cmd: true,
        ctrl: false,
        opt: false,
        shift: true,
    };
    const NONE: Mods = Mods {
        cmd: false,
        ctrl: false,
        opt: false,
        shift: false,
    };

    #[test]
    fn side_bar_appears_with_the_bar_and_does_not_cover_it() {
        let t0 = Instant::now();
        let m = selected_model(t0);
        let (_, bar) = m.bar().unwrap();
        let (_, side) = m.side().unwrap();
        assert!(m.view_state(0).side.is_some());
        let overlap = side.x < bar.right()
            && bar.x < side.right()
            && side.y < bar.bottom()
            && bar.y < side.bottom();
        assert!(!overlap, "{bar:?} {side:?}");
        // Màn hình khác và lúc đang kéo vùng: không có thanh dọc.
        assert_eq!(m.view_state(1).side, None);
        let mut m = two_screens(t0);
        m.press(0, pt(100, 100), t0);
        m.drag(pt(500, 300), t0);
        assert_eq!(m.view_state(0).side, None);
    }

    #[test]
    fn draw_with_a_tool_then_undo_and_redo_with_keys_and_buttons() {
        let t0 = Instant::now();
        let mut m = selected_model(t0);
        use_tool(&mut m, Tool::Rect);
        assert_eq!(m.annot().tool(), Some(Tool::Rect));
        draw(&mut m, pt(200, 150), pt(400, 250), t0);
        draw(&mut m, pt(220, 160), pt(420, 260), t0);
        assert_eq!(m.annot().shapes().len(), 2);
        // Vùng chọn không đổi khi vẽ.
        assert_eq!(m.rect(), Some(px(100, 100, 400, 200)));
        // ⌘Z và ⇧⌘Z.
        assert_eq!(key(&mut m, 6, "z", CMD, t0), KeyOutcome::default());
        assert_eq!(m.annot().shapes().len(), 1);
        key(&mut m, 6, "z", CMD_SHIFT, t0);
        assert_eq!(m.annot().shapes().len(), 2);
        // Nút Hoàn tác / Làm lại của thanh ngang.
        let (_, bar) = m.bar().unwrap();
        for (item, want) in [(BarItem::Undo, 1), (BarItem::Undo, 0), (BarItem::Redo, 1)] {
            let c = bar_button_rect(bar, m.dpi(0), item).unwrap();
            let p = pt(c.x + c.w / 2, c.y + c.h / 2);
            m.press(0, p, t0);
            assert_eq!(m.release(p, t0), None, "không đóng lớp phủ");
            assert_eq!(m.annot().shapes().len(), want);
        }
    }

    #[test]
    fn drawing_outside_the_selection_creates_no_shape_and_handles_still_work() {
        let t0 = Instant::now();
        let mut m = selected_model(t0);
        use_tool(&mut m, Tool::Line);
        // Bấm xa vùng chọn: không tạo hình, không tạo vùng mới.
        draw(&mut m, pt(1500, 1500), pt(1800, 1700), t0);
        assert!(m.annot().shapes().is_empty());
        assert_eq!(m.rect(), Some(px(100, 100, 400, 200)));
        assert!(!m.is_dragging());
        // Tay nắm vùng chọn (góc dưới phải) vẫn đổi cỡ vùng dù đang có công cụ.
        draw(&mut m, pt(500, 300), pt(600, 400), t0);
        assert!(m.annot().shapes().is_empty());
        assert_eq!(m.rect(), Some(px(100, 100, 500, 300)));
        // Trong vùng chọn: vẽ.
        draw(&mut m, pt(200, 150), pt(400, 250), t0);
        assert_eq!(m.annot().shapes().len(), 1);
    }

    #[test]
    fn a_selection_cannot_be_redrawn_elsewhere_once_shapes_exist() {
        let t0 = Instant::now();
        let mut m = selected_model(t0);
        use_tool(&mut m, Tool::Rect);
        draw(&mut m, pt(200, 150), pt(400, 250), t0);
        key(&mut m, 15, "r", NONE, t0);
        assert_eq!(m.annot().tool(), None);
        // Không công cụ nhưng có hình: bấm ngoài vùng bị bỏ qua (cả trên màn hình kia).
        draw(&mut m, pt(1500, 1500), pt(1800, 1700), t0);
        assert_eq!(m.rect(), Some(px(100, 100, 400, 200)));
        m.press(1, pt(50, 50), t0);
        m.drag(pt(300, 300), t0);
        m.release(pt(300, 300), t0);
        assert_eq!(m.active(), Some(0));
        assert_eq!(m.annot().shapes().len(), 1);
        // Hoàn tác hết rồi thì chọn lại ở màn hình khác được; công cụ nhớ.
        key(&mut m, 6, "z", CMD, t0);
        m.press(1, pt(50, 50), t0);
        m.drag(pt(300, 300), t0);
        m.release(pt(300, 300), t0);
        assert_eq!(m.active(), Some(1));
        assert_eq!(m.rect(), Some(px(50, 50, 250, 250)));
    }

    #[test]
    fn esc_unwinds_layers_but_always_ends_in_cancel() {
        let t0 = Instant::now();
        let mut m = selected_model(t0);
        // Bảng màu mở: Esc đóng bảng (chưa hủy), Esc nữa hủy.
        click_side(&mut m, SideItem::Color, t0);
        assert_eq!(m.annot().flyout(), Flyout::Palette);
        assert_eq!(key(&mut m, KEY_ESCAPE, "\u{1b}", NONE, t0).action, None);
        assert_eq!(m.annot().flyout(), Flyout::None);
        assert_eq!(
            key(&mut m, KEY_ESCAPE, "\u{1b}", NONE, t0).action,
            Some(Action::Cancel)
        );
        // Đang gõ chữ: Esc hủy ô, Esc nữa hủy lớp phủ.
        click_side(&mut m, SideItem::Tool(Tool::Text), t0);
        click(&mut m, pt(200, 150), t0);
        assert!(m.annot().text().is_some());
        assert_eq!(key(&mut m, KEY_ESCAPE, "\u{1b}", NONE, t0).action, None);
        assert!(m.annot().text().is_none());
        assert_eq!(
            key(&mut m, KEY_ESCAPE, "\u{1b}", NONE, t0).action,
            Some(Action::Cancel)
        );
        // Đang kéo nét: Esc hủy lớp phủ ngay.
        click_side(&mut m, SideItem::Tool(Tool::Pen), t0);
        m.press(0, pt(200, 150), t0);
        m.drag(pt(250, 180), t0);
        assert_eq!(
            key(&mut m, KEY_ESCAPE, "\u{1b}", NONE, t0).action,
            Some(Action::Cancel)
        );
        // Đang chọn một hình rồi kéo nó: Esc hủy cử chỉ, rồi bỏ chọn, rồi hủy.
        let mut m = selected_model(t0);
        use_tool(&mut m, Tool::Rect);
        draw(&mut m, pt(200, 150), pt(400, 250), t0);
        click_side(&mut m, SideItem::Tool(Tool::Select), t0);
        m.press(0, pt(200, 200), t0);
        m.drag(pt(230, 220), t0);
        assert!(m.annot().capturing());
        let mut presses = 0;
        loop {
            presses += 1;
            assert!(presses < 8, "Esc phải luôn dẫn tới thoát");
            if key(&mut m, KEY_ESCAPE, "\u{1b}", NONE, t0).action == Some(Action::Cancel) {
                break;
            }
        }
        assert_eq!(presses, 3);
    }

    #[test]
    fn esc_while_the_input_method_composes_goes_to_it_then_forces_exit_of_the_box() {
        let t0 = Instant::now();
        let mut m = selected_model(t0);
        click_side(&mut m, SideItem::Tool(Tool::Text), t0);
        click(&mut m, pt(200, 150), t0);
        m.annot_mut().text_marked("a");
        assert!(m.annot().text_composing());
        // Esc đầu: chuyển cho bộ gõ (hủy chuỗi soạn của nó), chưa đóng ô.
        let o = key(&mut m, KEY_ESCAPE, "\u{1b}", NONE, t0);
        assert!(o.ime && o.action.is_none());
        assert!(m.annot().text().is_some());
        // Bộ gõ không trả Esc (vẫn đang soạn): Esc kế ép đóng ô, không kẹt.
        let o = key(&mut m, KEY_ESCAPE, "\u{1b}", NONE, t0);
        assert!(!o.ime && o.action.is_none());
        assert!(m.annot().text().is_none());
        assert_eq!(
            key(&mut m, KEY_ESCAPE, "\u{1b}", NONE, t0).action,
            Some(Action::Cancel)
        );
        // Bộ gõ đã hủy soạn sau Esc đầu: Esc kế đóng ô bình thường.
        click_side(&mut m, SideItem::Tool(Tool::Text), t0);
        click_side(&mut m, SideItem::Tool(Tool::Text), t0);
        click(&mut m, pt(200, 150), t0);
        m.annot_mut().text_marked("a");
        assert!(key(&mut m, KEY_ESCAPE, "\u{1b}", NONE, t0).ime);
        m.annot_mut().text_unmark();
        m.annot_mut().text_insert("");
        let o = key(&mut m, KEY_ESCAPE, "\u{1b}", NONE, t0);
        assert!(o.action.is_none() && !o.ime);
    }

    #[test]
    fn keys_while_typing_go_to_the_input_method_and_cmd_keys_still_act() {
        let t0 = Instant::now();
        let mut m = selected_model(t0);
        click_side(&mut m, SideItem::Tool(Tool::Text), t0);
        click(&mut m, pt(200, 150), t0);
        // Chữ cái, cả chữ của công cụ ('p'), đi vào bộ gõ chứ không đổi công cụ.
        let o = key(&mut m, 35, "p", NONE, t0);
        assert!(o.ime);
        assert_eq!(m.annot().tool(), Some(Tool::Text));
        // Mũi tên, Enter, Xóa cũng đi qua bộ gõ.
        assert!(key(&mut m, 123, "\u{f702}", NONE, t0).ime);
        assert!(key(&mut m, 36, "\r", NONE, t0).ime);
        // ⌘V: dán; ⌘S: lưu (chốt chữ ở bước hoàn tất); ⌘Z khi có chữ: xóa ô.
        assert!(key(&mut m, 9, "v", CMD, t0).paste);
        m.annot_mut().text_insert("abc");
        key(&mut m, 6, "z", CMD, t0);
        assert!(m.annot().text().is_none());
        click(&mut m, pt(200, 150), t0);
        m.annot_mut().text_insert("xyz");
        assert_eq!(key(&mut m, 1, "s", CMD, t0).action, Some(Action::Save));
        // Hoàn tất: chữ đang gõ vào cuối sổ rồi mới lấy để đốt vào ảnh.
        let shapes = m.take_shapes();
        assert!(matches!(shapes.last(), Some(Shape::Text { .. })));
        let _ = TextCmd::Newline;
    }

    #[test]
    fn tool_letters_sizes_and_delete_work_without_modifiers() {
        let t0 = Instant::now();
        let mut m = selected_model(t0);
        key(&mut m, 15, "r", NONE, t0);
        assert_eq!(m.annot().tool(), Some(Tool::Rect));
        // Phím lặp (giữ phím) không bật/tắt liên tục.
        m.key(15, Some("r"), NONE, true, t0);
        assert_eq!(m.annot().tool(), Some(Tool::Rect));
        draw(&mut m, pt(200, 150), pt(400, 250), t0);
        key(&mut m, 30, "]", NONE, t0);
        assert_eq!(m.annot().size(), darkshot_core::annotate::SizeLevel::Large);
        // Chọn hình rồi Xóa.
        key(&mut m, 9, "v", NONE, t0);
        m.press(0, pt(200, 170), t0);
        m.release(pt(200, 170), t0);
        assert_eq!(m.annot().selected(), Some(0));
        key(&mut m, KEY_DELETE, "\u{7f}", NONE, t0);
        assert!(m.annot().shapes().is_empty());
        // ⌘D/⌘S/⌘C vẫn là hành động, ⌘Z không bao giờ trả hành động.
        assert_eq!(key(&mut m, 2, "d", CMD, t0).action, Some(Action::Upload));
        assert_eq!(key(&mut m, 8, "c", CMD, t0).action, Some(Action::Copy));
        assert_eq!(key(&mut m, 6, "z", CMD, t0).action, None);
    }

    #[test]
    fn keys_do_nothing_while_a_stroke_is_held_except_esc() {
        let t0 = Instant::now();
        let mut m = selected_model(t0);
        click_side(&mut m, SideItem::Tool(Tool::Pen), t0);
        m.press(0, pt(200, 150), t0);
        m.drag(pt(250, 180), t0);
        assert!(m.annot().capturing());
        assert_eq!(key(&mut m, 1, "s", CMD, t0).action, None);
        key(&mut m, 6, "z", CMD, t0);
        key(&mut m, 15, "r", NONE, t0);
        assert_eq!(m.annot().tool(), Some(Tool::Pen));
        m.release(pt(300, 200), t0);
        assert_eq!(m.annot().shapes().len(), 1);
    }

    #[test]
    fn every_tool_interaction_counts_for_the_idle_safety_net() {
        let t0 = Instant::now();
        let mut m = selected_model(t0);
        click_side(&mut m, SideItem::Tool(Tool::Pen), t0);
        let t1 = t0 + Duration::from_secs(100);
        m.press(0, pt(200, 150), t1);
        let t2 = t1 + Duration::from_secs(100);
        m.drag(pt(250, 180), t2);
        assert!(!m.idle_expired(t2 + Duration::from_secs(119)));
        let t3 = t2 + Duration::from_secs(100);
        m.shift_changed(true, t3);
        assert!(!m.idle_expired(t3 + Duration::from_secs(119)));
        let t4 = t3 + Duration::from_secs(100);
        m.touch(t4);
        assert!(!m.idle_expired(t4 + Duration::from_secs(119)));
        assert!(m.idle_expired(t4 + IDLE_TIMEOUT));
    }

    #[test]
    fn shift_held_through_the_model_constrains_the_stroke() {
        let t0 = Instant::now();
        let mut m = selected_model(t0);
        use_tool(&mut m, Tool::Rect);
        m.press(0, pt(200, 150), t0);
        m.set_shift(true);
        m.drag(pt(400, 190), t0);
        m.release(pt(400, 190), t0);
        match &m.annot().shapes()[0] {
            Shape::Rect { a, b, .. } => {
                assert_eq!(b.x - a.x, b.y - a.y, "hình vuông khi giữ Shift");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn prefs_and_image_survive_to_the_end() {
        let t0 = Instant::now();
        let img = Rc::new(image(3024, 1964));
        let mut m = Model::with_images(vec![RETINA], vec![img], Prefs::default(), t0);
        m.press(0, pt(100, 100), t0);
        m.drag(pt(500, 300), t0);
        m.release(pt(500, 300), t0);
        use_tool(&mut m, Tool::Ellipse);
        assert_eq!(m.prefs().shape_tool, Tool::Ellipse);
        let out = m.take_image(0).unwrap();
        assert_eq!((out.width, out.height), (3024, 1964));
        assert!(out.is_valid());
    }

    #[test]
    fn a_missed_mouse_up_never_wedges_the_overlay() {
        let t0 = Instant::now();
        let mut m = selected_model(t0);
        click_side(&mut m, SideItem::Tool(Tool::Pen), t0);
        // Nét đang vẽ mà không có mouseUp (nhả ngoài cửa sổ).
        m.press(0, pt(200, 150), t0);
        m.drag(pt(250, 180), t0);
        assert!(m.annot().capturing());
        // Nhấn tiếp vào nút Hủy: nét cũ được chốt, nút hoạt động bình thường.
        let (_, bar) = m.bar().unwrap();
        let c = bar_button_rect(bar, m.dpi(0), BarItem::Act(Action::Cancel)).unwrap();
        let p = pt(c.x + c.w / 2, c.y + c.h / 2);
        m.press(0, p, t0);
        assert!(!m.annot().capturing());
        assert_eq!(m.annot().shapes().len(), 1);
        assert_eq!(m.release(p, t0), Some(Action::Cancel));
        // Mất key giữa lúc kéo: chốt nét.
        let mut m = selected_model(t0);
        click_side(&mut m, SideItem::Tool(Tool::Pen), t0);
        m.press(0, pt(200, 150), t0);
        m.drag(pt(250, 180), t0);
        m.lost_capture(t0);
        assert!(!m.annot().capturing());
        assert_eq!(m.annot().shapes().len(), 1);
    }

    #[test]
    fn hover_on_another_screen_clears_the_side_bar_highlight() {
        let t0 = Instant::now();
        let mut m = selected_model(t0);
        let (_, side) = m.side().unwrap();
        m.hover(0, pt(side.x + 2, side.y + 2), t0);
        assert!(m.annot().side_hover().is_some());
        m.hover(1, pt(side.x + 2, side.y + 2), t0);
        assert_eq!(m.annot().side_hover(), None);
    }

    #[test]
    fn tool_cursor_through_the_model() {
        use crate::annotate_ui::CursorKind;
        let t0 = Instant::now();
        let mut m = selected_model(t0);
        assert_eq!(m.cursor_at(0, pt(300, 200)), CursorKind::Move);
        assert_eq!(m.cursor_at(0, pt(1500, 1500)), CursorKind::Cross);
        click_side(&mut m, SideItem::Tool(Tool::Text), t0);
        assert_eq!(m.cursor_at(0, pt(300, 200)), CursorKind::IBeam);
        // Không có vùng chọn trên màn hình đó: chữ thập chọn vùng.
        assert_eq!(m.cursor_at(1, pt(10, 10)), CursorKind::Cross);
    }
}
