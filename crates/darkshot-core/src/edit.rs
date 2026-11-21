//! Chỉnh sửa hình đã vẽ: trúng hình, tay nắm, biến đổi, đổi kiểu.
//! Thuần logic, không phụ thuộc hệ điều hành.

use crate::annotate::{
    arrow_geometry, arrow_min_head, blur_pixel_rect, constrain_45, dip_to_px, redact_color, Pt,
    Shape, SizeLevel, MIN_DRAG_PX, STEP_RADIUS_MAX_PX, STEP_RADIUS_MIN_PX, TEXT_SIZE_MAX_PX,
    TEXT_SIZE_MIN_PX,
};
use crate::geometry::{Handle, Rect};
use crate::image::Image;
use crate::theme::Rgb;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grip {
    Move,
    Box(Handle),
    End(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Gesture {
    pub index: usize,
    pub grip: Grip,
    pub start: Pt,
    pub original: Shape,
    pub original_bounds: Rect,
}

fn dist(a: Pt, b: Pt) -> f32 {
    (a.x - b.x).hypot(a.y - b.y)
}

/// Khoảng cách từ `p` tới đoạn a–b.
fn seg_dist(p: Pt, a: Pt, b: Pt) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    if len2 <= 0.0 {
        return dist(p, a);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0);
    dist(
        p,
        Pt {
            x: a.x + t * dx,
            y: a.y + t * dy,
        },
    )
}

fn polyline_dist(points: &[Pt], p: Pt) -> Option<f32> {
    let first = points.first()?;
    if points.len() == 1 {
        return Some(dist(p, *first));
    }
    points
        .windows(2)
        .map(|w| seg_dist(p, w[0], w[1]))
        .fold(None, |m: Option<f32>, d| Some(m.map_or(d, |m| m.min(d))))
}

fn rect_f(r: Rect) -> (f32, f32, f32, f32) {
    (
        r.x as f32,
        r.y as f32,
        r.x as f32 + r.w as f32,
        r.y as f32 + r.h as f32,
    )
}

fn in_rect(r: Rect, p: Pt, pad: f32) -> bool {
    let (x0, y0, x1, y1) = rect_f(r);
    p.x >= x0 - pad && p.x <= x1 + pad && p.y >= y0 - pad && p.y <= y1 + pad
}

/// Hộp chuẩn hóa (x0, y0, x1, y1) của a–b.
fn norm_box(a: Pt, b: Pt) -> (f32, f32, f32, f32) {
    (a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y))
}

fn points_box(points: &[Pt]) -> Option<(f32, f32, f32, f32)> {
    let f = points.first()?;
    Some(
        points
            .iter()
            .fold((f.x, f.y, f.x, f.y), |(x0, y0, x1, y1), q| {
                (x0.min(q.x), y0.min(q.y), x1.max(q.x), y1.max(q.y))
            }),
    )
}

fn same_sign_or(v: f32, fallback: f32) -> f32 {
    if v < 0.0 {
        -1.0
    } else if v > 0.0 {
        1.0
    } else {
        fallback
    }
}

