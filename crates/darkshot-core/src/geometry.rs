//! Hình học vùng chọn: khung chọn, tay nắm, vị trí thanh công cụ.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn from_points(a: Point, b: Point) -> Rect {
        Rect {
            x: a.x.min(b.x),
            y: a.y.min(b.y),
            w: (a.x - b.x).abs(),
            h: (a.y - b.y).abs(),
        }
    }

    /// Cạnh phải và cạnh dưới không tính.
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }

    pub fn right(&self) -> i32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Outside,
    Inside,
    Handle(Handle),
}

#[derive(Clone, Copy, Debug)]
enum Drag {
    New { anchor: Point },
    Move { start: Point, orig: Rect },
    Resize { handle: Handle, orig: Rect },
}

pub struct Selection {
    bounds: Rect,
    rect: Option<Rect>,
    drag: Option<Drag>,
}

/// Cạnh nhỏ nhất của một vùng chọn (pixel); nhỏ hơn thì coi như không có.
pub const MIN_SIZE: i32 = 3;

fn clamp_point(p: Point, b: Rect) -> Point {
    Point {
        x: p.x.clamp(b.x, b.right()),
        y: p.y.clamp(b.y, b.bottom()),
    }
}

impl Selection {
    pub fn new(bounds: Rect) -> Self {
        Selection {
            bounds,
            rect: None,
            drag: None,
        }
    }

    /// Vùng chọn có sẵn (khung đã nhớ). Khung không nằm trọn trong `bounds` hoặc nhỏ hơn
    /// `MIN_SIZE` thì coi như không có.
    pub fn with_rect(bounds: Rect, rect: Option<Rect>) -> Selection {
        let mut s = Selection::new(bounds);
        if let Some(r) = rect {
            let fits = r.w >= MIN_SIZE
                && r.h >= MIN_SIZE
                && r.x >= bounds.x
                && r.y >= bounds.y
                && i64::from(r.x) + i64::from(r.w) <= i64::from(bounds.x) + i64::from(bounds.w)
                && i64::from(r.y) + i64::from(r.h) <= i64::from(bounds.y) + i64::from(bounds.h);
            if fits {
                s.rect = Some(r);
            }
        }
        s
    }

