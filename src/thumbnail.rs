//! A picture of a build for its card: an isometric view from the south-east
//! and above, each block drawn by `blockrender` from the game's own block
//! models and textures, as the game draws it. The game's files are fetched
//! once from the public copy the 3D viewer's browser uses and kept beside
//! the database.

use std::path::{Component, Path};
use std::time::Duration;

use anyhow::{Context, Result};
use tiny_skia::Pixmap;

use crate::schem::Blocks;

/// Names the way thumbnails are drawn, in their addresses and their kept
/// files, so drawing them anew changes both.
pub const RENDERER: &str = "t6";

/// Where drawn thumbnails are kept, beside the database, so a restart
/// draws none again.
pub const DRAWN_DIR: &str = "thumbnails";

pub const WIDTH: u16 = 640;
pub const HEIGHT: u16 = 480;
const PADDING: f32 = 24.0;

/// Where the game's files are kept once fetched, beside the database, so
/// each is fetched once.
const ASSET_DIR: &str = "assets";

/// The public copy of one release's resource files the game's blockstates,
/// models, and textures are fetched from.
const ASSETS: &str = "https://cdn.jsdelivr.net/gh/InventivetalentDev/minecraft-assets@1.21.4/assets/minecraft";

/// What draws thumbnails, keeping each file it reads and each block state
/// it works out for the next.
pub type Renderer = blockrender::Renderer<Assets>;

/// The game's files, from the folder they are kept in, else fetched and
/// kept. Fetching waits on the runtime it was made in, so a renderer using
/// it draws off the async workers, as `spawn_blocking` does.
pub struct Assets {
    http: reqwest::Client,
    runtime: tokio::runtime::Handle,
}

impl Assets {
    /// Must be called within the Tokio runtime it is to fetch with.
    pub fn new(http: reqwest::Client) -> Self {
        Assets {
            http,
            runtime: tokio::runtime::Handle::current(),
        }
    }

    /// The file at `path` under the resource files' root, kept or fetched.
    fn fetch(&self, path: &str) -> Option<Vec<u8>> {
        // Paths come from fetched models, so none may climb out of the
        // folder they are kept in.
        if !Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        {
            return None;
        }
        let kept = Path::new(ASSET_DIR).join(path);
        if let Ok(bytes) = std::fs::read(&kept) {
            return Some(bytes);
        }
        let bytes = self.runtime.block_on(async {
            let response = self
                .http
                .get(format!("{ASSETS}/{path}"))
                .timeout(Duration::from_secs(20))
                .send()
                .await
                .ok()?
                .error_for_status()
                .ok()?;
            Some(response.bytes().await.ok()?.to_vec())
        })?;
        if let Some(folder) = kept.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        let _ = std::fs::write(&kept, &bytes);
        Some(bytes)
    }
}

impl blockrender::Assets for Assets {
    fn json(&self, path: &str) -> Option<serde_json::Value> {
        serde_json::from_slice(&self.fetch(path)?).ok()
    }

    fn texture(&self, path: &str) -> Option<Pixmap> {
        Pixmap::decode_png(&self.fetch(path)?).ok()
    }
}

/// The build as a WebP, `WIDTH` by `HEIGHT` on a see-through background.
pub fn render_with<A: blockrender::Assets>(
    blocks: &Blocks,
    renderer: &blockrender::Renderer<A>,
) -> Result<Vec<u8>> {
    let view = blockrender::View {
        width: u32::from(WIDTH),
        height: u32::from(HEIGHT),
        padding: PADDING,
    };
    render_view(blocks, renderer, view)
}

/// The build as a WebP drawn as `view` says.
pub fn render_view<A: blockrender::Assets>(
    blocks: &Blocks,
    renderer: &blockrender::Renderer<A>,
    view: blockrender::View,
) -> Result<Vec<u8>> {
    let pixmap = renderer
        .render(blocks.iter(), view)
        .context("thumbnail size is zero")?;
    Ok(webp(&pixmap))
}

/// `pixmap` as a lossy WebP: textured faces compress poorly as PNG, and a
/// WebP of them is a fraction the size, its see-through background kept.
fn webp(pixmap: &Pixmap) -> Vec<u8> {
    let rgba: Vec<u8> = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    let encoder =
        webp::Encoder::from_rgba(&rgba, pixmap.width(), pixmap.height());
    encoder.encode(82.0).to_vec()
}
