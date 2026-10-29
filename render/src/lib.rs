//! Draws Minecraft blocks as the game's own block models show them, in an
//! isometric view from the south-east and above, for pictures of builds.
//!
//! Everything comes from the game's resource files, read through
//! [`Assets`]: a block state picks its models from its blockstate file,
//! each model takes elements and textures from its parents, and every
//! element's faces are painted with their textures, far faces first. A
//! block whose model or textures can't be found is a cube of one colour.

// Pixel and texel arithmetic turns small whole numbers into floats and
// back throughout, every value far inside both types' exact ranges.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

mod blockstate;
mod draw;
mod fallback;
mod geometry;
mod model;
mod texture;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::Value;
pub use tiny_skia::Pixmap;

pub use fallback::dye_color;
use geometry::Block;
use model::Model;
use texture::Texture;

/// A block's place: x east, y up, z south.
pub type Pos = [i32; 3];

/// Where the game's resource files come from, each named by its path
/// under `assets/minecraft/`, such as `blockstates/lever.json`,
/// `models/block/lever.json`, or `textures/block/lever.png`.
pub trait Assets {
    /// A JSON file, parsed, or `None` if there is none.
    fn json(&self, path: &str) -> Option<Value>;
    /// A PNG, decoded, or `None` if there is none.
    fn texture(&self, path: &str) -> Option<Pixmap>;
}

/// A picture's size in pixels, and the margin left around the build.
#[derive(Debug, Clone, Copy)]
pub struct View {
    pub width: u32,
    pub height: u32,
    pub padding: f32,
}

/// Draws builds with the resource files of `A`, each file read once and
/// each block state worked out once, however many pictures are drawn.
pub struct Renderer<A> {
    assets: A,
    files: Mutex<HashMap<String, Option<Arc<Value>>>>,
    models: Mutex<HashMap<String, Option<Arc<Model>>>>,
    textures: Mutex<HashMap<String, Option<Arc<Texture>>>>,
    blocks: Mutex<HashMap<String, Arc<Block>>>,
}

impl<A: Assets> Renderer<A> {
    pub fn new(assets: A) -> Self {
        Renderer {
            assets,
            files: Mutex::default(),
            models: Mutex::default(),
            textures: Mutex::default(),
            blocks: Mutex::default(),
        }
    }

    /// A picture of `blocks`, each a position and a full block state such
    /// as `minecraft:repeater[delay=1,facing=north,locked=false,powered=false]`,
    /// scaled to fill `view`; `None` if `view` has no pixels.
    pub fn render<'a>(
        &self,
        blocks: impl IntoIterator<Item = (Pos, &'a str)>,
        view: View,
    ) -> Option<Pixmap> {
        draw::draw(&self.cells(blocks), &view)
    }

    /// Each of `blocks` with anything to draw, as drawn.
    fn cells<'a>(
        &self,
        blocks: impl IntoIterator<Item = (Pos, &'a str)>,
    ) -> HashMap<Pos, Arc<Block>> {
        blocks
            .into_iter()
            .map(|(pos, state)| (pos, self.block(state)))
            .filter(|(_, block)| !block.quads.is_empty() || block.cube)
            .collect()
    }

    /// Block `state` as drawn, worked out the first time it is asked for.
    fn block(&self, state: &str) -> Arc<Block> {
        if let Some(block) = lock(&self.blocks).get(state) {
            return Arc::clone(block);
        }
        let block = Arc::new(self.bake(state));
        lock(&self.blocks).insert(state.to_owned(), Arc::clone(&block));
        block
    }

    fn bake(&self, state: &str) -> Block {
        let (id, properties) = blockstate::parse(state);
        let name = id.strip_prefix("minecraft:").unwrap_or(id);
        let property = |key: &str| {
            properties.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
        };
        if fallback::invisible(name) {
            return Block {
                id: id.to_owned(),
                quads: Vec::new(),
                opaque: false,
                cube: false,
                ao: false,
            };
        }
        let tint = fallback::tint(name, property("power"));
        let modelled = || {
            let file = self.file(&format!("blockstates/{name}.json"))?;
            let parts = blockstate::select(&file, &properties)?
                .into_iter()
                .map(|placement| {
                    Some((placement.clone(), self.model(&placement.model)?))
                })
                .collect::<Option<Vec<_>>>()?;
            // A model of no elements, such as a chest's, which the game
            // draws otherwise, is drawn as a cube rather than nothing.
            if parts.iter().all(|(_, model)| model.elements.is_empty()) {
                return None;
            }
            geometry::build(id, &parts, |path| self.texture(path), tint)
        };
        modelled().unwrap_or_else(|| {
            let lit = property("lit") == Some("true");
            fallback_cube(id, fallback::color(name, lit))
        })
    }

    fn file(&self, path: &str) -> Option<Arc<Value>> {
        cached(&self.files, path, || self.assets.json(path).map(Arc::new))
    }

    fn model(&self, name: &str) -> Option<Arc<Model>> {
        cached(&self.models, name, || {
            model::load(name, |path| self.file(path)).map(Arc::new)
        })
    }

    fn texture(&self, path: &str) -> Option<Arc<Texture>> {
        cached(&self.textures, path, || {
            self.assets
                .texture(path)
                .and_then(Texture::new)
                .map(Arc::new)
        })
    }
}