    pub fn rect(&self) -> Option<Rect> {
        self.rect
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    pub fn hit_test(&self, p: Point, handle_radius: i32) -> Hit {
        let Some(r) = self.rect else {
            return Hit::Outside;
        };
        let rad = handle_radius;
        let near = |v: i32, edge: i32| (v - edge).abs() <= rad;
        let in_x = p.x >= r.x - rad && p.x <= r.right() + rad;
        let in_y = p.y >= r.y - rad && p.y <= r.bottom() + rad;
        if in_x && in_y {
            let w = near(p.x, r.x);
            let e = near(p.x, r.right());
            let n = near(p.y, r.y);
            let s = near(p.y, r.bottom());
            let h = match (n, s, w, e) {
                (true, _, true, _) => Some(Handle::NW),
                (true, _, _, true) => Some(Handle::NE),
                (_, true, true, _) => Some(Handle::SW),
                (_, true, _, true) => Some(Handle::SE),
                (true, ..) => Some(Handle::N),
                (_, true, ..) => Some(Handle::S),
                (_, _, true, _) => Some(Handle::W),
                (_, _, _, true) => Some(Handle::E),
                _ => None,
            };
            if let Some(h) = h {
                return Hit::Handle(h);
            }
        }
        if r.contains(p) {
            Hit::Inside
        } else {
            Hit::Outside
        }
    }

    pub fn press(&mut self, p: Point, handle_radius: i32) {
        let p = clamp_point(p, self.bounds);
        self.drag = Some(match (self.hit_test(p, handle_radius), self.rect) {
            (Hit::Handle(handle), Some(orig)) => Drag::Resize { handle, orig },
            (Hit::Inside, Some(orig)) => Drag::Move { start: p, orig },
            _ => {
                self.rect = Some(Rect::from_points(p, p));
                Drag::New { anchor: p }
            }
        });
    }

    pub fn drag_to(&mut self, p: Point) {
        let Some(drag) = self.drag else { return };
        let b = self.bounds;
        let p = clamp_point(p, b);
        self.rect = Some(match drag {
            Drag::New { anchor } => Rect::from_points(anchor, p),
            Drag::Move { start, orig } => {
                let max_x = (b.right() - orig.w).max(b.x);
                let max_y = (b.bottom() - orig.h).max(b.y);
                Rect {
                    x: (orig.x + p.x - start.x).clamp(b.x, max_x),
                    y: (orig.y + p.y - start.y).clamp(b.y, max_y),
                    ..orig
                }
            }
            Drag::Resize { handle, orig } => {
                let (mut l, mut t, mut r, mut bt) = (orig.x, orig.y, orig.right(), orig.bottom());
                use Handle::*;
                if matches!(handle, W | NW | SW) {
                    l = p.x;
                }
                if matches!(handle, E | NE | SE) {
                    r = p.x;
                }
                if matches!(handle, N | NW | NE) {
                    t = p.y;
                }
                if matches!(handle, S | SW | SE) {
                    bt = p.y;
                }
                Rect::from_points(Point { x: l, y: t }, Point { x: r, y: bt })
            }
        });
    }

    pub fn release(&mut self) {
        self.drag = None;
        if let Some(r) = self.rect {
            if r.w < MIN_SIZE || r.h < MIN_SIZE {
                self.rect = None;
            }
        }
    }
}

/// Đổi khung đã nhớ (tọa độ màn hình) sang tọa độ ảnh có gốc `origin`; `None` khi khung không
/// còn dùng được. `monitors` ở **tọa độ màn hình**, cùng hệ với `remembered` (không phải tọa độ
/// ảnh như `Screenshot::monitors`): hàm tự trừ `origin` cho cả hai.
pub fn restore_selection(
    remembered: Rect,
    origin: Point,
    image_w: i32,
    image_h: i32,
    monitors: &[Rect],
) -> Option<Rect> {
    if remembered.w <= 0 || remembered.h <= 0 {
        return None;
    }
    let x = i64::from(remembered.x) - i64::from(origin.x);
    let y = i64::from(remembered.y) - i64::from(origin.y);
    let (r, b) = (x + i64::from(remembered.w), y + i64::from(remembered.h));
    if x < 0 || y < 0 || r > i64::from(image_w) || b > i64::from(image_h) {
        return None;
    }
    // Từ đây x, y, r, b nằm trong phạm vi i32 vì ảnh nằm trong i32.
    let touches = monitors.iter().any(|m| {
        let mx = i64::from(m.x) - i64::from(origin.x);
        let my = i64::from(m.y) - i64::from(origin.y);
        let (mr, mb) = (mx + i64::from(m.w), my + i64::from(m.h));
        x.max(mx) < r.min(mr) && y.max(my) < b.min(mb)
    });
    if !touches {
        return None;
    }
    Some(Rect {
        x: i32::try_from(x).ok()?,
        y: i32::try_from(y).ok()?,
        w: remembered.w,
        h: remembered.h,
    })
}

/// Màn hình chứa `p`; nếu không có thì màn hình gần nhất.
pub fn monitor_containing(p: Point, monitors: &[Rect]) -> Rect {
    let dist = |m: &Rect| -> i64 {
        let dx = (m.x - p.x).max(p.x - (m.right() - 1)).max(0) as i64;
        let dy = (m.y - p.y).max(p.y - (m.bottom() - 1)).max(0) as i64;
        dx * dx + dy * dy
    };
    monitors
        .iter()
        .min_by_key(|m| dist(m))
        .copied()
        .unwrap_or(Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        })
}

pub fn place_toolbar(sel: Rect, size: (i32, i32), gap: i32, monitor: Rect) -> Rect {
    let (w, h) = size;
    let mut x = sel.right() - w;
    let below = sel.bottom() + gap;
    let above = sel.y - gap - h;
    let mut y = if below + h <= monitor.bottom() {
        below
    } else if above >= monitor.y {
        above
    } else {
        x = sel.right() - gap - w;
        sel.bottom() - gap - h
    };
    x = x.min(monitor.right() - w).max(monitor.x);
    y = y.min(monitor.bottom() - h).max(monitor.y);
    Rect { x, y, w, h }
}

