//! Every capture lives in its own folder: the untouched screenshot, the annotations,
//! and a thumbnail of the latest result.

use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, Local};
use tiny_skia::Pixmap;

use crate::{doc::Doc, render};

pub struct Capture {
    pub id: String,
    pub image: Pixmap,
    pub doc: Doc,
}

pub struct Entry {
    pub id: String,
    pub taken: DateTime<Local>,
    pub thumb: PathBuf,
}

fn root() -> PathBuf {
    dirs::data_dir().unwrap().join("Annotate").join("history")
}

fn dir(id: &str) -> PathBuf {
    root().join(id)
}

/// Let the user drag out a region with the system marquee. `None` if they cancelled.
pub fn capture_region() -> Option<Capture> {
    let millis = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    let id = format!("{millis:015}");
    fs::create_dir_all(dir(&id)).ok()?;
    let path = dir(&id).join("original.png");
    let status = Command::new("/usr/sbin/screencapture").args(["-i", "-x"]).arg(&path).status();
    if status.is_err() || !path.exists() {
        fs::remove_dir_all(dir(&id)).ok();
        return None;
    }
    let capture = load(&id);
    if capture.is_none() {
        fs::remove_dir_all(dir(&id)).ok();
    }
    capture
}

pub fn load(id: &str) -> Option<Capture> {
    let bytes = fs::read(dir(id).join("original.png")).ok()?;
    let rgba = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let image = render::pixmap_from_rgba(rgba.width(), rgba.height(), rgba.into_raw());
    let doc = fs::read(dir(id).join("doc.json"))
        .ok()
        .and_then(|json| serde_json::from_slice(&json).ok())
        .unwrap_or_else(|| Doc { scale: png_scale(&bytes), callouts: Vec::new() });
    Some(Capture { id: id.to_string(), image, doc })
}

/// Pixels per point, from the PNG's DPI (screencapture writes 144 on retina).
fn png_scale(bytes: &[u8]) -> f32 {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let dims = decoder.read_info().ok().and_then(|r| r.info().pixel_dims);
    match dims {
        Some(png::PixelDimensions { xppu, unit: png::Unit::Meter, .. }) => (xppu as f32 * 0.0254 / 72.).round().max(1.),
        _ => 1.,
    }
}

pub fn save(id: &str, doc: &Doc, result: &Pixmap) {
    let dir = dir(id);
    if let Ok(json) = serde_json::to_vec_pretty(doc) {
        fs::write(dir.join("doc.json"), json).ok();
    }
    // Fresh name each save so GPUI's path-keyed image cache shows the new thumbnail.
    let old: Vec<PathBuf> = thumbs(&dir);
    let width = 480.min(result.width());
    let height = ((result.height() as f32 * width as f32 / result.width() as f32).round() as u32).max(1);
    let rgba = image::RgbaImage::from_raw(result.width(), result.height(), result.data().to_vec()).unwrap();
    let thumb = image::imageops::resize(&rgba, width, height, image::imageops::FilterType::Triangle);
    let millis = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    if thumb.save(dir.join(format!("thumb-{millis}.png"))).is_ok() {
        for path in old {
            fs::remove_file(path).ok();
        }
    }
}

fn thumbs(dir: &PathBuf) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("thumb-")))
        .collect()
}

/// Newest first.
pub fn list() -> Vec<Entry> {
    let mut entries: Vec<Entry> = fs::read_dir(root())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let id = e.file_name().to_string_lossy().into_owned();
            // Ids are capture times in Unix millis.
            let taken = DateTime::from_timestamp_millis(id.parse().ok()?)?.with_timezone(&Local);
            let thumb = thumbs(&e.path()).into_iter().max().unwrap_or_else(|| e.path().join("original.png"));
            thumb.exists().then_some(Entry { id, taken, thumb })
        })
        .collect();
    entries.sort_by(|a, b| b.id.cmp(&a.id));
    entries
}
