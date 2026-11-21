//! Phần thuần của lớp phủ chọn vùng macOS (Task 105): đổi điểm của sự kiện chuột sang pixel,
//! khớp ảnh chụp với `NSScreen`, máy trạng thái "màn hình đang chọn" bọc `Selection` của core,
//! thanh Lưu/Chép/Hủy, bảng phím, nhãn kích thước, lưới an toàn khi không có tương tác.
//!
//! Không gọi Objective-C: kiểm thử chạy được trên máy không có GUI. Mọi toạ độ của mô hình là
//! pixel của ảnh màn hình đang xét, gốc trên trái (như core).

use crate::coords::{cocoa_to_top_left, pt_to_px, RectF};
use darkshot_core::actions::Action;
use darkshot_core::geometry::{place_toolbar, Point, Rect, Selection};
use darkshot_core::i18n::Msg;
use darkshot_core::overlay_layout::{clamp_x, scaled, BUTTON, GAP, HANDLE_HIT};
use darkshot_core::snap;
use std::time::{Duration, Instant};

/// Lưới an toàn: không có tương tác (chuột, phím) trong chừng này thì lớp phủ tự hủy, để một lỗi
/// sự kiện không bao giờ khóa màn hình vô thời hạn.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(120);
/// Chu kỳ kiểm lưới an toàn.
pub const IDLE_CHECK_EVERY: Duration = Duration::from_secs(5);
/// `kVK_Escape`.
pub const KEY_ESCAPE: u16 = 53;
/// Các nút của thanh hành động, trái sang phải.
pub const MAC_BAR: [Action; 3] = [Action::Save, Action::Copy, Action::Cancel];
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

/// Với mỗi ảnh chụp (khung gốc trên trái, điểm), chỉ số `NSScreen` có cùng khung. `screens` là
/// khung Cocoa theo thứ tự `NSScreen.screens` (phần tử đầu là màn hình chính). Thiếu, thừa hay
/// trùng thì `None` (cấu hình màn hình đã đổi sau khi chụp).
pub fn match_screens(shots: &[RectF], screens: &[RectF]) -> Option<Vec<usize>> {
    if shots.is_empty() || shots.len() != screens.len() {
        return None;
    }
    let primary_h = screens[0].h;
    let tl: Vec<RectF> = screens
        .iter()
        .map(|s| cocoa_to_top_left(*s, primary_h))
        .collect();
    let same = |a: &RectF, b: &RectF| {
        (a.x - b.x).abs() <= FRAME_EPS
            && (a.y - b.y).abs() <= FRAME_EPS
            && (a.w - b.w).abs() <= FRAME_EPS
            && (a.h - b.h).abs() <= FRAME_EPS
    };
    let mut out = Vec::with_capacity(shots.len());
    for shot in shots {
        let i = tl.iter().position(|s| same(s, shot))?;
        if out.contains(&i) {
            return None;
        }
        out.push(i);
    }
    Some(out)
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

/// Cạnh một nút của thanh hành động (pixel).
pub fn bar_button_size(dpi: u32) -> i32 {
    scaled(BUTTON, dpi)
}

/// Thanh hành động cạnh vùng chọn `sel`, luôn nằm trong màn hình `mon`.
pub fn bar_rect(sel: Rect, dpi: u32, mon: Rect) -> Rect {
    let b = bar_button_size(dpi);
    let w = b.saturating_mul(MAC_BAR.len() as i32);
    place_toolbar(sel, (w, b), scaled(GAP, dpi), mon)
}

/// Ô của nút thứ `i` trên thanh `bar`.
pub fn bar_button_rect(bar: Rect, dpi: u32, i: usize) -> Rect {
    let b = bar_button_size(dpi);
    let i = i32::try_from(i).unwrap_or(i32::MAX);
    Rect {
        x: bar.x.saturating_add(b.saturating_mul(i)),
        y: bar.y,
        w: b,
        h: b,
    }
}

/// Nút của thanh `bar` dưới điểm `p`.
pub fn bar_action_at(bar: Rect, p: Point, dpi: u32) -> Option<Action> {
    if !bar.contains(p) {
        return None;
    }
    MAC_BAR
        .iter()
        .enumerate()
        .find(|(i, _)| bar_button_rect(bar, dpi, *i).contains(p))
        .map(|(_, a)| *a)
}

/// Gợi ý của một nút (phím tắt kiểu Mac).
pub fn bar_tip(a: Action) -> Msg {
    match a {
        Action::Save => Msg::OverlaySaveMac,
        Action::Copy => Msg::OverlayCopyMac,
        _ => Msg::OverlayCancelMac,
    }
}

/// Ô gợi ý của nút đang trỏ: dưới thanh nếu đủ chỗ, không thì trên; trong màn hình.
pub fn tip_rect(bar: Rect, text: (i32, i32), dpi: u32, mon: Rect) -> Rect {
    let w = text.0.max(0).saturating_add(2 * scaled(LABEL_PAD_X, dpi));
    let h = text.1.max(0).saturating_add(2 * scaled(LABEL_PAD_Y, dpi));
    place_toolbar(bar, (w.min(mon.w), h.min(mon.h)), scaled(GAP, dpi), mon)
}

/// Hành động của một phím: `Esc` luôn hủy; `⌘S` lưu, `⌘C` chép khi có vùng chọn và không đang
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
    /// Thanh hành động (khi có vùng chọn và không đang kéo).
    pub bar: Option<Rect>,
    pub hover: Option<Action>,
    pub pressed: Option<Action>,
    /// Viên nhắc (khi chưa có vùng chọn ở đâu cả).
    pub hint: bool,
}