/// Thanh dọc cạnh vùng chọn: phải (cách gap, mép dưới thẳng sel.bottom()), không đủ thì trái,
/// không đủ cả hai thì trong vùng sát mép phải; kẹp vào monitor; nếu giao `avoid` thì đặt mép
/// dưới cách avoid.y một gap (kẹp lại vào monitor); vẫn giao thì theo `clear_of`.
pub fn place_side_toolbar(
    sel: Rect,
    size: (i32, i32),
    gap: i32,
    monitor: Rect,
    avoid: Option<Rect>,
) -> Rect {
    let (w, h) = size;
    let right_x = sel.right().saturating_add(gap);
    let left_x = sel.x.saturating_sub(gap).saturating_sub(w);
    let x = if right_x.saturating_add(w) <= monitor.right() {
        right_x
    } else if left_x >= monitor.x {
        left_x
    } else {
        sel.right().saturating_sub(gap).saturating_sub(w)
    };
    let clamp_x = |x: i32| x.min(monitor.right() - w).max(monitor.x);
    let clamp_y = |y: i32| y.min(monitor.bottom() - h).max(monitor.y);
    let x = clamp_x(x);
    let mut y = clamp_y(sel.bottom() - h);
    if let Some(a) = avoid {
        let overlaps = x < a.right() && a.x < x + w && y < a.bottom() && a.y < y + h;
        if overlaps {
            y = clamp_y(a.y - gap - h);
            let pushed = Rect { x, y, w, h };
            if intersects(pushed, a) {
                return clear_of(sel, (w, h), gap, monitor, a, x, clamp_y(sel.bottom() - h))
                    .unwrap_or(pushed);
            }
        }
    }
    Rect { x, y, w, h }
}

fn intersects(a: Rect, b: Rect) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

/// Bình phương khoảng cách giữa hai hình chữ nhật (0 khi chạm hoặc giao).
fn rect_gap2(a: Rect, b: Rect) -> i64 {
    let dx = i64::from((a.x - b.right()).max(b.x - a.right()).max(0));
    let dy = i64::from((a.y - b.bottom()).max(b.y - a.bottom()).max(0));
    dx * dx + dy * dy
}

