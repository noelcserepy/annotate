//! Composites a document into a bitmap. The editor shows this bitmap and the clipboard
//! gets it, so what you see is exactly what you copy.

use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use cosmic_text::{Attrs, Buffer, Edit, Editor, Family, FontSystem, Metrics, Shaping, SwashCache, Weight, Wrap, fontdb};
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, PixmapPaint, PremultipliedColorU8, Stroke, Transform,
};

use crate::doc::{Callout, Dims, Doc, P, R, Style, Target, separate, turns};

const INTER: &[u8] = include_bytes!("../assets/Inter-SemiBold.ttf");
const RED: [u8; 3] = [0xDC, 0x26, 0x26];

pub struct Text {
    pub fonts: FontSystem,
    swash: SwashCache,
}

static TEXT: OnceLock<Mutex<Text>> = OnceLock::new();

/// Shared font system. Loading system fonts (for emoji and other scripts) is slow, so
/// `main` warms this up on a background thread at launch.
pub fn text() -> MutexGuard<'static, Text> {
    TEXT.get_or_init(|| {
        let fonts = FontSystem::new_with_fonts([fontdb::Source::Binary(Arc::new(INTER))]);
        Mutex::new(Text { fonts, swash: SwashCache::new() })
    })
    .lock()
    .unwrap()
}

fn attrs() -> Attrs<'static> {
    Attrs::new().family(Family::Name("Inter")).weight(Weight::SEMIBOLD)
}

pub fn text_buffer(fonts: &mut FontSystem, text: &str, d: &Dims) -> Buffer {
    let mut buffer = Buffer::new(fonts, Metrics::new(d.font, d.line));
    buffer.set_wrap(Wrap::WordOrGlyph);
    buffer.set_size(Some(d.max_text_w), None);
    buffer.set_text(text, &attrs(), Shaping::Advanced, None);
    buffer.shape_until_scroll(fonts, false);
    buffer
}

pub fn buffer_text(buffer: &Buffer) -> String {
    buffer.lines.iter().map(|l| l.text()).collect::<Vec<_>>().join("\n")
}

fn measure(buffer: &Buffer, d: &Dims) -> (f32, f32) {
    let (mut w, mut lines) = (0f32, 0);
    for run in buffer.layout_runs() {
        w = w.max(run.line_w);
        lines += 1;
    }
    (w.max(d.min_text_w).ceil(), lines.max(1) as f32 * d.line)
}

pub struct Scene<'a> {
    pub image: &'a Pixmap,
    pub fill: Color,
    pub doc: &'a Doc,
    pub dims: &'a Dims,
    pub editing: Option<(usize, &'a mut Editor<'static>)>,
    pub selected: Option<usize>,
    pub marquee: Option<R>,
}

pub struct Frame {
    pub pixmap: Pixmap,
    /// Canvas extent in document coordinates.
    pub bounds: R,
    pub boxes: Vec<R>,
    /// Where each connector starts, after `separate` pushed boxes apart.
    pub anchors: Vec<P>,
}