/// A cube of one colour, for a block whose model or textures are missing.
fn fallback_cube(id: &str, color: [u8; 4]) -> Block {
    let texture = Arc::new(Texture::solid(color));
    let cube = Model {
        elements: vec![model::Element {
            from: [0.0; 3],
            to: [16.0; 3],
            rotation: None,
            shade: true,
            faces: geometry::Dir::ALL
                .iter()
                .map(|&dir| model::Face {
                    dir,
                    uv: None,
                    texture: Some(String::new()),
                    cullface: Some(dir),
                    rotation: 0,
                    tinted: false,
                })
                .collect(),
        }],
        ambient_occlusion: true,
    };
    let placement = blockstate::Placement {
        model: String::new(),
        x: 0,
        y: 0,
        uvlock: false,
    };
    geometry::build(
        id,
        &[(placement, Arc::new(cube))],
        |_| Some(Arc::clone(&texture)),
        [1.0; 3],
    )
    .expect("a cube of one colour has its texture")
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The value kept under `key` in `cache`, else made by `make` and kept,
/// the lock not held while making it, which may fetch.
fn cached<T: Clone>(
    cache: &Mutex<HashMap<String, T>>,
    key: &str,
    make: impl FnOnce() -> T,
) -> T {
    if let Some(value) = lock(cache).get(key) {
        return value.clone();
    }
    let value = make();
    lock(cache).insert(key.to_owned(), value.clone());
    value
}

// The values the tests compare are exact in binary.
#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Stone and glass cubes, glass see-through at its middle.
    struct Fixtures;

    impl Assets for Fixtures {
        fn json(&self, path: &str) -> Option<Value> {
            Some(match path {
                "blockstates/stone.json" => {
                    json!({ "variants": { "": { "model": "block/stone" } } })
                }
                "blockstates/glass.json" => {
                    json!({ "variants": { "": { "model": "block/glass" } } })
                }
                "models/block/stone.json" => json!({
                    "parent": "block/cube_all",
                    "textures": { "all": "block/stone" }
                }),
                "models/block/glass.json" => json!({
                    "parent": "block/cube_all",
                    "textures": { "all": "block/glass" }
                }),
                "models/block/cube_all.json" => {
                    let face = |dir: &str| json!({ "texture": "#all", "cullface": dir });
                    json!({ "elements": [{
                        "from": [0, 0, 0], "to": [16, 16, 16],
                        "faces": {
                            "down": face("down"), "up": face("up"),
                            "north": face("north"), "south": face("south"),
                            "west": face("west"), "east": face("east"),
                        }
                    }]})
                }
                _ => return None,
            })
        }

        fn texture(&self, path: &str) -> Option<Pixmap> {
            let mut pixmap = Pixmap::new(16, 16)?;
            pixmap.fill(tiny_skia::Color::WHITE);
            if path == "textures/block/glass.png" {
                pixmap.pixels_mut()[8 * 16 + 8] =
                    tiny_skia::PremultipliedColorU8::TRANSPARENT;
            }
            Some(pixmap)
        }
    }

    /// How many faces of `blocks` are painted.
    fn faces(blocks: &[(Pos, &str)]) -> usize {
        let renderer = Renderer::new(Fixtures);
        draw::visible(&renderer.cells(blocks.iter().copied())).len()
    }

    #[test]
    fn a_face_against_an_opaque_block_is_culled() {
        let stone = "minecraft:stone";
        assert_eq!(faces(&[([0, 0, 0], stone)]), 3);
        // The west block's east face is hidden; its top and south show.
        assert_eq!(faces(&[([0, 0, 0], stone), ([1, 0, 0], stone)]), 5);
        // Glass hides nothing behind it, but stone hides glass.
        let glass = "minecraft:glass";
        assert_eq!(faces(&[([0, 0, 0], stone), ([1, 0, 0], glass)]), 6);
        assert_eq!(faces(&[([0, 0, 0], glass), ([1, 0, 0], stone)]), 5);
        // Glass beside glass shows no face between.
        assert_eq!(faces(&[([0, 0, 0], glass), ([1, 0, 0], glass)]), 5);
    }

    #[test]
    fn a_block_without_a_model_is_a_cube_of_one_colour() {
        let renderer = Renderer::new(Fixtures);
        let cells = renderer.cells([([0, 0, 0], "minecraft:unknown")]);
        let block = &cells[&[0, 0, 0]];
        assert!(block.opaque);
        assert_eq!(block.quads.len(), 3);
        assert_eq!(faces(&[([0, 0, 0], "minecraft:air")]), 0);
    }

    #[test]
    fn a_face_darkens_toward_a_solid_block_beside_its_corners() {
        let renderer = Renderer::new(Fixtures);
        let cells = renderer.cells([
            ([0, 0, 0], "minecraft:stone"),
            ([1, 1, 0], "minecraft:stone"),
        ]);
        let faces = draw::visible(&cells);
        let top = faces
            .iter()
            .find(|f| f.corners.iter().all(|c| c[1] == 1.0))
            .unwrap();
        // The top's corners, north-west, north-east, south-east, and
        // south-west: the east two beside the block above to the east.
        assert_eq!(top.light, [1.0, 0.85, 0.85, 1.0]);
    }
}