/// Chỗ cho thanh dọc khi đẩy lên trên `avoid` vẫn giao: trong các ứng viên nằm trọn trong
/// `monitor` và không giao `avoid`, chọn cái gần `sel` nhất (bằng nhau thì theo thứ tự): dưới
/// `avoid` (giữ `x`), bên phải `avoid`, bên trái `avoid`, bên trái `sel` (ba cái sau giữ `y`).
/// Mọi ứng viên cách `avoid` một `gap`. `None` khi không ứng viên nào hợp lệ.
fn clear_of(
    sel: Rect,
    size: (i32, i32),
    gap: i32,
    monitor: Rect,
    avoid: Rect,
    x: i32,
    y: i32,
) -> Option<Rect> {
    let (w, h) = size;
    let a = avoid;
    let candidates = [
        (x, a.bottom().saturating_add(gap)),
        (a.right().saturating_add(gap), y),
        (a.x.saturating_sub(gap).saturating_sub(w), y),
        (sel.x.saturating_sub(gap).saturating_sub(w), y),
    ];
    candidates
        .into_iter()
        .map(|(x, y)| Rect { x, y, w, h })
        .filter(|r| {
            r.x >= monitor.x
                && r.y >= monitor.y
                && r.right() <= monitor.right()
                && r.bottom() <= monitor.bottom()
                && !intersects(*r, a)
        })
        .min_by_key(|r| rect_gap2(*r, sel))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SideLayout {
    pub size: (i32, i32),
    /// Ô của từng mục, tương đối góc trên trái thanh.
    pub items: Vec<Rect>,
    /// Ô vạch ngăn (cao `sep`, rộng `b`), tương đối góc trên trái thanh.
    pub seps: Vec<Rect>,
}

/// Bố cục thanh dọc.
pub fn side_layout(n: usize, seps: &[usize], b: i32, sep: i32, max_h: i32) -> SideLayout {
    let b = b.max(0);
    let sep = sep.max(0);
    let n_i = i32::try_from(n).unwrap_or(i32::MAX);
    let total = n_i.saturating_mul(b).saturating_add(
        i32::try_from(seps.len())
            .unwrap_or(i32::MAX)
            .saturating_mul(sep),
    );
    let mut items = Vec::with_capacity(n);
    let mut sep_rects = Vec::new();
    // Xếp một cột các mục [from, to) tại x; `skip_first_sep` bỏ vạch ở đầu cột. Trả chiều cao cột.
    let mut column = |from: usize, to: usize, x: i32, skip_first_sep: bool| -> i32 {
        let mut y = 0i32;
        for i in from..to {
            if seps.contains(&i) && !(skip_first_sep && i == from) {
                sep_rects.push(Rect { x, y, w: b, h: sep });
                y = y.saturating_add(sep);
            }
            items.push(Rect { x, y, w: b, h: b });
            y = y.saturating_add(b);
        }
        y
    };
    if total <= max_h {
        let h = column(0, n, 0, false);
        return SideLayout {
            size: (b, h),
            items,
            seps: sep_rects,
        };
    }
    let k = n.div_ceil(2);
    let left = column(0, k, 0, true);
    let right = column(k, n, b, true);
    SideLayout {
        size: (b.saturating_mul(2), left.max(right)),
        items,
        seps: sep_rects,
    }
}

/// Đổi một hình chữ nhật từ tọa độ màn hình sang tọa độ ảnh của màn hình ảo.
pub fn to_image_coords(screen: Rect, origin: Point) -> Rect {
    Rect {
        x: screen.x - origin.x,
        y: screen.y - origin.y,
        ..screen
    }
}

/// Đổi một hình chữ nhật từ tọa độ ảnh của màn hình ảo sang tọa độ màn hình (ngược của
/// `to_image_coords`).
pub fn to_screen_coords(image: Rect, origin: Point) -> Rect {
    Rect {
        x: image.x + origin.x,
        y: image.y + origin.y,
        ..image
    }
}

/// Góc trên trái để vẽ con trỏ vào ảnh: vị trí con trỏ (tọa độ màn hình) trừ điểm nóng,
/// rồi đổi sang tọa độ ảnh của màn hình ảo có gốc `origin`.
pub fn cursor_draw_origin(pos: Point, hotspot: Point, origin: Point) -> Point {
    Point {
        x: pos.x - hotspot.x - origin.x,
        y: pos.y - hotspot.y - origin.y,
    }
}

/// Phần giao của `r` với ảnh `w`×`h`; rỗng thì `None`.
pub fn clip_to_image(r: Rect, w: i32, h: i32) -> Option<Rect> {
    let x0 = r.x.max(0);
    let y0 = r.y.max(0);
    let x1 = r.right().min(w);
    let y1 = r.bottom().min(h);
    (x1 > x0 && y1 > y0).then_some(Rect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    })
}

/// Vùng (tọa độ ảnh) của lượt chụp toàn màn hình: màn hình chứa con trỏ (`cursor`, tọa độ màn
/// hình), không chứa thì màn hình gần nhất; vùng rỗng thì cả ảnh `w`×`h`.
pub fn full_screen_rect(cursor: Point, origin: Point, monitors: &[Rect], w: i32, h: i32) -> Rect {
    let p = Point {
        x: cursor.x - origin.x,
        y: cursor.y - origin.y,
    };
    let m = monitor_containing(p, monitors);
    if m.w <= 0 || m.h <= 0 {
        Rect { x: 0, y: 0, w, h }
    } else {
        m
    }
}

#[cfg(test)]
mod side_layout_tests {
    use super::*;

    #[test]
    fn side_layout_single_column() {
        let l = side_layout(14, &[1, 10, 12], 36, 9, 1000);
        assert_eq!(l.size, (36, 531));
        assert_eq!(l.items.len(), 14);
        assert_eq!(l.items[1].y, 36 + 9);
        assert_eq!(l.seps.len(), 3);
    }