pub fn compose(scene: Scene) -> Frame {
    let Scene { image, fill, doc, dims: d, mut editing, selected, marquee } = scene;
    let mut guard = text();
    let Text { fonts, swash } = &mut *guard;

    let mut buffers: Vec<Option<Buffer>> = Vec::new();
    let mut boxes = Vec::new();
    for (i, c) in doc.callouts.iter().enumerate() {
        let size = match &mut editing {
            Some((e, editor)) if *e == i => {
                editor.shape_as_needed(fonts, false);
                buffers.push(None);
                editor.with_buffer(|b| measure(b, d))
            }
            _ => {
                let b = text_buffer(fonts, &c.text, d);
                let size = measure(&b, d);
                buffers.push(Some(b));
                size
            }
        };
        boxes.push(c.box_rect(size, d));
    }
    let anchors = separate(&doc.callouts, &mut boxes, d);
    let laid: Vec<Callout> = doc.callouts.iter().zip(&anchors).map(|(c, &anchor)| Callout { anchor, ..c.clone() }).collect();
    let paths: Vec<Vec<P>> = laid.iter().zip(turns(&laid, d)).map(|(c, turn)| c.connector(turn)).collect();

    let image_rect = R::new(0., 0., image.width() as f32, image.height() as f32);
    let mut bounds = image_rect;
    let reach = d.stroke + d.dot;
    for (i, c) in doc.callouts.iter().enumerate() {
        bounds = bounds.union(&boxes[i].inflate(d.margin));
        for p in &paths[i] {
            bounds = bounds.union(&R::new(p.x, p.y, 0., 0.).inflate(d.margin.max(reach)));
        }
        if let Target::Rect(r) = c.target {
            bounds = bounds.union(&r.inflate(reach));
        }
    }
    let bounds = R::new(
        bounds.x.floor(),
        bounds.y.floor(),
        (bounds.right().ceil() - bounds.x.floor()).max(1.),
        (bounds.bottom().ceil() - bounds.y.floor()).max(1.),
    );

    let mut pixmap = Pixmap::new(bounds.w as u32, bounds.h as u32).unwrap();
    pixmap.fill(fill);
    pixmap.draw_pixmap(
        -bounds.x as i32,
        -bounds.y as i32,
        image.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    let t = Transform::from_translate(-bounds.x, -bounds.y);

    for (i, c) in doc.callouts.iter().enumerate() {
        draw_connector(&mut pixmap, t, c, &paths[i], d);
    }
    if let Some(r) = marquee {
        draw_rect_target(&mut pixmap, t, r, d);
    }
    for (i, b) in boxes.iter().enumerate() {
        if selected == Some(i) {
            let ring = rounded_rect(b.inflate(d.stroke), d.radius + d.stroke);
            pixmap.fill_path(&ring, &paint([255, 255, 255], 255), FillRule::Winding, t, None);
        }
        pixmap.fill_path(&rounded_rect(*b, d.radius), &paint(RED, 255), FillRule::Winding, t, None);

        let origin = (b.x + d.pad_x - bounds.x, b.y + d.pad_y - bounds.y);
        let mut blit = |x: i32, y: i32, w: u32, h: u32, color: cosmic_text::Color| {
            blend_rect(&mut pixmap, origin.0 as i32 + x, origin.1 as i32 + y, w, h, color.as_rgba());
        };
        let white = cosmic_text::Color::rgb(255, 255, 255);
        match (&mut editing, &mut buffers[i]) {
            (Some((_, editor)), None) => {
                let clear = cosmic_text::Color::rgba(0, 0, 0, 0);
                let selection = cosmic_text::Color::rgba(255, 255, 255, 80);
                editor.draw(fonts, swash, white, clear, selection, white, &mut blit);
                if let Some((x, y)) = editor.cursor_position() {
                    let w = (d.stroke * 0.6).round().max(1.) as u32;
                    blit(x, y + (d.line * 0.12) as i32, w, (d.line * 0.76) as u32, white);
                }
            }
            (_, Some(buffer)) => buffer.draw(fonts, swash, white, &mut blit),
            _ => {}
        }
    }

    Frame { pixmap, bounds, boxes, anchors }
}

fn paint(rgb: [u8; 3], a: u8) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(rgb[0], rgb[1], rgb[2], a);
    p.anti_alias = true;
    p
}

fn polyline(points: &[P]) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(points[0].x, points[0].y);
    for p in &points[1..] {
        pb.line_to(p.x, p.y);
    }
    pb.finish()
}

fn red_stroke(pixmap: &mut Pixmap, t: Transform, path: &tiny_skia::Path, d: &Dims) {
    let stroke = Stroke { width: d.stroke, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };
    pixmap.stroke_path(path, &paint(RED, 255), &stroke, t, None);
}

fn draw_rect_target(pixmap: &mut Pixmap, t: Transform, r: R, d: &Dims) {
    red_stroke(pixmap, t, &rounded_rect(r, d.radius * 0.66), d);
}

fn draw_connector(pixmap: &mut Pixmap, t: Transform, c: &Callout, path: &[P], d: &Dims) {
    if let Target::Rect(r) = c.target {
        draw_rect_target(pixmap, t, r, d);
    }
    let tip = *path.last().unwrap();
    let before = path[path.len() - 2];
    match c.style {
        Style::Elbow => {
            if let Some(line) = polyline(path) {
                red_stroke(pixmap, t, &line, d);
            }
            if matches!(c.target, Target::Point(_)) {
                let dot = PathBuilder::from_circle(tip.x, tip.y, d.dot).unwrap();
                pixmap.fill_path(&dot, &paint(RED, 255), FillRule::Winding, t, None);
            }
        }
        Style::Arrow => {
            let len = tip.dist(before).max(0.001);
            let dir = P::new((tip.x - before.x) / len, (tip.y - before.y) / len);
            let head = d.head.min(len);
            let base = P::new(tip.x - dir.x * head, tip.y - dir.y * head);
            let side = P::new(-dir.y * head * 0.45, dir.x * head * 0.45);
            let shaft_end = P::new(tip.x - dir.x * head * 0.5, tip.y - dir.y * head * 0.5);
            if let Some(line) = polyline(&[before, shaft_end]) {
                red_stroke(pixmap, t, &line, d);
            }
            let mut pb = PathBuilder::new();
            pb.move_to(tip.x, tip.y);
            pb.line_to(base.x + side.x, base.y + side.y);
            pb.line_to(base.x - side.x, base.y - side.y);
            pb.close();
            pixmap.fill_path(&pb.finish().unwrap(), &paint(RED, 255), FillRule::Winding, t, None);
        }
    }
}