impl ViewState {
    /// Không vẽ gì ngoài ảnh mờ (màn hình không tồn tại).
    pub fn default_hidden() -> ViewState {
        ViewState {
            rect: None,
            bar: None,
            hover: None,
            pressed: None,
            hint: false,
        }
    }
}

/// Trạng thái lớp phủ trên mọi màn hình. Chỉ một màn hình "đang chọn" giữ `Selection`; bấm sang
/// màn hình khác thì bỏ vùng cũ (vùng chọn không vắt qua màn hình, quyết định 135).
pub struct Model {
    screens: Vec<ScreenPx>,
    active: Option<usize>,
    sel: Selection,
    pressed: Option<Action>,
    hover: Option<Action>,
    last_input: Instant,
}

const EMPTY: Rect = Rect {
    x: 0,
    y: 0,
    w: 0,
    h: 0,
};

impl Model {
    pub fn new(screens: Vec<ScreenPx>, now: Instant) -> Model {
        Model {
            screens,
            active: None,
            sel: Selection::new(EMPTY),
            pressed: None,
            hover: None,
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

    /// Thanh hành động trên màn hình đang chọn.
    pub fn bar(&self) -> Option<(usize, Rect)> {
        if self.is_dragging() {
            return None;
        }
        let i = self.active?;
        let sel = self.rect()?;
        let mon = self.bounds(i)?;
        Some((i, bar_rect(sel, self.dpi(i), mon)))
    }

    pub fn view_state(&self, i: usize) -> ViewState {
        if i >= self.screens.len() {
            return ViewState::default_hidden();
        }
        let mine = self.active == Some(i);
        ViewState {
            rect: if mine { self.rect() } else { None },
            bar: self.bar().filter(|(s, _)| *s == i).map(|(_, b)| b),
            hover: if mine { self.hover } else { None },
            pressed: if mine { self.pressed } else { None },
            hint: self.rect().is_none(),
        }
    }

    /// Nhấn chuột trái ở `p` (pixel của màn hình `screen`).
    pub fn press(&mut self, screen: usize, p: Point, now: Instant) {
        self.last_input = now;
        let Some(bounds) = self.bounds(screen) else {
            return;
        };
        if let Some((s, bar)) = self.bar() {
            if s == screen && bar.contains(p) {
                self.pressed = bar_action_at(bar, p, self.dpi(s));
                return;
            }
        }
        self.pressed = None;
        if self.active != Some(screen) {
            self.active = Some(screen);
            self.sel = Selection::new(bounds);
            self.hover = None;
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
        self.sel.drag_to(p);
    }

    /// Nhả chuột ở `p`; trả hành động khi nhấn và nhả trên cùng một nút của thanh.
    pub fn release(&mut self, p: Point, now: Instant) -> Option<Action> {
        self.last_input = now;
        if let Some(a) = self.pressed.take() {
            let (s, bar) = self.bar()?;
            return (bar_action_at(bar, p, self.dpi(s)) == Some(a)).then_some(a);
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
        self.hover = self
            .bar()
            .filter(|(s, _)| *s == screen)
            .and_then(|(s, bar)| bar_action_at(bar, p, self.dpi(s)));
    }

    /// Một phím xuống; trả hành động cần làm.
    pub fn key(
        &mut self,
        key_code: u16,
        chars: Option<&str>,
        mods: Mods,
        now: Instant,
    ) -> Option<Action> {
        self.last_input = now;
        key_action(
            key_code,
            chars,
            mods,
            self.rect().is_some(),
            self.is_dragging(),
        )
    }

    /// Đã quá `IDLE_TIMEOUT` kể từ tương tác cuối.
    pub fn idle_expired(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.last_input) >= IDLE_TIMEOUT
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
    fn bar_is_three_buttons_from_core_constants() {
        assert_eq!(bar_button_size(96), BUTTON);
        assert_eq!(bar_button_size(192), BUTTON * 2);
        let mon = px(0, 0, 3024, 1964);
        let bar = bar_rect(px(100, 100, 400, 200), 192, mon);
        assert_eq!((bar.w, bar.h), (3 * BUTTON * 2, BUTTON * 2));
        // Đủ chỗ: dưới vùng, cách GAP, mép phải thẳng mép phải vùng.
        assert_eq!(bar.y, 300 + GAP * 2);
        assert_eq!(bar.right(), 500);
    }

    #[test]
    fn bar_stays_on_screen_at_corners_and_edges() {
        for (scale, mon) in [
            (2.0, px(0, 0, 3024, 1964)),
            (1.0, px(0, 0, 1920, 1080)),
            (1.0, px(0, 0, 200, 150)),
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
        let bar = px(1000, 500, 3 * b, b);
        for (i, a) in MAC_BAR.iter().enumerate() {
            let cell = bar_button_rect(bar, dpi, i);
            assert_eq!(cell, px(1000 + i as i32 * b, 500, b, b));
            assert_eq!(
                bar_action_at(bar, pt(cell.x + b / 2, cell.y + b / 2), dpi),
                Some(*a)
            );
            assert_eq!(bar_action_at(bar, pt(cell.x, cell.y), dpi), Some(*a));
        }
        assert_eq!(bar_action_at(bar, pt(999, 510), dpi), None);
        assert_eq!(bar_action_at(bar, pt(1000 + 3 * b, 510), dpi), None);
        assert_eq!(bar_action_at(bar, pt(1010, 500 + b), dpi), None);
    }

    #[test]
    fn mac_tips_use_mac_shortcuts() {
        assert_eq!(bar_tip(Action::Save), Msg::OverlaySaveMac);
        assert_eq!(bar_tip(Action::Copy), Msg::OverlayCopyMac);
        assert_eq!(bar_tip(Action::Cancel), Msg::OverlayCancelMac);
        for l in Lang::ALL {
            assert!(t(l, Msg::OverlaySaveMac).contains("(\u{2318}S)"));
            assert!(t(l, Msg::OverlayCopyMac).contains("(\u{2318}C)"));
            assert!(t(l, Msg::OverlayCancelMac).contains("(Esc)"));
            for m in [
                Msg::OverlaySaveMac,
                Msg::OverlayCopyMac,
                Msg::OverlayCancelMac,
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
        // 1x: cùng khoảng cách ngoài bán kính 6 px → bắt đầu vùng mới.
        m.press(1, pt(100, 100), t0);
        m.drag(pt(500, 300), t0);
        m.release(pt(500, 300), t0);
        m.press(1, pt(510, 310), t0);
        m.drag(pt(600, 400), t0);
        m.release(pt(600, 400), t0);
        assert_eq!(m.rect(), Some(px(510, 310, 90, 90)));
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
        let center = |i: usize| {
            let c = bar_button_rect(bar, dpi, i);
            pt(c.x + c.w / 2, c.y + c.h / 2)
        };
        for (i, a) in MAC_BAR.iter().enumerate() {
            m.press(0, center(i), t0);
            assert_eq!(m.view_state(0).pressed, Some(*a));
            // Nhấn nút không đổi vùng chọn.
            assert_eq!(m.rect(), Some(px(100, 100, 400, 200)));
            m.drag(pt(0, 0), t0);
            assert_eq!(m.rect(), Some(px(100, 100, 400, 200)));
            assert_eq!(m.release(center(i), t0), Some(*a));
            assert_eq!(m.view_state(0).pressed, None);
        }
        // Nhả ở nút khác hoặc ngoài thanh: không làm gì.
        m.press(0, center(0), t0);
        assert_eq!(m.release(center(1), t0), None);
        m.press(0, center(1), t0);
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
        let c = bar_button_rect(bar, m.dpi(0), 1);
        m.hover(0, pt(c.x + 1, c.y + 1), t0);
        assert_eq!(m.view_state(0).hover, Some(Action::Copy));
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
        assert_eq!(m.key(1, Some("s"), cmd, t0), None);
        assert_eq!(
            m.key(KEY_ESCAPE, None, Mods::default(), t0),
            Some(Action::Cancel)
        );
        m.press(0, pt(100, 100), t0);
        m.drag(pt(500, 300), t0);
        // Đang kéo: ⌘C không làm gì, Esc vẫn hủy.
        assert_eq!(m.key(8, Some("c"), cmd, t0), None);
        assert_eq!(
            m.key(KEY_ESCAPE, None, Mods::default(), t0),
            Some(Action::Cancel)
        );
        m.release(pt(500, 300), t0);
        assert_eq!(m.key(8, Some("c"), cmd, t0), Some(Action::Copy));
        assert_eq!(m.key(1, Some("s"), cmd, t0), Some(Action::Save));
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
        m.key(0, Some("a"), Mods::default(), t5);
        assert!(!m.idle_expired(t5 + Duration::from_secs(119)));
        assert!(m.idle_expired(t5 + IDLE_TIMEOUT));
        // Đồng hồ lùi (không thể với `Instant`, nhưng phòng hờ): không tính là hết hạn.
        assert!(!m.idle_expired(t0));
    }
}