    #[test]
    fn side_layout_wraps_when_short() {
        let l = side_layout(14, &[1, 10, 12], 36, 9, 500);
        assert_eq!(l.size, (72, 270));
        assert_eq!(l.items[7].x, 36);
        assert_eq!(l.items[7].y, 0);
        assert!(l.seps.iter().all(|s| s.y >= 0));
        for r in l.items.iter().chain(l.seps.iter()) {
            assert!(r.x >= 0 && r.y >= 0 && r.right() <= 72 && r.bottom() <= 270);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: Rect = Rect {
        x: 0,
        y: 0,
        w: 1000,
        h: 800,
    };
    fn p(x: i32, y: i32) -> Point {
        Point { x, y }
    }
    fn sel(x: i32, y: i32, w: i32, h: i32) -> Selection {
        let mut s = Selection::new(B);
        s.press(p(x, y), 6);
        s.drag_to(p(x + w, y + h));
        s.release();
        s
    }

    #[test]
    fn new_selection_from_drag() {
        let mut s = Selection::new(B);
        s.press(p(300, 200), 6);
        s.drag_to(p(100, 50));
        s.release();
        assert_eq!(
            s.rect(),
            Some(Rect {
                x: 100,
                y: 50,
                w: 200,
                h: 150
            })
        );
    }
    #[test]
    fn tiny_selection_is_discarded() {
        let mut s = Selection::new(B);
        s.press(p(10, 10), 6);
        s.drag_to(p(12, 12));
        s.release();
        assert_eq!(s.rect(), None);
    }
    #[test]
    fn drag_is_clamped_to_bounds() {
        let mut s = Selection::new(B);
        s.press(p(900, 700), 6);
        s.drag_to(p(5000, -40));
        s.release();
        assert_eq!(
            s.rect(),
            Some(Rect {
                x: 900,
                y: 0,
                w: 100,
                h: 700
            })
        );
    }
    #[test]
    fn move_keeps_size_and_stays_inside() {
        let mut s = sel(100, 100, 200, 100);
        s.press(p(150, 150), 6);
        s.drag_to(p(2000, 2000));
        s.release();
        assert_eq!(
            s.rect(),
            Some(Rect {
                x: 800,
                y: 700,
                w: 200,
                h: 100
            })
        );
    }
    #[test]
    fn resize_east_handle_and_flip() {
        let mut s = sel(100, 100, 200, 100);
        assert_eq!(s.hit_test(p(300, 150), 6), Hit::Handle(Handle::E));
        s.press(p(300, 150), 6);
        s.drag_to(p(40, 150));
        s.release();
        assert_eq!(
            s.rect(),
            Some(Rect {
                x: 40,
                y: 100,
                w: 60,
                h: 100
            })
        );
    }
    #[test]
    fn press_outside_starts_new_selection() {
        let mut s = sel(100, 100, 200, 100);
        s.press(p(500, 500), 6);
        s.drag_to(p(600, 650));
        s.release();
        assert_eq!(
            s.rect(),
            Some(Rect {
                x: 500,
                y: 500,
                w: 100,
                h: 150
            })
        );
    }
    #[test]
    fn hit_test_corners_inside_outside() {
        let s = sel(100, 100, 200, 100);
        assert_eq!(s.hit_test(p(100, 100), 6), Hit::Handle(Handle::NW));
        assert_eq!(s.hit_test(p(200, 150), 6), Hit::Inside);
        assert_eq!(s.hit_test(p(50, 50), 6), Hit::Outside);
    }
    #[test]
    fn toolbar_below_right_aligned() {
        assert_eq!(
            place_toolbar(
                Rect {
                    x: 100,
                    y: 100,
                    w: 300,
                    h: 200
                },
                (160, 40),
                6,
                B
            ),
            Rect {
                x: 240,
                y: 306,
                w: 160,
                h: 40
            }
        );
    }
    #[test]
    fn toolbar_above_when_no_room_below() {
        assert_eq!(
            place_toolbar(
                Rect {
                    x: 100,
                    y: 500,
                    w: 300,
                    h: 290
                },
                (160, 40),
                6,
                B
            ),
            Rect {
                x: 240,
                y: 454,
                w: 160,
                h: 40
            }
        );
    }
    #[test]
    fn toolbar_inside_when_selection_fills_monitor() {
        assert_eq!(
            place_toolbar(B, (160, 40), 6, B),
            Rect {
                x: 834,
                y: 754,
                w: 160,
                h: 40
            }
        );
    }
    #[test]
    fn toolbar_clamped_horizontally_for_narrow_selection() {
        let r = place_toolbar(
            Rect {
                x: 10,
                y: 100,
                w: 50,
                h: 50,
            },
            (160, 40),
            6,
            B,
        );
        assert_eq!(
            r,
            Rect {
                x: 0,
                y: 156,
                w: 160,
                h: 40
            }
        );
    }
    #[test]
    fn toolbar_stays_in_second_monitor_with_offset_origin() {
        let m2 = Rect {
            x: 1000,
            y: 200,
            w: 800,
            h: 600,
        };
        let r = place_toolbar(
            Rect {
                x: 1100,
                y: 600,
                w: 300,
                h: 190,
            },
            (160, 40),
            6,
            m2,
        );
        assert_eq!(
            r,
            Rect {
                x: 1240,
                y: 554,
                w: 160,
                h: 40
            }
        );
    }
    #[test]
    fn monitor_containing_picks_nearest_when_in_gap() {
        let ms = [
            Rect {
                x: 0,
                y: 0,
                w: 1000,
                h: 800,
            },
            Rect {
                x: 1000,
                y: 200,
                w: 800,
                h: 600,
            },
        ];
        assert_eq!(monitor_containing(p(1500, 300), &ms), ms[1]);
        assert_eq!(monitor_containing(p(1500, 50), &ms), ms[1]);
    }

    #[test]
    fn with_rect_starts_selected() {
        let b = Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        };
        let r = Rect {
            x: 10,
            y: 20,
            w: 30,
            h: 40,
        };
        let s = Selection::with_rect(b, Some(r));
        assert_eq!(s.rect(), Some(r));
        assert!(!s.is_dragging());
        assert_eq!(s.hit_test(p(20, 30), 4), Hit::Inside);
        assert_eq!(Selection::with_rect(b, None).rect(), None);
        let over = Rect {
            x: 90,
            y: 0,
            w: 30,
            h: 10,
        };
        assert_eq!(Selection::with_rect(b, Some(over)).rect(), None);
        let tiny = Rect {
            x: 0,
            y: 0,
            w: 1,
            h: 50,
        };
        assert_eq!(Selection::with_rect(b, Some(tiny)).rect(), None);
    }

    #[test]
    fn restore_selection_left_monitor_in_screen_coords() {
        // Màn hình phụ bên trái; `mons` theo tọa độ màn hình, cùng hệ với khung nhớ.
        let mons = [
            Rect {
                x: -1280,
                y: 0,
                w: 1280,
                h: 1024,
            },
            Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            },
        ];
        let o = p(-1280, 0);
        let on_left = Rect {
            x: -1000,
            y: 100,
            w: 300,
            h: 200,
        };
        assert_eq!(
            restore_selection(on_left, o, 3200, 1080, &mons),
            Some(Rect {
                x: 280,
                y: 100,
                w: 300,
                h: 200
            })
        );
        let on_main = Rect {
            x: 100,
            y: 100,
            w: 300,
            h: 200,
        };
        assert_eq!(
            restore_selection(on_main, o, 3200, 1080, &mons),
            Some(Rect { x: 1380, ..on_main })
        );
        // màn hình trái đã rút: gốc về (0, 0), khung cũ nằm ngoài ảnh
        assert_eq!(
            restore_selection(on_left, p(0, 0), 1920, 1080, &mons[1..]),
            None
        );
    }

