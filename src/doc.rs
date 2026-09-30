//! The annotation document and its geometry. Coordinates are image pixels with the
//! screenshot's top-left at the origin; annotations may sit outside the image.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct P {
    pub x: f32,
    pub y: f32,
}

impl P {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn dist(self, o: P) -> f32 {
        ((self.x - o.x).powi(2) + (self.y - o.y).powi(2)).sqrt()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct R {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl R {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn spanning(a: P, b: P) -> Self {
        let (x, y) = (a.x.min(b.x), a.y.min(b.y));
        Self::new(x, y, (a.x - b.x).abs(), (a.y - b.y).abs())
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn center(&self) -> P {
        P::new(self.x + self.w / 2., self.y + self.h / 2.)
    }

    pub fn contains(&self, p: P) -> bool {
        p.x >= self.x && p.x <= self.right() && p.y >= self.y && p.y <= self.bottom()
    }

    pub fn inflate(&self, d: f32) -> Self {
        Self::new(self.x - d, self.y - d, self.w + 2. * d, self.h + 2. * d)
    }

    pub fn union(&self, o: &R) -> Self {
        let (x, y) = (self.x.min(o.x), self.y.min(o.y));
        Self::new(x, y, self.right().max(o.right()) - x, self.bottom().max(o.bottom()) - y)
    }

    pub fn intersects(&self, o: &R) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }

    pub fn clamp(&self, bounds: &R) -> Self {
        let x = self.x.max(bounds.x);
        let y = self.y.max(bounds.y);
        Self::new(x, y, (self.right().min(bounds.right()) - x).max(0.), (self.bottom().min(bounds.bottom()) - y).max(0.))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Style {
    Elbow,
    Arrow,
}

impl Style {
    pub fn toggled(self) -> Self {
        match self {
            Style::Elbow => Style::Arrow,
            Style::Arrow => Style::Elbow,
        }
    }
}

/// Which edge of the image a callout's text box sits beyond.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Target {
    Point(P),
    Rect(R),
}

/// A connector from `anchor` (on the text box edge) to `target`. The box grows away
/// from the anchor, so typing never moves the connector.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Callout {
    pub target: Target,
    pub side: Side,
    pub anchor: P,
    pub text: String,
    pub style: Style,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Doc {
    /// Image pixels per point (2 on retina).
    pub scale: f32,
    pub callouts: Vec<Callout>,
}

/// Visual constants, converted from points to image pixels.
pub struct Dims {
    pub font: f32,
    pub line: f32,
    pub pad_x: f32,
    pub pad_y: f32,
    pub max_text_w: f32,
    pub min_text_w: f32,
    pub radius: f32,
    pub stroke: f32,
    pub dot: f32,
    pub head: f32,
    pub gap: f32,
    pub margin: f32,
    pub spacing: f32,
    pub drag_threshold: f32,
}

impl Dims {
    pub fn new(scale: f32) -> Self {
        let s = scale;
        Self {
            font: 15. * s,
            line: 21. * s,
            pad_x: 10. * s,
            pad_y: 6. * s,
            max_text_w: 280. * s,
            min_text_w: 6. * s,
            radius: 6. * s,
            stroke: 2.5 * s,
            dot: 4. * s,
            head: 12. * s,
            gap: 40. * s,
            margin: 14. * s,
            spacing: 12. * s,
            drag_threshold: 4. * s,
        }
    }
}

impl Callout {
    /// Box rect for a text block of the given size. Left/right boxes line their first
    /// text line up with the anchor and grow downward.
    pub fn box_rect(&self, text: (f32, f32), d: &Dims) -> R {
        let (w, h) = (text.0 + 2. * d.pad_x, text.1 + 2. * d.pad_y);
        let a = self.anchor;
        let first_line = d.pad_y + d.line / 2.;
        match self.side {
            Side::Right => R::new(a.x, a.y - first_line, w, h),
            Side::Left => R::new(a.x - w, a.y - first_line, w, h),
            Side::Top => R::new(a.x - w / 2., a.y - h, w, h),
            Side::Bottom => R::new(a.x - w / 2., a.y, w, h),
        }
    }

    /// Where the connector ends: the point itself, or the rect edge facing the box.
    pub fn tip(&self) -> P {
        let a = self.anchor;
        match self.target {
            Target::Point(p) => p,
            Target::Rect(r) => match self.side {
                Side::Right => P::new(r.right(), a.y.clamp(r.y, r.bottom())),
                Side::Left => P::new(r.x, a.y.clamp(r.y, r.bottom())),
                Side::Top => P::new(a.x.clamp(r.x, r.right()), r.y),
                Side::Bottom => P::new(a.x.clamp(r.x, r.right()), r.bottom()),
            },
        }
    }

    /// The tip relative to the anchor: how far out from the box, and how far to the side
    /// (along the image edge).
    fn reach(&self) -> (f32, f32) {
        let (a, t) = (self.anchor, self.tip());
        if matches!(self.side, Side::Left | Side::Right) { (t.x - a.x, t.y - a.y) } else { (t.y - a.y, t.x - a.x) }
    }

    /// The tip's position along the image edge, and how far in from the edge it sits. The
    /// depth is off by a constant per side, so it compares tips whose boxes sit at different
    /// distances out.
    fn tip_along(&self) -> (f32, f32) {
        let t = self.tip();
        match self.side {
            Side::Left => (t.y, t.x),
            Side::Right => (t.y, -t.x),
            Side::Top => (t.x, t.y),
            Side::Bottom => (t.x, -t.y),
        }
    }

    /// Where a 45° line into the tip from after it (the way boxes get pushed) meets the image
    /// edge. Boxes on a side line up in this order.
    fn foot(&self) -> f32 {
        let (t, h) = self.tip_along();
        t + h
    }

    /// Connector polyline from anchor to tip. Elbow runs straight out of the box, then
    /// turns 45° onto the tip. With a `turn`, it first turns sideways that far out from the
    /// box, then enters the tip at 45°.
    pub fn connector(&self, turn: Option<f32>) -> Vec<P> {
        let (a, t) = (self.anchor, self.tip());
        let (out, side) = self.reach();
        if self.style == Style::Arrow || side.abs() < 0.5 {
            return vec![a, t];
        }
        // Points `o` out from the box and `s` to the side, from the anchor.
        let at = |o: f32, s: f32| match self.side {
            Side::Left | Side::Right => P::new(a.x + o * out.signum(), a.y + s * side.signum()),
            Side::Top | Side::Bottom => P::new(a.x + s * side.signum(), a.y + o * out.signum()),
        };
        match turn {
            None => vec![a, at(out.abs() - side.abs(), 0.), t],
            Some(turn) => {
                let turn = turn.min(out.abs());
                vec![a, at(turn, 0.), at(turn, side.abs() - (out.abs() - turn)), t]
            }
        }
    }
}

/// How far out from its box each connector turns sideways, for those whose tip is too far to
/// the side to reach at 45°. Connectors turning the same way nest: in `foot` order, each turns
/// nearer the image than the one before, counted from the box nearest the image so boxes
/// dragged farther out still nest. All turns fit in the gap between the boxes and the image,
/// clear of other sides' boxes.
pub fn turns(callouts: &[Callout], d: &Dims) -> Vec<Option<f32>> {
    let nest: Vec<Option<(Side, bool, f32, f32)>> = callouts
        .iter()
        .map(|c| {
            let (out, side) = c.reach();
            let sideways = c.style == Style::Elbow && side.abs() >= 0.5 && out.abs() - side.abs() < d.spacing;
            // Turning toward the start of the edge nests by foot; the other way, mirrored.
            let (t, h) = c.tip_along();
            let row = match c.side {
                Side::Left => -c.anchor.x,
                Side::Right => c.anchor.x,
                Side::Top => -c.anchor.y,
                Side::Bottom => c.anchor.y,
            };
            sideways.then(|| (c.side, side < 0., if side < 0. { t + h } else { h - t }, row))
        })
        .collect();
    nest.iter()
        .map(|n| {
            let &(side, back, key, row) = n.as_ref()?;
            let group = || nest.iter().flatten().filter(|&&(s, b, _, _)| s == side && b == back);
            let step = d.spacing.min(d.gap / (group().count() + 1) as f32);
            let nearest = group().map(|g| g.3).fold(f32::MAX, f32::min);
            Some(row - nearest + (group().filter(|g| g.2 < key).count() + 1) as f32 * step)
        })
        .collect()
}

/// Pick the image edge nearest the target and an anchor just beyond it, level with the
/// target. `separate` moves it off any neighbour.
pub fn place(target: &Target, image: (f32, f32), d: &Dims) -> (Side, P) {
    let (w, h) = image;
    let region = match *target {
        Target::Point(p) => R::new(p.x, p.y, 0., 0.),
        Target::Rect(r) => r,
    };
    let c = region.center();
    let side = [
        (Side::Right, w - region.right()),
        (Side::Left, region.x),
        (Side::Bottom, h - region.bottom()),
        (Side::Top, region.y),
    ]
    .into_iter()
    .min_by(|a, b| a.1.total_cmp(&b.1))
    .map(|(s, _)| s)
    .unwrap();

    let anchor = match side {
        Side::Right => P::new(w + d.gap, c.y),
        Side::Left => P::new(-d.gap, c.y),
        Side::Top => P::new(c.x, -d.gap),
        Side::Bottom => P::new(c.x, h + d.gap),
    };
    (side, anchor)
}

/// Push boxes apart along their side, so growing text never covers a neighbour and
/// connectors don't cross. Returns each callout's anchor after the push.
///
/// A connector sits, at each height above the box row, where its anchor clamps into the 45°
/// cone below its tip. So with boxes in `foot` order, a box only has to clear the one before
/// it, and the tip of any earlier connector that reaches its tip from before it with a cone
/// edge ahead of this one's. Pushing a box later along the side satisfies both, and only
/// later boxes move.
pub fn separate(callouts: &[Callout], boxes: &mut [R], d: &Dims) -> Vec<P> {
    let mut anchors: Vec<P> = callouts.iter().map(|c| c.anchor).collect();
    let mut placed: Vec<usize> = Vec::new();
    for side in [Side::Left, Side::Right, Side::Top, Side::Bottom] {
        let along_x = matches!(side, Side::Top | Side::Bottom);
        let mut order: Vec<usize> = (0..callouts.len()).filter(|&i| callouts[i].side == side).collect();
        order.sort_by(|&a, &b| callouts[a].foot().total_cmp(&callouts[b].foot()));
        let along = |p: P| if along_x { p.x } else { p.y };
        let behind = |b: &R, p: &R| if along_x { b.x < p.right() + d.spacing } else { b.y < p.bottom() + d.spacing };
        let mut done: Vec<usize> = Vec::new();
        for i in order {
            let (t, h) = callouts[i].tip_along();
            let clear = done
                .iter()
                .filter_map(|&j| {
                    let (tj, hj) = callouts[j].tip_along();
                    (along(anchors[j]) < tj && tj - hj > t - h).then(|| tj - hj + h.min(hj) + d.spacing)
                })
                .fold(f32::MIN, f32::max);
            let mut push = (clear - along(anchors[i])).max(0.);
            loop {
                let shift = if along_x { P::new(push, 0.) } else { P::new(0., push) };
                boxes[i].x += shift.x;
                boxes[i].y += shift.y;
                anchors[i] = P::new(anchors[i].x + shift.x, anchors[i].y + shift.y);
                let clash = placed.iter().copied().find(|&j| boxes[i].intersects(&boxes[j].inflate(d.spacing)));
                let Some(j) = clash.or(done.last().copied().filter(|&p| behind(&boxes[i], &boxes[p]))) else { break };
                push = if along_x { boxes[j].right() + d.spacing - boxes[i].x } else { boxes[j].bottom() + d.spacing - boxes[i].y };
            }
            placed.push(i);
            done.push(i);
        }
    }
    anchors
}