fn rounded_rect(r: R, radius: f32) -> tiny_skia::Path {
    let k = radius.min(r.w / 2.).min(r.h / 2.);
    let c = k * 0.4477; // distance from corner to cubic control points for a circular arc
    let (x0, y0, x1, y1) = (r.x, r.y, r.right(), r.bottom());
    let mut pb = PathBuilder::new();
    pb.move_to(x0 + k, y0);
    pb.line_to(x1 - k, y0);
    pb.cubic_to(x1 - c, y0, x1, y0 + c, x1, y0 + k);
    pb.line_to(x1, y1 - k);
    pb.cubic_to(x1, y1 - c, x1 - c, y1, x1 - k, y1);
    pb.line_to(x0 + k, y1);
    pb.cubic_to(x0 + c, y1, x0, y1 - c, x0, y1 - k);
    pb.line_to(x0, y0 + k);
    pb.cubic_to(x0, y0 + c, x0 + c, y0, x0 + k, y0);
    pb.close();
    pb.finish().unwrap()
}

fn blend_rect(pixmap: &mut Pixmap, x: i32, y: i32, w: u32, h: u32, [r, g, b, a]: [u8; 4]) {
    if a == 0 {
        return;
    }
    let (pw, ph) = (pixmap.width() as i32, pixmap.height() as i32);
    let pixels = pixmap.pixels_mut();
    let a32 = a as u32;
    for py in y.max(0)..(y + h as i32).min(ph) {
        for px in x.max(0)..(x + w as i32).min(pw) {
            let dst = &mut pixels[(py * pw + px) as usize];
            let mix = |s: u8, d: u8| ((s as u32 * a32 + d as u32 * (255 - a32)) / 255) as u8;
            let alpha = (a32 + dst.alpha() as u32 * (255 - a32) / 255) as u8;
            let (r, g, b) = (mix(r, dst.red()), mix(g, dst.green()), mix(b, dst.blue()));
            *dst = PremultipliedColorU8::from_rgba(r.min(alpha), g.min(alpha), b.min(alpha), alpha).unwrap();
        }
    }
}

/// Most common color along the image border, used to fill the extended canvas.
pub fn edge_color(image: &Pixmap) -> Color {
    let (w, h) = (image.width(), image.height());
    let mut border = Vec::new();
    for x in 0..w {
        border.push(image.pixel(x, 0).unwrap());
        border.push(image.pixel(x, h - 1).unwrap());
    }
    for y in 0..h {
        border.push(image.pixel(0, y).unwrap());
        border.push(image.pixel(w - 1, y).unwrap());
    }
    let key = |c: &PremultipliedColorU8| ((c.red() as u32 >> 3) << 10) | ((c.green() as u32 >> 3) << 5) | (c.blue() as u32 >> 3);
    let mut counts = std::collections::HashMap::new();
    for c in &border {
        *counts.entry(key(c)).or_insert(0u32) += 1;
    }
    let best = counts.into_iter().max_by_key(|(_, n)| *n).map(|(k, _)| k).unwrap_or(0);
    let (mut sum, mut n) = ([0u64; 3], 0u64);
    for c in border.iter().filter(|c| key(c) == best) {
        let c = c.demultiply();
        sum[0] += c.red() as u64;
        sum[1] += c.green() as u64;
        sum[2] += c.blue() as u64;
        n += 1;
    }
    let n = n.max(1);
    Color::from_rgba8((sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8, 255)
}

pub fn pixmap_from_rgba(width: u32, height: u32, mut rgba: Vec<u8>) -> Pixmap {
    for px in rgba.chunks_exact_mut(4) {
        let a = px[3] as u32;
        for c in &mut px[..3] {
            *c = (*c as u32 * a / 255) as u8;
        }
    }
    Pixmap::from_vec(rgba, tiny_skia::IntSize::from_wh(width, height).unwrap()).unwrap()
}

fn demultiplied(pixmap: &Pixmap) -> Vec<u8> {
    pixmap.pixels().iter().flat_map(|p| {
        let c = p.demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    }).collect()
}

/// PNG with its DPI set so apps paste retina captures at their on-screen size.
pub fn encode_png(pixmap: &Pixmap, scale: f32) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, pixmap.width(), pixmap.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let ppm = (72. * scale / 0.0254).round() as u32;
    encoder.set_pixel_dims(Some(png::PixelDimensions { xppu: ppm, yppu: ppm, unit: png::Unit::Meter }));
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&demultiplied(pixmap)).unwrap();
    writer.finish().unwrap();
    out
}

/// GPUI wants BGRA.
pub fn to_render_image(pixmap: &Pixmap) -> gpui::RenderImage {
    let mut bgra = demultiplied(pixmap);
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    let buffer = image::RgbaImage::from_raw(pixmap.width(), pixmap.height(), bgra).unwrap();
    gpui::RenderImage::new(vec![image::Frame::new(buffer)])
}