    #[test]
    fn restore_selection_cases() {
        let mons = [
            Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            },
            Rect {
                x: 1920,
                y: 0,
                w: 1280,
                h: 1024,
            },
        ];
        let o = p(0, 0);
        let r = Rect {
            x: 100,
            y: 100,
            w: 300,
            h: 200,
        };
        assert_eq!(restore_selection(r, o, 3200, 1080, &mons), Some(r));
        // gốc âm: chỉ kiểm phép trừ gốc; `mons` ở đây không khớp bố cục có màn hình bên trái
        // (ca đúng bố cục, `mons` theo tọa độ màn hình: `restore_selection_left_monitor_in_screen_coords`)
        assert_eq!(
            restore_selection(r, p(-1280, 0), 3200, 1080, &mons),
            Some(Rect { x: 1380, ..r })
        );
        // màn hình phải đã rút: khung nằm ngoài ảnh
        let far = Rect {
            x: 2000,
            y: 100,
            w: 300,
            h: 200,
        };
        assert_eq!(restore_selection(far, o, 1920, 1080, &mons[..1]), None);
        // trong ảnh nhưng ở vùng chết
        let dead = Rect {
            x: 2000,
            y: 1040,
            w: 100,
            h: 30,
        };
        assert_eq!(restore_selection(dead, o, 3200, 1080, &mons), None);
        // tràn một phần; rỗng; giá trị cực lớn không panic
        let part = Rect {
            x: 1800,
            y: 900,
            w: 300,
            h: 300,
        };
        assert_eq!(restore_selection(part, o, 1920, 1080, &mons[..1]), None);
        let empty = Rect {
            x: 5,
            y: 5,
            w: 0,
            h: 10,
        };
        assert_eq!(restore_selection(empty, o, 1920, 1080, &mons[..1]), None);
        let huge = Rect {
            x: i32::MAX,
            y: i32::MIN,
            w: i32::MAX,
            h: i32::MAX,
        };
        let o3 = p(i32::MIN, i32::MAX);
        assert_eq!(restore_selection(huge, o3, 1920, 1080, &mons[..1]), None);
    }

    const MON: Rect = Rect {
        x: 0,
        y: 0,
        w: 1000,
        h: 800,
    };
    fn rc(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn side_toolbar_placement() {
        let place = place_side_toolbar;
        let size = (40, 200);
        // bên phải
        let sel = rc(100, 100, 300, 200);
        assert_eq!(place(sel, size, 6, MON, None), rc(406, 100, 40, 200));
        // sát mép phải màn hình: bên trái
        assert_eq!(
            place(rc(100, 100, 880, 200), size, 6, MON, None),
            rc(54, 100, 40, 200)
        );
        // kín màn hình: bên trong, sát mép phải
        assert_eq!(place(MON, size, 6, MON, None), rc(954, 600, 40, 200));
        // kẹp dọc
        assert_eq!(
            place(rc(100, 0, 300, 150), size, 6, MON, None),
            rc(406, 0, 40, 200)
        );
        // tránh avoid
        let avoid = rc(400, 350, 100, 40);
        assert_eq!(
            place(rc(100, 100, 300, 300), size, 6, MON, Some(avoid)),
            rc(406, 144, 40, 200)
        );
        // không giao thì giữ nguyên
        let far = rc(0, 700, 50, 40);
        assert_eq!(
            place(rc(100, 100, 300, 300), size, 6, MON, Some(far)),
            rc(406, 200, 40, 200)
        );
        // màn hình thứ hai
        let m2 = rc(1000, 200, 800, 600);
        assert_eq!(
            place(rc(1100, 300, 300, 300), size, 6, m2, None),
            rc(1406, 400, 40, 200)
        );
        assert_eq!(
            place(rc(1500, 300, 290, 300), size, 6, m2, None),
            rc(1454, 400, 40, 200)
        );
        // gốc âm
        let m3 = rc(-1000, 0, 1000, 800);
        assert_eq!(
            place(rc(-500, 100, 300, 300), size, 6, m3, None),
            rc(-194, 200, 40, 200)
        );
    }

    /// Đẩy lên trên thanh bốn nút vẫn giao (vùng chọn sát mép trên, hẹp hơn thanh bốn nút):
    /// đặt ở chỗ không giao, trọn trong màn hình, gần vùng chọn nhất.
    #[test]
    fn side_toolbar_leaves_avoid_when_push_up_fails() {
        let place = place_side_toolbar;
        let disjoint = |a: Rect, b: Rect| {
            a.right() <= b.x || b.right() <= a.x || a.bottom() <= b.y || b.bottom() <= a.y
        };
        // Dưới thanh bốn nút còn chỗ và gần vùng hơn: xuống dưới thanh.
        let sel = rc(0, 0, 50, 300);
        let bar = place_toolbar(sel, (160, 40), 6, MON);
        assert_eq!(bar, rc(0, 306, 160, 40));
        let side = place(sel, (40, 400), 6, MON, Some(bar));
        assert_eq!(side, rc(56, 352, 40, 400));
        assert!(disjoint(side, bar));
        // Dưới không đủ chỗ: sang phải mép phải thanh bốn nút, cách gap.
        let sel = rc(0, 0, 50, 380);
        let bar = place_toolbar(sel, (160, 40), 6, MON);
        assert_eq!(bar, rc(0, 386, 160, 40));
        let side = place(sel, (40, 500), 6, MON, Some(bar));
        assert_eq!(side, rc(166, 0, 40, 500));
        assert!(disjoint(side, bar));
        // Đẩy lên được thì giữ như cũ.
        let avoid = rc(400, 350, 100, 40);
        assert_eq!(
            place(rc(100, 100, 300, 300), (40, 200), 6, MON, Some(avoid)),
            rc(406, 144, 40, 200)
        );
    }

    #[test]
    fn image_coords_with_negative_origin() {
        let r = to_image_coords(
            Rect {
                x: -1920,
                y: -200,
                w: 1920,
                h: 1080,
            },
            Point { x: -1920, y: -200 },
        );
        assert_eq!(
            r,
            Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080
            }
        );
        let r = to_image_coords(
            Rect {
                x: 0,
                y: 0,
                w: 2560,
                h: 1440,
            },
            Point { x: -1920, y: -200 },
        );
        assert_eq!(
            r,
            Rect {
                x: 1920,
                y: 200,
                w: 2560,
                h: 1440
            }
        );
    }

    #[test]
    fn screen_and_image_coords_are_inverse() {
        let o = Point { x: -1280, y: 40 };
        let r = Rect {
            x: 10,
            y: 20,
            w: 30,
            h: 40,
        };
        assert_eq!(
            to_screen_coords(r, o),
            Rect {
                x: -1270,
                y: 60,
                w: 30,
                h: 40
            }
        );
        assert_eq!(to_image_coords(to_screen_coords(r, o), o), r);
    }

    #[test]
    fn cursor_origin_subtracts_hotspot_and_screen_origin() {
        assert_eq!(
            cursor_draw_origin(
                Point { x: 100, y: 50 },
                Point { x: 0, y: 0 },
                Point { x: 0, y: 0 }
            ),
            Point { x: 100, y: 50 }
        );
        // màn hình phụ bên trái: gốc âm; con trỏ chữ I có điểm nóng ở giữa
        assert_eq!(
            cursor_draw_origin(
                Point { x: -200, y: 10 },
                Point { x: 8, y: 12 },
                Point { x: -1280, y: 0 }
            ),
            Point { x: 1072, y: -2 }
        );
    }

    #[test]
    fn full_screen_takes_the_monitor_under_the_cursor() {
        // Màn hình phụ 1280×1024 bên trái, chính 1920×1080; gốc màn hình ảo (-1280, 0).
        let o = Point { x: -1280, y: 0 };
        let left = Rect {
            x: 0,
            y: 0,
            w: 1280,
            h: 1024,
        };
        let main = Rect {
            x: 1280,
            y: 0,
            w: 1920,
            h: 1080,
        };
        let mons = [left, main];
        assert_eq!(
            full_screen_rect(Point { x: -5, y: 500 }, o, &mons, 3200, 1080),
            left
        );
        assert_eq!(
            full_screen_rect(Point { x: 0, y: 0 }, o, &mons, 3200, 1080),
            main
        );
        // vùng chết dưới màn hình trái: màn hình gần nhất
        assert_eq!(
            full_screen_rect(Point { x: -100, y: 1060 }, o, &mons, 3200, 1080),
            left
        );
        // không có màn hình hợp lệ: cả ảnh
        let empty = [Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        }];
        assert_eq!(
            full_screen_rect(Point { x: 0, y: 0 }, o, &empty, 3200, 1080),
            Rect {
                x: 0,
                y: 0,
                w: 3200,
                h: 1080
            }
        );
    }

    #[test]
    fn clip_to_image_keeps_the_part_inside() {
        let r = |x, y, w, h| Rect { x, y, w, h };
        assert_eq!(
            clip_to_image(r(10, 10, 20, 20), 100, 100),
            Some(r(10, 10, 20, 20))
        );
        assert_eq!(
            clip_to_image(r(-5, 90, 20, 20), 100, 100),
            Some(r(0, 90, 15, 10))
        );
        assert_eq!(clip_to_image(r(100, 0, 10, 10), 100, 100), None);
        assert_eq!(clip_to_image(r(-30, 0, 10, 10), 100, 100), None);
    }
}