/// `p` nằm trong (hoặc trên cạnh) tam giác `t`; tam giác suy biến (diện tích 0) hoặc `p` không
/// hữu hạn thì không chứa gì.
fn tri_contains(p: Pt, t: [Pt; 3]) -> bool {
    let area2 = (t[1].x - t[0].x) * (t[2].y - t[0].y) - (t[2].x - t[0].x) * (t[1].y - t[0].y);
    if !p.x.is_finite() || !p.y.is_finite() || !area2.is_finite() || area2 == 0.0 {
        return false;
    }
    let s = |a: Pt, b: Pt| (p.x - b.x) * (a.y - b.y) - (a.x - b.x) * (p.y - b.y);
    let (d1, d2, d3) = (s(t[0], t[1]), s(t[1], t[2]), s(t[2], t[0]));
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

fn rect_of(a: Pt, b: Pt) -> Rect {
    let (x0, y0, x1, y1) = norm_box(a, b);
    Rect {
        x: x0.floor() as i32,
        y: y0.floor() as i32,
        w: (x1.ceil() - x0.floor()) as i32,
        h: (y1.ceil() - y0.floor()) as i32,
    }
}

/// Gần viền khung a–b: khoảng cách từ `p` tới viền <= width/2 + tol.
fn rect_edge_hit(a: Pt, b: Pt, width: f32, p: Pt, tol: f32) -> bool {
    let (x0, y0, x1, y1) = norm_box(a, b);
    let d = if p.x >= x0 && p.x <= x1 && p.y >= y0 && p.y <= y1 {
        (p.x - x0).min(x1 - p.x).min(p.y - y0).min(y1 - p.y)
    } else {
        let dx = (x0 - p.x).max(0.0).max(p.x - x1);
        let dy = (y0 - p.y).max(0.0).max(p.y - y1);
        dx.hypot(dy)
    };
    d <= width / 2.0 + tol
}

/// Gần viền elip nội tiếp khung a–b.
fn ellipse_edge_hit(a: Pt, b: Pt, width: f32, p: Pt, tol: f32) -> bool {
    let (x0, y0, x1, y1) = norm_box(a, b);
    let (rx, ry) = ((x1 - x0) / 2.0, (y1 - y0) / 2.0);
    let lim = width / 2.0 + tol;
    if rx < 1.0 || ry < 1.0 {
        return seg_dist(p, a, b) <= lim;
    }
    let (cx, cy) = (x0 + rx, y0 + ry);
    let k = (((p.x - cx) / rx).powi(2) + ((p.y - cy) / ry).powi(2)).sqrt();
    (k - 1.0).abs() * rx.min(ry) <= lim
}

/// `p` nằm trong elip nội tiếp khung a–b.
fn inside_ellipse(a: Pt, b: Pt, p: Pt) -> bool {
    let (x0, y0, x1, y1) = norm_box(a, b);
    let (rx, ry) = ((x1 - x0) / 2.0, (y1 - y0) / 2.0);
    if rx < 1.0 || ry < 1.0 {
        return false;
    }
    ((p.x - x0 - rx) / rx).powi(2) + ((p.y - y0 - ry) / ry).powi(2) <= 1.0
}

/// `p` trúng hình. `bounds`: hộp bao đo của hình (dùng cho Text). `tol` px.
pub fn hit_shape(shape: &Shape, bounds: Rect, p: Pt, tol: f32) -> bool {
    match shape {
        Shape::Pen { points, width, .. } | Shape::Marker { points, width, .. } => {
            polyline_dist(points, p).is_some_and(|d| d <= width / 2.0 + tol)
        }
        Shape::Line { a, b, width, .. } => seg_dist(p, *a, *b) <= width / 2.0 + tol,
        Shape::Arrow { a, b, width, .. } => {
            if seg_dist(p, *a, *b) <= width / 2.0 + tol {
                return true;
            }
            let g = arrow_geometry(*a, *b, *width, arrow_min_head(*width));
            tri_contains(p, g.head)
        }
        Shape::Rect { a, b, width, .. } => rect_edge_hit(*a, *b, *width, p, tol),
        Shape::Ellipse { a, b, width, .. } => ellipse_edge_hit(*a, *b, *width, p, tol),
        Shape::Box { a, b, oval, style } => {
            let filled = style.fill.is_some() && style.opacity > 0.0;
            let w = style.border_width();
            if *oval {
                (filled && inside_ellipse(*a, *b, p)) || ellipse_edge_hit(*a, *b, w, p, tol)
            } else {
                (filled && in_rect(rect_of(*a, *b), p, 0.0)) || rect_edge_hit(*a, *b, w, p, tol)
            }
        }
        Shape::Highlight { a, b, opacity, .. } => {
            *opacity > 0.0 && in_rect(rect_of(*a, *b), p, tol)
        }
        Shape::Blur { a, b, .. } => in_rect(blur_pixel_rect(*a, *b), p, tol),
        Shape::Step { center, radius, .. } => dist(p, *center) <= radius + tol,
        Shape::Text { .. } | Shape::Label { .. } => in_rect(bounds, p, tol),
    }
}

/// Vị trí hình trên cùng trúng `p` (duyệt từ cuối).
pub fn topmost_hit(shapes: &[Shape], bounds: &[Rect], p: Pt, tol: f32) -> Option<usize> {
    (0..shapes.len()).rev().find(|&i| {
        let (Some(s), Some(b)) = (shapes.get(i), bounds.get(i)) else {
            return false;
        };
        in_rect(*b, p, tol) && hit_shape(s, *b, p, tol)
    })
}

fn box_handles(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<(Grip, Pt)> {
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let pt = |x, y| Pt { x, y };
    vec![
        (Grip::Box(Handle::NW), pt(x0, y0)),
        (Grip::Box(Handle::N), pt(cx, y0)),
        (Grip::Box(Handle::NE), pt(x1, y0)),
        (Grip::Box(Handle::E), pt(x1, cy)),
        (Grip::Box(Handle::SE), pt(x1, y1)),
        (Grip::Box(Handle::S), pt(cx, y1)),
        (Grip::Box(Handle::SW), pt(x0, y1)),
        (Grip::Box(Handle::W), pt(x0, cy)),
    ]
}

fn corner_handles(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<(Grip, Pt)> {
    let pt = |x, y| Pt { x, y };
    vec![
        (Grip::Box(Handle::NW), pt(x0, y0)),
        (Grip::Box(Handle::NE), pt(x1, y0)),
        (Grip::Box(Handle::SE), pt(x1, y1)),
        (Grip::Box(Handle::SW), pt(x0, y1)),
    ]
}

/// Tay nắm và vị trí tâm của chúng.
pub fn handles(shape: &Shape, bounds: Rect) -> Vec<(Grip, Pt)> {
    match shape {
        Shape::Line { a, b, .. } | Shape::Arrow { a, b, .. } => {
            vec![(Grip::End(0), *a), (Grip::End(1), *b)]
        }
        Shape::Rect { a, b, .. }
        | Shape::Ellipse { a, b, .. }
        | Shape::Blur { a, b, .. }
        | Shape::Box { a, b, .. }
        | Shape::Highlight { a, b, .. } => {
            let (x0, y0, x1, y1) = norm_box(*a, *b);
            box_handles(x0, y0, x1, y1)
        }
        Shape::Pen { points, .. } | Shape::Marker { points, .. } => match points_box(points) {
            Some((x0, y0, x1, y1)) => corner_handles(x0, y0, x1, y1),
            None => Vec::new(),
        },
        Shape::Text { .. } | Shape::Label { .. } => {
            let (x0, y0, x1, y1) = rect_f(bounds);
            corner_handles(x0, y0, x1, y1)
        }
        Shape::Step { center, radius, .. } => corner_handles(
            center.x - radius,
            center.y - radius,
            center.x + radius,
            center.y + radius,
        ),
    }
}

/// Tay nắm có tâm cách p (theo max |dx|, |dy|) ≤ radius; nhiều cái thì cái gần nhất.
pub fn grip_at(shape: &Shape, bounds: Rect, p: Pt, radius: f32) -> Option<Grip> {
    handles(shape, bounds)
        .into_iter()
        .map(|(g, c)| (g, (c.x - p.x).abs().max((c.y - p.y).abs())))
        .filter(|(_, d)| *d <= radius)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(g, _)| g)
}

/// Khung chọn (bounds nới 2 px); None với Line và Arrow.
pub fn frame(shape: &Shape, bounds: Rect) -> Option<Rect> {
    match shape {
        Shape::Line { .. } | Shape::Arrow { .. } => None,
        _ => Some(Rect {
            x: bounds.x.saturating_sub(2),
            y: bounds.y.saturating_sub(2),
            w: bounds.w.saturating_add(4),
            h: bounds.h.saturating_add(4),
        }),
    }
}

fn shift_pt(p: Pt, dx: f32, dy: f32) -> Pt {
    Pt {
        x: p.x + dx,
        y: p.y + dy,
    }
}

fn recolor(img: &Image, a: Pt, b: Pt, old: Rgb) -> Rgb {
    redact_color(img, blur_pixel_rect(a, b)).unwrap_or(old)
}

/// Một chiều của hộp: cạnh đứng yên `fixed`, cạnh động theo `to`; `moves_low` khi cạnh động là cạnh nhỏ.
fn edge(fixed: f32, to: f32, moves_low: bool, min_extent: f32) -> f32 {
    let d = to - fixed;
    if !d.is_finite() {
        return fixed + if moves_low { -min_extent } else { min_extent };
    }
    if d.abs() < min_extent {
        fixed + same_sign_or(d, if moves_low { -1.0 } else { 1.0 }) * min_extent
    } else {
        to
    }
}

fn transform_box(
    g: &Gesture,
    h: Handle,
    to: Pt,
    shift: bool,
    min_extent: f32,
    img: &Image,
) -> Shape {
    let (a, b) = match &g.original {
        Shape::Rect { a, b, .. }
        | Shape::Ellipse { a, b, .. }
        | Shape::Blur { a, b, .. }
        | Shape::Box { a, b, .. }
        | Shape::Highlight { a, b, .. } => (*a, *b),
        _ => return g.original.clone(),
    };
    let (x0, y0, x1, y1) = norm_box(a, b);
    let left = matches!(h, Handle::W | Handle::NW | Handle::SW);
    let right = matches!(h, Handle::E | Handle::NE | Handle::SE);
    let top = matches!(h, Handle::N | Handle::NW | Handle::NE);
    let bottom = matches!(h, Handle::S | Handle::SW | Handle::SE);
    let (mut tx, mut ty) = (to.x, to.y);
    if shift && (left || right) && (top || bottom) {
        let fx = if left { x1 } else { x0 };
        let fy = if top { y1 } else { y0 };
        let (w0, h0) = (x1 - x0, y1 - y0);
        let (dx, dy) = (tx - fx, ty - fy);
        let rx = if w0 > 0.0 { dx.abs() / w0 } else { 0.0 };
        let ry = if h0 > 0.0 { dy.abs() / h0 } else { 0.0 };
        let s = rx.max(ry);
        if s.is_finite() {
            tx = fx + same_sign_or(dx, if left { -1.0 } else { 1.0 }) * s * w0;
            ty = fy + same_sign_or(dy, if top { -1.0 } else { 1.0 }) * s * h0;
        }
    }
    let (nx0, nx1) = if left {
        (edge(x1, tx, true, min_extent), x1)
    } else if right {
        (x0, edge(x0, tx, false, min_extent))
    } else {
        (x0, x1)
    };
    let (ny0, ny1) = if top {
        (edge(y1, ty, true, min_extent), y1)
    } else if bottom {
        (y0, edge(y0, ty, false, min_extent))
    } else {
        (y0, y1)
    };
    let na = Pt {
        x: nx0.min(nx1),
        y: ny0.min(ny1),
    };
    let nb = Pt {
        x: nx0.max(nx1),
        y: ny0.max(ny1),
    };
    match &g.original {
        Shape::Rect { width, color, .. } => Shape::Rect {
            a: na,
            b: nb,
            width: *width,
            color: *color,
        },
        Shape::Ellipse { width, color, .. } => Shape::Ellipse {
            a: na,
            b: nb,
            width: *width,
            color: *color,
        },
        Shape::Blur { color, .. } => Shape::Blur {
            a: na,
            b: nb,
            color: recolor(img, na, nb, *color),
        },
        Shape::Box { oval, style, .. } => Shape::Box {
            a: na,
            b: nb,
            oval: *oval,
            style: *style,
        },
        Shape::Highlight { color, opacity, .. } => Shape::Highlight {
            a: na,
            b: nb,
            color: *color,
            opacity: *opacity,
        },
        other => other.clone(),
    }
}

/// Neo (góc đối) và góc nắm của một tay nắm góc trên hộp (x0, y0, x1, y1).
fn corner_pair(h: Handle, bx: (f32, f32, f32, f32)) -> Option<(Pt, Pt)> {
    let (x0, y0, x1, y1) = bx;
    let pt = |x, y| Pt { x, y };
    match h {
        Handle::NW => Some((pt(x1, y1), pt(x0, y0))),
        Handle::NE => Some((pt(x0, y1), pt(x1, y0))),
        Handle::SE => Some((pt(x0, y0), pt(x1, y1))),
        Handle::SW => Some((pt(x1, y0), pt(x0, y1))),
        _ => None,
    }
}

fn transform_points(g: &Gesture, h: Handle, to: Pt, shift: bool, min_extent: f32) -> Shape {
    let (points, width, color, marker_opacity) = match &g.original {
        Shape::Pen {
            points,
            width,
            color,
        } => (points, *width, *color, None),
        Shape::Marker {
            points,
            width,
            color,
            opacity,
        } => (points, *width, *color, Some(*opacity)),
        _ => return g.original.clone(),
    };
    let Some(bx) = points_box(points) else {
        return g.original.clone();
    };
    let Some((anchor, c)) = corner_pair(h, bx) else {
        return g.original.clone();
    };
    let factor = |to_v: f32, a: f32, c: f32| {
        let den = c - a;
        if den == 0.0 {
            return 1.0;
        }
        let s = (to_v - a) / den;
        if !s.is_finite() {
            return 1.0;
        }
        let w = den.abs();
        if (s * w).abs() < min_extent {
            same_sign_or(s, 1.0) * min_extent / w
        } else {
            s
        }
    };
    let (mut sx, mut sy) = (factor(to.x, anchor.x, c.x), factor(to.y, anchor.y, c.y));
    if shift {
        let s = sx.abs().max(sy.abs());
        sx = same_sign_or(sx, 1.0) * s;
        sy = same_sign_or(sy, 1.0) * s;
    }
    let pts: Vec<Pt> = points
        .iter()
        .map(|q| Pt {
            x: anchor.x + (q.x - anchor.x) * sx,
            y: anchor.y + (q.y - anchor.y) * sy,
        })
        .collect();
    match marker_opacity {
        None => Shape::Pen {
            points: pts,
            width,
            color,
        },
        Some(opacity) => Shape::Marker {
            points: pts,
            width,
            color,
            opacity,
        },
    }
}

fn transform_scaled(g: &Gesture, h: Handle, to: Pt) -> Shape {
    let (bx, lo, hi) = match &g.original {
        Shape::Text { size, .. } | Shape::Label { size, .. } => {
            let (x0, y0, x1, y1) = rect_f(g.original_bounds);
            (
                (x0, y0, x1, y1),
                TEXT_SIZE_MIN_PX,
                (TEXT_SIZE_MAX_PX, *size),
            )
        }
        Shape::Step { center, radius, .. } => (
            (
                center.x - radius,
                center.y - radius,
                center.x + radius,
                center.y + radius,
            ),
            STEP_RADIUS_MIN_PX,
            (STEP_RADIUS_MAX_PX, *radius),
        ),
        _ => return g.original.clone(),
    };
    let (max, cur) = hi;
    if cur <= 0.0 || !cur.is_finite() {
        return g.original.clone();
    }
    let Some((neo, c)) = corner_pair(h, bx) else {
        return g.original.clone();
    };
    let (vx, vy) = (c.x - neo.x, c.y - neo.y);
    let den = vx * vx + vy * vy;
    let mut k = if den > 0.0 {
        ((to.x - neo.x) * vx + (to.y - neo.y) * vy) / den
    } else {
        1.0
    };
    if !k.is_finite() {
        k = 1.0;
    }
    k = k.clamp(lo / cur, max / cur);
    let scale = |p: Pt| Pt {
        x: neo.x + (p.x - neo.x) * k,
        y: neo.y + (p.y - neo.y) * k,
    };
    match &g.original {
        Shape::Text {
            origin,
            text,
            size,
            color,
        } => Shape::Text {
            origin: scale(*origin),
            text: text.clone(),
            size: size * k,
            color: *color,
        },
        Shape::Label {
            origin,
            text,
            size,
            color,
            align,
            style,
        } => Shape::Label {
            origin: scale(*origin),
            text: text.clone(),
            size: size * k,
            color: *color,
            align: *align,
            style: *style,
        },
        Shape::Step {
            center,
            radius,
            n,
            color,
        } => Shape::Step {
            center: scale(*center),
            radius: radius * k,
            n: *n,
            color: *color,
        },
        other => other.clone(),
    }
}

fn transform_end(g: &Gesture, i: usize, to: Pt, shift: bool) -> Shape {
    let (a, b) = match &g.original {
        Shape::Line { a, b, .. } | Shape::Arrow { a, b, .. } => (*a, *b),
        _ => return g.original.clone(),
    };
    let (moved, other) = match i {
        0 => (a, b),
        1 => (b, a),
        _ => return g.original.clone(),
    };
    let new = if shift { constrain_45(other, to) } else { to };
    let d = dist(new, other);
    let new = if d.is_finite() && d >= MIN_DRAG_PX {
        new
    } else {
        moved
    };
    let (na, nb) = if i == 0 { (new, b) } else { (a, new) };
    match &g.original {
        Shape::Line { width, color, .. } => Shape::Line {
            a: na,
            b: nb,
            width: *width,
            color: *color,
        },
        Shape::Arrow { width, color, .. } => Shape::Arrow {
            a: na,
            b: nb,
            width: *width,
            color: *color,
        },
        other => other.clone(),
    }
}

fn transform_move(g: &Gesture, to: Pt, img: &Image) -> Shape {
    let (dx, dy) = (to.x - g.start.x, to.y - g.start.y);
    let mv = |p: &Pt| shift_pt(*p, dx, dy);
    match &g.original {
        Shape::Pen {
            points,
            width,
            color,
        } => Shape::Pen {
            points: points.iter().map(mv).collect(),
            width: *width,
            color: *color,
        },
        Shape::Marker {
            points,
            width,
            color,
            opacity,
        } => Shape::Marker {
            points: points.iter().map(mv).collect(),
            width: *width,
            color: *color,
            opacity: *opacity,
        },
        Shape::Line { a, b, width, color } => Shape::Line {
            a: mv(a),
            b: mv(b),
            width: *width,
            color: *color,
        },
        Shape::Arrow { a, b, width, color } => Shape::Arrow {
            a: mv(a),
            b: mv(b),
            width: *width,
            color: *color,
        },
        Shape::Rect { a, b, width, color } => Shape::Rect {
            a: mv(a),
            b: mv(b),
            width: *width,
            color: *color,
        },
        Shape::Ellipse { a, b, width, color } => Shape::Ellipse {
            a: mv(a),
            b: mv(b),
            width: *width,
            color: *color,
        },
        Shape::Blur { a, b, color } => {
            let (na, nb) = (mv(a), mv(b));
            Shape::Blur {
                a: na,
                b: nb,
                color: recolor(img, na, nb, *color),
            }
        }
        Shape::Text {
            origin,
            text,
            size,
            color,
        } => Shape::Text {
            origin: mv(origin),
            text: text.clone(),
            size: *size,
            color: *color,
        },
        Shape::Box { a, b, oval, style } => Shape::Box {
            a: mv(a),
            b: mv(b),
            oval: *oval,
            style: *style,
        },
        Shape::Highlight {
            a,
            b,
            color,
            opacity,
        } => Shape::Highlight {
            a: mv(a),
            b: mv(b),
            color: *color,
            opacity: *opacity,
        },
        Shape::Label {
            origin,
            text,
            size,
            color,
            align,
            style,
        } => Shape::Label {
            origin: mv(origin),
            text: text.clone(),
            size: *size,
            color: *color,
            align: *align,
            style: *style,
        },
        Shape::Step {
            center,
            radius,
            n,
            color,
        } => Shape::Step {
            center: mv(center),
            radius: *radius,
            n: *n,
            color: *color,
        },
    }
}

/// Hình xem trước / chốt của cử chỉ `g` khi chuột ở `to`.
pub fn transform(g: &Gesture, to: Pt, shift: bool, min_extent: f32, img: &Image) -> Shape {
    match g.grip {
        Grip::Move => transform_move(g, to, img),
        Grip::End(i) => transform_end(g, i, to, shift),
        Grip::Box(h) => match &g.original {
            Shape::Rect { .. }
            | Shape::Ellipse { .. }
            | Shape::Blur { .. }
            | Shape::Box { .. }
            | Shape::Highlight { .. } => transform_box(g, h, to, shift, min_extent, img),
            Shape::Pen { .. } | Shape::Marker { .. } => {
                transform_points(g, h, to, shift, min_extent)
            }
            Shape::Text { .. } | Shape::Label { .. } | Shape::Step { .. } => {
                transform_scaled(g, h, to)
            }
            _ => g.original.clone(),
        },
    }
}

/// Đổi kiểu: `color` (Blur bỏ qua), `level` đổi px theo `dpi`; Blur bỏ qua cả hai.
pub fn restyle(shape: &Shape, level: Option<SizeLevel>, color: Option<Rgb>, dpi: u32) -> Shape {
    let mut s = shape.clone();
    let stroke = level.map(|l| dip_to_px(l.stroke_dip(), dpi));
    match &mut s {
        Shape::Pen {
            width, color: c, ..
        }
        | Shape::Line {
            width, color: c, ..
        }
        | Shape::Arrow {
            width, color: c, ..
        }
        | Shape::Rect {
            width, color: c, ..
        }
        | Shape::Ellipse {
            width, color: c, ..
        } => {
            if let Some(w) = stroke {
                *width = w;
            }
            if let Some(n) = color {
                *c = n;
            }
        }
        Shape::Marker {
            width, color: c, ..
        } => {
            if let Some(l) = level {
                *width = dip_to_px(l.marker_dip(), dpi);
            }
            if let Some(n) = color {
                *c = n;
            }
        }
        Shape::Highlight { color: c, .. } => {
            if let Some(n) = color {
                *c = n;
            }
        }
        Shape::Box { style, .. } => {
            if let Some(l) = level {
                style.stroke_width = dip_to_px(l.stroke_dip(), dpi);
            }
            if let Some(n) = color {
                style.stroke = n;
            }
        }
        Shape::Text { size, color: c, .. } | Shape::Label { size, color: c, .. } => {
            if let Some(l) = level {
                *size = dip_to_px(l.text_dip(), dpi);
            }
            if let Some(n) = color {
                *c = n;
            }
        }
        Shape::Step {
            radius, color: c, ..
        } => {
            if let Some(l) = level {
                *radius = dip_to_px(l.step_dip(), dpi);
            }
            if let Some(n) = color {
                *c = n;
            }
        }
        Shape::Blur { .. } => {}
    }
    s
}

/// Đổi riêng cho Highlight (Task 113): Bút dạ quang nhận độ dày `width_px` và màu; Highlight hình
/// chữ nhật nhận độ trong suốt (kẹp 0..=1, không phải số thì giữ) và màu. Hình khác giữ nguyên.
pub fn highlight_restyle(
    shape: &Shape,
    width_px: Option<f32>,
    opacity: Option<f32>,
    color: Option<Rgb>,
) -> Shape {
    let mut s = shape.clone();
    match &mut s {
        Shape::Marker {
            width,
            color: c,
            opacity: o,
            ..
        } => {
            if let Some(w) = width_px.filter(|w| w.is_finite() && *w > 0.0) {
                *width = w;
            }
            if let Some(v) = opacity.filter(|v| !v.is_nan()) {
                *o = v.clamp(0.0, 1.0);
            }
            if let Some(n) = color {
                *c = n;
            }
        }
        Shape::Highlight {
            opacity: o,
            color: c,
            ..
        } => {
            if let Some(v) = opacity.filter(|v| !v.is_nan()) {
                *o = v.clamp(0.0, 1.0);
            }
            if let Some(n) = color {
                *c = n;
            }
        }
        _ => {}
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotate::{
        blur_pixel_rect, redact_color, STEP_RADIUS_MAX_PX, STEP_RADIUS_MIN_PX, TEXT_SIZE_MAX_PX,
        TEXT_SIZE_MIN_PX,
    };

    const RED: Rgb = Rgb(255, 0, 0);
    const BLUE: Rgb = Rgb(0, 0, 255);
    const Z: Rect = Rect {
        x: 0,
        y: 0,
        w: 0,
        h: 0,
    };

    fn p(x: f32, y: f32) -> Pt {
        Pt { x, y }
    }
    fn rc(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }
    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }
    /// 100×100, nửa trái đỏ thuần, nửa phải xanh thuần.
    fn img() -> Image {
        let mut bgra = Vec::new();
        for _y in 0..100 {
            for x in 0..100 {
                let c = if x < 50 { RED } else { BLUE };
                bgra.extend_from_slice(&[c.2, c.1, c.0, 255]);
            }
        }
        Image {
            width: 100,
            height: 100,
            bgra,
        }
    }
    fn rect(a: Pt, b: Pt) -> Shape {
        Shape::Rect {
            a,
            b,
            width: 4.0,
            color: RED,
        }
    }
    fn blur(a: Pt, b: Pt, color: Rgb) -> Shape {
        Shape::Blur { a, b, color }
    }
    fn gesture(shape: Shape, grip: Grip, start: Pt) -> Gesture {
        let ob = shape.bounds().unwrap_or(Z);
        Gesture {
            index: 0,
            grip,
            start,
            original: shape,
            original_bounds: ob,
        }
    }
    fn tf(shape: Shape, grip: Grip, to: Pt, shift: bool, min: f32) -> Shape {
        transform(&gesture(shape, grip, p(0.0, 0.0)), to, shift, min, &img())
    }

    #[test]
    fn hit_each_kind() {
        let pen = Shape::Pen {
            points: vec![p(10.0, 10.0), p(50.0, 10.0)],
            width: 4.0,
            color: RED,
        };
        assert!(hit_shape(&pen, Z, p(30.0, 13.0), 4.0));
        assert!(!hit_shape(&pen, Z, p(30.0, 30.0), 4.0));
        let marker = Shape::Marker {
            points: vec![p(10.0, 10.0)],
            width: 16.0,
            color: RED,
            opacity: crate::annotate::HIGHLIGHT_OPACITY,
        };
        assert!(hit_shape(&marker, Z, p(15.0, 10.0), 4.0));
        assert!(!hit_shape(&marker, Z, p(40.0, 10.0), 4.0));
        let line = Shape::Line {
            a: p(0.0, 0.0),
            b: p(40.0, 0.0),
            width: 4.0,
            color: RED,
        };
        assert!(hit_shape(&line, Z, p(20.0, 5.0), 4.0));
        assert!(!hit_shape(&line, Z, p(20.0, 15.0), 4.0));
        let arrow = Shape::Arrow {
            a: p(0.0, 0.0),
            b: p(60.0, 0.0),
            width: 3.0,
            color: RED,
        };
        assert!(hit_shape(&arrow, Z, p(30.0, 2.0), 4.0));
        assert!(hit_shape(&arrow, Z, p(52.0, 3.0), 0.0));
        assert!(!hit_shape(&arrow, Z, p(30.0, 20.0), 4.0));
        let r = rect(p(10.0, 10.0), p(50.0, 50.0));
        assert!(hit_shape(&r, Z, p(30.0, 10.0), 4.0));
        assert!(!hit_shape(&r, Z, p(30.0, 30.0), 4.0));
        let e = Shape::Ellipse {
            a: p(0.0, 0.0),
            b: p(40.0, 20.0),
            width: 4.0,
            color: RED,
        };
        assert!(hit_shape(&e, Z, p(20.0, 0.0), 4.0));
        assert!(!hit_shape(&e, Z, p(20.0, 10.0), 4.0));
        let st = Shape::Step {
            center: p(50.0, 50.0),
            radius: 10.0,
            n: 1,
            color: RED,
        };
        assert!(hit_shape(&st, Z, p(58.0, 50.0), 4.0));
        assert!(!hit_shape(&st, Z, p(70.0, 50.0), 4.0));
        let b = blur(p(10.0, 10.0), p(30.0, 30.0), RED);
        assert!(hit_shape(&b, Z, p(20.0, 20.0), 4.0));
        assert!(!hit_shape(&b, Z, p(50.0, 50.0), 4.0));
        let t = Shape::Text {
            origin: p(10.0, 10.0),
            text: "x".into(),
            size: 20.0,
            color: RED,
        };
        assert!(hit_shape(&t, rc(10, 10, 40, 20), p(30.0, 20.0), 4.0));
        assert!(hit_shape(&t, rc(10, 10, 40, 20), p(52.0, 20.0), 4.0));
        assert!(!hit_shape(&t, rc(10, 10, 40, 20), p(80.0, 80.0), 4.0));
    }

    #[test]
    fn arrow_degenerate_head_no_hit() {
        // a == b: tam giác đầu suy biến (diện tích 0) không được trúng mọi điểm.
        let arrow = Shape::Arrow {
            a: p(10.0, 10.0),
            b: p(10.0, 10.0),
            width: 3.0,
            color: RED,
        };
        assert!(!hit_shape(&arrow, Z, p(100.0, 100.0), 4.0));
        assert!(!hit_shape(&arrow, Z, p(-50.0, 30.0), 0.0));
        assert!(hit_shape(&arrow, Z, p(11.0, 10.0), 4.0));
        let t = [p(0.0, 0.0), p(5.0, 5.0), p(10.0, 10.0)];
        assert!(!tri_contains(p(5.0, 5.0), t));
        assert!(!tri_contains(p(100.0, 0.0), t));
        // Điểm không hữu hạn không trúng.
        let ok = [p(0.0, 0.0), p(10.0, 0.0), p(0.0, 10.0)];
        assert!(tri_contains(p(2.0, 2.0), ok));
        assert!(!tri_contains(p(f32::NAN, 2.0), ok));
        assert!(!tri_contains(p(2.0, f32::INFINITY), ok));
        let normal = Shape::Arrow {
            a: p(0.0, 0.0),
            b: p(60.0, 0.0),
            width: 3.0,
            color: RED,
        };
        assert!(!hit_shape(&normal, Z, p(f32::NAN, f32::NAN), 4.0));
    }

    #[test]
    fn topmost_wins() {
        let shapes = vec![
            blur(p(0.0, 0.0), p(40.0, 40.0), RED),
            blur(p(20.0, 20.0), p(60.0, 60.0), RED),
        ];
        let bounds: Vec<Rect> = shapes.iter().filter_map(|s| s.bounds()).collect();
        assert_eq!(topmost_hit(&shapes, &bounds, p(30.0, 30.0), 2.0), Some(1));
        assert_eq!(topmost_hit(&shapes, &bounds, p(5.0, 5.0), 2.0), Some(0));
        assert_eq!(topmost_hit(&shapes, &bounds, p(100.0, 100.0), 2.0), None);
        assert_eq!(
            topmost_hit(&shapes, &bounds[..1], p(30.0, 30.0), 2.0),
            Some(0)
        );
        assert_eq!(topmost_hit(&shapes, &[], p(30.0, 30.0), 2.0), None);
    }

    #[test]
    fn handles_per_kind() {
        let line = Shape::Line {
            a: p(0.0, 0.0),
            b: p(9.0, 9.0),
            width: 3.0,
            color: RED,
        };
        assert_eq!(handles(&line, Z).len(), 2);
        let r = rect(p(30.0, 40.0), p(10.0, 20.0));
        let hs = handles(&r, Z);
        assert_eq!(hs.len(), 8);
        assert_eq!(hs[0], (Grip::Box(Handle::NW), p(10.0, 20.0)));
        assert_eq!(handles(&blur(p(0.0, 0.0), p(9.0, 9.0), RED), Z).len(), 8);
        let e = Shape::Ellipse {
            a: p(0.0, 0.0),
            b: p(9.0, 9.0),
            width: 3.0,
            color: RED,
        };
        assert_eq!(handles(&e, Z).len(), 8);
        let pen = Shape::Pen {
            points: vec![p(0.0, 0.0), p(9.0, 5.0)],
            width: 3.0,
            color: RED,
        };
        assert_eq!(handles(&pen, Z).len(), 4);
        let mk = Shape::Marker {
            points: vec![p(0.0, 0.0), p(9.0, 5.0)],
            width: 3.0,
            color: RED,
            opacity: crate::annotate::HIGHLIGHT_OPACITY,
        };
        assert_eq!(handles(&mk, Z).len(), 4);
        let t = Shape::Text {
            origin: p(1.0, 1.0),
            text: "a".into(),
            size: 18.0,
            color: RED,
        };
        assert_eq!(handles(&t, rc(1, 1, 10, 20)).len(), 4);
        let st = Shape::Step {
            center: p(5.0, 5.0),
            radius: 4.0,
            n: 1,
            color: RED,
        };
        assert_eq!(handles(&st, Z).len(), 4);
    }

    #[test]
    fn grip_at_picks_nearest() {
        let r = rect(p(10.0, 10.0), p(50.0, 50.0));
        assert_eq!(
            grip_at(&r, Z, p(13.0, 10.0), 5.0),
            Some(Grip::Box(Handle::NW))
        );
        assert_eq!(grip_at(&r, Z, p(16.0, 10.0), 5.0), None);
        let line = Shape::Line {
            a: p(0.0, 0.0),
            b: p(10.0, 0.0),
            width: 3.0,
            color: RED,
        };
        assert_eq!(grip_at(&line, Z, p(9.0, 1.0), 5.0), Some(Grip::End(1)));
    }

    #[test]
    fn frame_rules() {
        let r = rect(p(10.0, 10.0), p(50.0, 50.0));
        assert_eq!(frame(&r, rc(10, 10, 40, 40)), Some(rc(8, 8, 44, 44)));
        let line = Shape::Line {
            a: p(0.0, 0.0),
            b: p(9.0, 9.0),
            width: 3.0,
            color: RED,
        };
        assert_eq!(frame(&line, rc(0, 0, 9, 9)), None);
    }

    #[test]
    fn move_preserves_style() {
        let pen = Shape::Pen {
            points: vec![p(1.0, 1.0), p(5.0, 5.0)],
            width: 3.0,
            color: BLUE,
        };
        let pen2 = Shape::Pen {
            points: vec![p(8.0, -2.0), p(12.0, 2.0)],
            width: 3.0,
            color: BLUE,
        };
        let line = Shape::Line {
            a: p(1.0, 1.0),
            b: p(5.0, 5.0),
            width: 3.0,
            color: BLUE,
        };
        let line2 = Shape::Line {
            a: p(8.0, -2.0),
            b: p(12.0, 2.0),
            width: 3.0,
            color: BLUE,
        };
        let arrow = Shape::Arrow {
            a: p(1.0, 1.0),
            b: p(5.0, 5.0),
            width: 3.0,
            color: BLUE,
        };
        let arrow2 = Shape::Arrow {
            a: p(8.0, -2.0),
            b: p(12.0, 2.0),
            width: 3.0,
            color: BLUE,
        };
        let text = Shape::Text {
            origin: p(1.0, 1.0),
            text: "hi".into(),
            size: 22.0,
            color: BLUE,
        };
        let text2 = Shape::Text {
            origin: p(8.0, -2.0),
            text: "hi".into(),
            size: 22.0,
            color: BLUE,
        };
        let step = Shape::Step {
            center: p(1.0, 1.0),
            radius: 9.0,
            n: 3,
            color: BLUE,
        };
        let step2 = Shape::Step {
            center: p(8.0, -2.0),
            radius: 9.0,
            n: 3,
            color: BLUE,
        };
        let mk = Shape::Marker {
            points: vec![p(1.0, 1.0)],
            width: 16.0,
            color: BLUE,
            opacity: crate::annotate::HIGHLIGHT_OPACITY,
        };
        let mk2 = Shape::Marker {
            points: vec![p(8.0, -2.0)],
            width: 16.0,
            color: BLUE,
            opacity: crate::annotate::HIGHLIGHT_OPACITY,
        };
        let el = Shape::Ellipse {
            a: p(1.0, 1.0),
            b: p(5.0, 5.0),
            width: 3.0,
            color: BLUE,
        };
        let el2 = Shape::Ellipse {
            a: p(8.0, -2.0),
            b: p(12.0, 2.0),
            width: 3.0,
            color: BLUE,
        };
        let rt = Shape::Rect {
            a: p(1.0, 1.0),
            b: p(5.0, 5.0),
            width: 3.0,
            color: BLUE,
        };
        let rt2 = Shape::Rect {
            a: p(8.0, -2.0),
            b: p(12.0, 2.0),
            width: 3.0,
            color: BLUE,
        };
        let bl = blur(p(1.0, 1.0), p(5.0, 5.0), RED);
        let bl2 = blur(p(8.0, -2.0), p(12.0, 2.0), RED);
        for (s, want) in [
            (pen, pen2),
            (line, line2),
            (arrow, arrow2),
            (text, text2),
            (step, step2),
            (mk, mk2),
            (el, el2),
            (rt, rt2),
            (bl, bl2),
        ] {
            let g = gesture(s, Grip::Move, p(10.0, 10.0));
            assert_eq!(transform(&g, p(17.0, 7.0), false, 2.0, &img()), want);
        }
    }

    #[test]
    fn rect_resize_flip_and_min() {
        let r = rect(p(10.0, 10.0), p(50.0, 50.0));
        match tf(r.clone(), Grip::Box(Handle::SE), p(5.0, 60.0), false, 2.0) {
            Shape::Rect { a, b, .. } => {
                assert_eq!((a, b), (p(5.0, 10.0), p(10.0, 60.0)));
            }
            o => panic!("{o:?}"),
        }
        match tf(r.clone(), Grip::Box(Handle::SE), p(10.5, 30.0), false, 6.0) {
            Shape::Rect { a, b, .. } => assert!(near(b.x - a.x, 6.0)),
            o => panic!("{o:?}"),
        }
        match tf(r, Grip::Box(Handle::SE), p(90.0, 60.0), true, 2.0) {
            Shape::Rect { a, b, .. } => {
                assert!(
                    near(b.x - a.x, 80.0) && near(b.y - a.y, 80.0),
                    "{a:?} {b:?}"
                );
            }
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn line_end_shift() {
        let l = Shape::Line {
            a: p(0.0, 0.0),
            b: p(30.0, 0.0),
            width: 3.0,
            color: RED,
        };
        match tf(l.clone(), Grip::End(1), p(50.0, 3.0), true, 2.0) {
            Shape::Line { a, b, .. } => {
                assert_eq!(a, p(0.0, 0.0));
                assert!(b.y == 0.0 && b.x > 40.0);
            }
            o => panic!("{o:?}"),
        }
        assert_eq!(tf(l.clone(), Grip::End(1), p(1.0, 0.0), false, 2.0), l);
    }

    #[test]
    fn pen_scale_keeps_width() {
        let pen = Shape::Pen {
            points: vec![p(0.0, 0.0), p(5.0, 0.0), p(10.0, 0.0)],
            width: 3.0,
            color: RED,
        };
        match tf(pen, Grip::Box(Handle::SE), p(20.0, 40.0), false, 2.0) {
            Shape::Pen { points, width, .. } => {
                assert_eq!(width, 3.0);
                let xs: Vec<f32> = points.iter().map(|q| q.x).collect();
                assert!(xs.iter().zip([0.0, 10.0, 20.0]).all(|(a, b)| near(*a, b)));
                assert!(points.iter().all(|q| q.y == 0.0));
            }
            o => panic!("{o:?}"),
        }
    }

    fn text() -> Shape {
        Shape::Text {
            origin: p(10.0, 10.0),
            text: "ab".into(),
            size: 20.0,
            color: RED,
        }
    }

    fn text_gesture(to: Pt) -> Shape {
        let g = Gesture {
            index: 0,
            grip: Grip::Box(Handle::SE),
            start: p(50.0, 30.0),
            original: text(),
            original_bounds: rc(10, 10, 40, 20),
        };
        transform(&g, to, false, 2.0, &img())
    }

    #[test]
    fn text_resize_uniform() {
        match text_gesture(p(90.0, 50.0)) {
            Shape::Text { origin, size, .. } => {
                assert!(near(size, 40.0));
                assert_eq!(origin, p(10.0, 10.0));
            }
            o => panic!("{o:?}"),
        }
        match text_gesture(p(10.5, 10.2)) {
            Shape::Text { size, .. } => assert_eq!(size, TEXT_SIZE_MIN_PX),
            o => panic!("{o:?}"),
        }
        match text_gesture(p(9000.0, 5000.0)) {
            Shape::Text { size, .. } => assert_eq!(size, TEXT_SIZE_MAX_PX),
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn step_resize_clamped() {
        let st = Shape::Step {
            center: p(50.0, 50.0),
            radius: 10.0,
            n: 2,
            color: RED,
        };
        let go = |to: Pt| tf(st.clone(), Grip::Box(Handle::SE), to, false, 2.0);
        match go(p(100.0, 100.0)) {
            Shape::Step {
                center, radius, n, ..
            } => {
                assert!(near(radius, 30.0) && n == 2);
                assert!(near(center.x, 70.0) && near(center.y, 70.0));
            }
            o => panic!("{o:?}"),
        }
        match go(p(41.0, 41.0)) {
            Shape::Step { radius, .. } => assert_eq!(radius, STEP_RADIUS_MIN_PX),
            o => panic!("{o:?}"),
        }
        match go(p(1000.0, 1000.0)) {
            Shape::Step { radius, .. } => assert_eq!(radius, STEP_RADIUS_MAX_PX),
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn blur_move_recolors_from_original() {
        let b = blur(p(10.0, 10.0), p(30.0, 30.0), RED);
        let g = gesture(b.clone(), Grip::Move, p(20.0, 20.0));
        match transform(&g, p(80.0, 20.0), false, 2.0, &img()) {
            Shape::Blur { a, color, .. } => {
                assert!(near(a.x, 70.0));
                assert_eq!(color, BLUE);
            }
            o => panic!("{o:?}"),
        }
        assert_eq!(g.original, b);
    }

    #[test]
    fn blur_resize_flip_stays_covered() {
        let im = img();
        let b = blur(p(60.0, 10.0), p(80.0, 30.0), BLUE);
        let g = gesture(b, Grip::Box(Handle::E), p(80.0, 20.0));
        match transform(&g, p(20.0, 20.0), false, 2.0, &im) {
            Shape::Blur { a, b, color } => {
                assert!(a.x < b.x);
                let r = blur_pixel_rect(a, b);
                assert!(r.w > 0 && r.h > 0);
                assert_eq!(Some(color), redact_color(&im, r));
                assert_ne!(color, BLUE);
            }
            o => panic!("{o:?}"),
        }
        let b = blur(p(60.0, 0.0), p(80.0, 20.0), BLUE);
        let g = gesture(b, Grip::Box(Handle::S), p(70.0, 20.0));
        match transform(&g, p(70.0, -30.0), false, 2.0, &im) {
            Shape::Blur { color, .. } => assert_eq!(color, BLUE),
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn restyle_rules() {
        let pen = Shape::Pen {
            points: vec![p(0.0, 0.0)],
            width: 3.0,
            color: RED,
        };
        match restyle(&pen, Some(SizeLevel::Large), None, 192) {
            Shape::Pen { width, color, .. } => assert!(near(width, 10.0) && color == RED),
            o => panic!("{o:?}"),
        }
        let mk = Shape::Marker {
            points: vec![p(0.0, 0.0)],
            width: 16.0,
            color: RED,
            opacity: crate::annotate::HIGHLIGHT_OPACITY,
        };
        match restyle(&mk, Some(SizeLevel::Small), Some(BLUE), 96) {
            Shape::Marker { width, color, .. } => assert!(near(width, 10.0) && color == BLUE),
            o => panic!("{o:?}"),
        }
        match restyle(&text(), Some(SizeLevel::Huge), None, 96) {
            Shape::Text { size, origin, .. } => {
                assert!(near(size, 36.0));
                assert_eq!(origin, p(10.0, 10.0));
            }
            o => panic!("{o:?}"),
        }
        let st = Shape::Step {
            center: p(5.0, 5.0),
            radius: 13.0,
            n: 4,
            color: RED,
        };
        match restyle(&st, Some(SizeLevel::Small), None, 144) {
            Shape::Step {
                radius, n, center, ..
            } => assert!(near(radius, 15.0) && n == 4 && center == p(5.0, 5.0)),
            o => panic!("{o:?}"),
        }
        let b = blur(p(0.0, 0.0), p(9.0, 9.0), RED);
        assert_eq!(restyle(&b, Some(SizeLevel::Huge), Some(BLUE), 96), b);
        let l = Shape::Line {
            a: p(0.0, 0.0),
            b: p(9.0, 9.0),
            width: 3.0,
            color: RED,
        };
        match restyle(&l, None, Some(BLUE), 96) {
            Shape::Line { color, width, .. } => assert!(color == BLUE && width == 3.0),
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn wrong_grip_returns_original() {
        let r = rect(p(10.0, 10.0), p(50.0, 50.0));
        assert_eq!(tf(r.clone(), Grip::End(0), p(0.0, 0.0), false, 2.0), r);
        // Số thực lạ không panic.
        let _ = tf(
            r,
            Grip::Box(Handle::SE),
            p(f32::NAN, f32::INFINITY),
            true,
            2.0,
        );
    }

    // ---- Task 112: Box và Label

    fn styled_box(oval: bool, fill: Option<Rgb>) -> Shape {
        Shape::Box {
            a: p(10.0, 10.0),
            b: p(60.0, 40.0),
            oval,
            style: crate::annotate::Style {
                fill,
                ..crate::annotate::Style::new(RED, 4.0)
            },
        }
    }

    #[test]
    fn box_hit_tests_border_and_only_a_filled_inside() {
        let hollow = styled_box(false, None);
        let filled = styled_box(false, Some(BLUE));
        let centre = p(35.0, 25.0);
        assert!(!hit_shape(&hollow, Z, centre, 2.0));
        assert!(hit_shape(&filled, Z, centre, 2.0));
        assert!(hit_shape(&hollow, Z, p(10.0, 25.0), 2.0));
        assert!(!hit_shape(&filled, Z, p(200.0, 200.0), 2.0));
        let oval_hollow = styled_box(true, None);
        let oval_filled = styled_box(true, Some(BLUE));
        assert!(!hit_shape(&oval_hollow, Z, centre, 2.0));
        assert!(hit_shape(&oval_filled, Z, centre, 2.0));
        // Góc khung nằm ngoài elip: nền không trúng.
        assert!(!hit_shape(&oval_filled, Z, p(12.0, 12.0), 0.5));
        // Nền trong suốt hoàn toàn không bắt chuột.
        let ghost = Shape::Box {
            a: p(10.0, 10.0),
            b: p(60.0, 40.0),
            oval: false,
            style: crate::annotate::Style {
                fill: Some(BLUE),
                opacity: 0.0,
                ..crate::annotate::Style::new(RED, 0.0)
            },
        };
        assert!(!hit_shape(&ghost, Z, centre, 0.0));
    }

    #[test]
    fn box_handles_move_and_resize_keep_the_style() {
        let b = styled_box(true, Some(BLUE));
        assert_eq!(handles(&b, Z).len(), 8);
        let g = Gesture {
            index: 0,
            grip: Grip::Box(Handle::SE),
            start: p(60.0, 40.0),
            original: b.clone(),
            original_bounds: Z,
        };
        let img = Image {
            width: 1,
            height: 1,
            bgra: vec![0; 4],
        };
        match transform(&g, p(90.0, 70.0), false, 6.0, &img) {
            Shape::Box {
                a,
                b: bb,
                oval,
                style,
            } => {
                assert_eq!((a, bb, oval), (p(10.0, 10.0), p(90.0, 70.0), true));
                assert_eq!(Some(style), b.style());
            }
            other => panic!("{other:?}"),
        }
        let mv = Gesture {
            grip: Grip::Move,
            start: p(0.0, 0.0),
            ..g
        };
        match transform(&mv, p(5.0, 7.0), false, 6.0, &img) {
            Shape::Box { a, style, .. } => {
                assert_eq!(a, p(15.0, 17.0));
                assert_eq!(Some(style), b.style());
            }
            other => panic!("{other:?}"),
        }
    }

    // ---- Task 113: Highlight hình chữ nhật

    fn hl(opacity: f32) -> Shape {
        Shape::Highlight {
            a: p(10.0, 10.0),
            b: p(60.0, 40.0),
            color: BLUE,
            opacity,
        }
    }

    #[test]
    fn highlight_hit_inside_only_while_visible() {
        let h = hl(0.4);
        assert!(hit_shape(&h, Z, p(35.0, 25.0), 2.0));
        assert!(hit_shape(&h, Z, p(11.0, 25.0), 0.0));
        assert!(!hit_shape(&h, Z, p(200.0, 200.0), 2.0));
        assert!(!hit_shape(&hl(0.0), Z, p(35.0, 25.0), 0.0));
        // Kéo ngược chiều vẫn trúng như nhau.
        let rev = Shape::Highlight {
            a: p(60.0, 40.0),
            b: p(10.0, 10.0),
            color: BLUE,
            opacity: 0.4,
        };
        assert!(hit_shape(&rev, Z, p(35.0, 25.0), 0.0));
    }

    #[test]
    fn highlight_has_eight_handles_moves_and_resizes() {
        let h = hl(0.5);
        assert_eq!(handles(&h, Z).len(), 8);
        let g = Gesture {
            index: 0,
            grip: Grip::Box(Handle::SE),
            start: p(60.0, 40.0),
            original: h.clone(),
            original_bounds: Z,
        };
        let img = Image {
            width: 1,
            height: 1,
            bgra: vec![0; 4],
        };
        match transform(&g, p(90.0, 70.0), false, 6.0, &img) {
            Shape::Highlight {
                a,
                b,
                color,
                opacity,
            } => assert_eq!(
                (a, b, color, opacity),
                (p(10.0, 10.0), p(90.0, 70.0), BLUE, 0.5)
            ),
            other => panic!("{other:?}"),
        }
        let mv = Gesture {
            grip: Grip::Move,
            start: p(0.0, 0.0),
            ..g
        };
        match transform(&mv, p(5.0, 7.0), false, 6.0, &img) {
            Shape::Highlight { a, b, opacity, .. } => {
                assert_eq!((a, b, opacity), (p(15.0, 17.0), p(65.0, 47.0), 0.5))
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn highlight_restyle_changes_color_opacity_and_marker_width() {
        match highlight_restyle(&hl(0.4), None, Some(0.7), Some(RED)) {
            Shape::Highlight { color, opacity, .. } => assert_eq!((color, opacity), (RED, 0.7)),
            other => panic!("{other:?}"),
        }
        // Độ trong suốt được kẹp; NaN giữ nguyên.
        match highlight_restyle(&hl(0.4), None, Some(9.0), None) {
            Shape::Highlight { opacity, .. } => assert_eq!(opacity, 1.0),
            other => panic!("{other:?}"),
        }
        match highlight_restyle(&hl(0.4), None, Some(f32::NAN), None) {
            Shape::Highlight { opacity, .. } => assert_eq!(opacity, 0.4),
            other => panic!("{other:?}"),
        }
        let mk = Shape::Marker {
            points: vec![p(1.0, 1.0), p(9.0, 9.0)],
            width: 10.0,
            color: RED,
            opacity: crate::annotate::HIGHLIGHT_OPACITY,
        };
        match highlight_restyle(&mk, Some(24.0), Some(0.9), Some(BLUE)) {
            Shape::Marker {
                points,
                width,
                color,
                opacity,
            } => assert_eq!((points.len(), width, color, opacity), (2, 24.0, BLUE, 0.9)),
            other => panic!("{other:?}"),
        }
        // Hình khác giữ nguyên.
        let r = rect(p(0.0, 0.0), p(5.0, 5.0));
        assert_eq!(highlight_restyle(&r, Some(3.0), Some(0.5), Some(BLUE)), r);
        // restyle chung đổi màu Highlight (nút Màu), bỏ qua cỡ.
        match restyle(&hl(0.4), Some(SizeLevel::Huge), Some(RED), 96) {
            Shape::Highlight { color, opacity, .. } => assert_eq!((color, opacity), (RED, 0.4)),
            other => panic!("{other:?}"),
        }
    }

    fn label() -> Shape {
        Shape::Label {
            origin: p(10.0, 10.0),
            text: "ab".into(),
            size: 20.0,
            color: RED,
            align: crate::annotate::Align::Center,
            style: crate::annotate::Style {
                fill: Some(BLUE),
                ..crate::annotate::Style::new(RED, 2.0)
            },
        }
    }

    #[test]
    fn label_behaves_like_text_for_hit_handles_move_and_scale() {
        let l = label();
        let bounds = Rect {
            x: 10,
            y: 10,
            w: 60,
            h: 30,
        };
        assert!(hit_shape(&l, bounds, p(30.0, 20.0), 2.0));
        assert!(!hit_shape(&l, bounds, p(300.0, 20.0), 2.0));
        assert_eq!(handles(&l, bounds).len(), 4);
        let img = Image {
            width: 1,
            height: 1,
            bgra: vec![0; 4],
        };
        let g = Gesture {
            index: 0,
            grip: Grip::Box(Handle::SE),
            start: p(70.0, 40.0),
            original: l.clone(),
            original_bounds: bounds,
        };
        match transform(&g, p(130.0, 70.0), false, 6.0, &img) {
            Shape::Label {
                size, align, style, ..
            } => {
                assert!(near_f(size, 40.0), "{size}");
                assert_eq!(align, crate::annotate::Align::Center);
                assert_eq!(Some(style), l.style());
            }
            other => panic!("{other:?}"),
        }
        let mv = Gesture {
            grip: Grip::Move,
            start: p(0.0, 0.0),
            ..g
        };
        match transform(&mv, p(3.0, 4.0), false, 6.0, &img) {
            Shape::Label { origin, .. } => assert_eq!(origin, p(13.0, 14.0)),
            other => panic!("{other:?}"),
        }
    }

    fn near_f(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn restyle_sets_box_stroke_and_label_text() {
        let b = styled_box(false, Some(BLUE));
        match restyle(&b, Some(SizeLevel::Huge), Some(BLUE), 96) {
            Shape::Box { style, .. } => {
                assert_eq!(style.stroke_width, 8.0);
                assert_eq!(style.stroke, BLUE);
                assert_eq!(style.fill, Some(BLUE));
            }
            other => panic!("{other:?}"),
        }
        match restyle(&label(), Some(SizeLevel::Small), Some(BLUE), 96) {
            Shape::Label {
                size, color, style, ..
            } => {
                assert_eq!((size, color), (14.0, BLUE));
                assert_eq!(style.stroke, RED);
            }
            other => panic!("{other:?}"),
        }
    }

    // ---- Task 114

    #[test]
    fn hidden_border_is_not_hit_but_the_fill_still_is() {
        let mk = |no_border, fill| Shape::Box {
            a: p(10.0, 10.0),
            b: p(60.0, 40.0),
            oval: false,
            style: crate::annotate::Style {
                fill,
                no_border,
                ..crate::annotate::Style::new(RED, 10.0)
            },
        };
        let edge = p(10.0, 25.0);
        // Có viền, không nền: trúng cạnh. Ẩn viền: cạnh không còn (ngoài dung sai).
        assert!(hit_shape(&mk(false, None), Z, edge, 1.0));
        assert!(!hit_shape(&mk(true, None), Z, p(6.0, 25.0), 1.0));
        // Nền vẫn trúng bên trong.
        assert!(hit_shape(&mk(true, Some(BLUE)), Z, p(35.0, 25.0), 1.0));
    }

    #[test]
    fn marker_opacity_is_kept_by_move_and_scale_and_changed_by_highlight_restyle() {
        let mk = Shape::Marker {
            points: vec![p(10.0, 10.0), p(40.0, 30.0)],
            width: 8.0,
            color: RED,
            opacity: 0.7,
        };
        let g = Gesture {
            index: 0,
            grip: Grip::Move,
            start: p(0.0, 0.0),
            original: mk.clone(),
            original_bounds: Z,
        };
        match transform(&g, p(5.0, 5.0), false, 6.0, &img()) {
            Shape::Marker { opacity, .. } => assert_eq!(opacity, 0.7),
            other => panic!("{other:?}"),
        }
        let g = Gesture {
            grip: Grip::Box(Handle::SE),
            ..g
        };
        match transform(&g, p(80.0, 60.0), false, 6.0, &img()) {
            Shape::Marker { opacity, .. } => assert_eq!(opacity, 0.7),
            other => panic!("{other:?}"),
        }
        // Đổi độ trong suốt: kẹp 0..=1, NaN giữ nguyên.
        for (want, v) in [(0.2, 0.2), (1.0, 7.0), (0.0, -2.0), (0.7, f32::NAN)] {
            match highlight_restyle(&mk, None, Some(v), None) {
                Shape::Marker { opacity, .. } => assert_eq!(opacity, want, "{v}"),
                other => panic!("{other:?}"),
            }
        }
        // Đổi cỡ qua `restyle` không đụng độ trong suốt.
        match restyle(&mk, Some(SizeLevel::Huge), Some(BLUE), 96) {
            Shape::Marker { opacity, .. } => assert_eq!(opacity, 0.7),
            other => panic!("{other:?}"),
        }
    }
}
