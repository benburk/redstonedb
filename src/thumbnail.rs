//! A picture of a build for its card: an isometric view from the south-east
//! and above. Each face takes its block's texture from the game, as the 3D
//! viewer does, when `Textures` holds it, and a flat colour otherwise. Full
//! blocks are shaded cubes; dust lies flat on its block, tinted by its
//! power; torches, repeaters, comparators, and levers are small boxes.

use std::collections::{BTreeSet, HashMap};

use anyhow::{Context, Result};
use tiny_skia::{
    FillRule, FilterQuality, Paint, PathBuilder, Pattern, Pixmap, SpreadMode,
    Stroke, Transform,
};

use crate::schem::{self, Blocks, Direction, Pos};

/// Names the way thumbnails are drawn, in their addresses and their kept
/// files, so drawing them anew changes both.
pub const RENDERER: &str = "t3";

/// Where drawn thumbnails are kept, beside the database, so a restart
/// draws none again.
pub const DRAWN_DIR: &str = "thumbnails";

pub const WIDTH: u16 = 640;
pub const HEIGHT: u16 = 480;
const PADDING: f32 = 24.0;

/// Brightness of the faces a viewer in the south-east above sees.
const TOP: f32 = 1.0;
const SOUTH: f32 = 0.8;
const EAST: f32 = 0.64;

type Rgba = [u8; 4];

/// A box within one block, in block units from its lower north-west corner.
struct Part {
    min: [f32; 3],
    max: [f32; 3],
    color: Rgba,
    /// Its faces to draw, top, south, and east: all of a box, one of a
    /// marking on a block's face.
    faces: [bool; 3],
    /// The texture of each face, top, south, and east, when it has one.
    tex: [Option<Tex>; 3],
    /// How far it leans for each block it rises, west to east and north
    /// to south, as a wall torch or a lever's handle does.
    lean: [f32; 2],
}

/// A face's texture: the game's texture `name`, the part of it in texels,
/// left, top, right, bottom, and how many quarter turns it is turned.
#[derive(Clone)]
struct Tex {
    name: String,
    uv: [f32; 4],
    turns: u8,
}

impl Tex {
    fn whole(name: &str) -> Self {
        Tex {
            name: name.to_owned(),
            uv: [0.0, 0.0, 16.0, 16.0],
            turns: 0,
        }
    }
}

/// The game's block textures a picture is drawn with, by name, each with
/// copies halved in size down to a pixel, so a small face shows its
/// texture's average rather than a few of its pixels.
#[derive(Default)]
pub struct Textures(
    HashMap<String, Vec<Pixmap>>,
    std::collections::HashSet<String>,
);

impl Textures {
    /// The textures of `names` it lacks and hasn't found missing before.
    pub fn lacking<'a>(&self, names: &'a BTreeSet<String>) -> Vec<&'a String> {
        names
            .iter()
            .filter(|name| {
                !self.0.contains_key(*name) && !self.1.contains(*name)
            })
            .collect()
    }

    /// Notes that the game has no texture `name`, so it isn't asked for
    /// again; its faces take a flat colour.
    pub fn missing(&mut self, name: &str) {
        self.1.insert(name.to_owned());
    }

    /// Takes a texture's PNG, `name` as the game names it.
    pub fn add(&mut self, name: &str, png: &[u8]) -> Result<()> {
        let mut pixmap = Pixmap::decode_png(png)?;
        // An animated texture's frames stand one above the next; the first
        // is enough.
        if pixmap.height() > pixmap.width() {
            let side = pixmap.width();
            let mut first = Pixmap::new(side, side).context("empty texture")?;
            first.draw_pixmap(
                0,
                0,
                pixmap.as_ref(),
                &tiny_skia::PixmapPaint::default(),
                Transform::identity(),
                None,
            );
            pixmap = first;
        }
        let mut levels = vec![pixmap];
        while let Some(last) = levels.last().filter(|p| p.width() > 1) {
            levels.push(halved(last));
        }
        self.0.insert(name.to_owned(), levels);
        Ok(())
    }

    /// Whether it holds texture `name`.
    pub fn has(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }
}

/// `pixmap` at half its size, each pixel the average of four.
fn halved(pixmap: &Pixmap) -> Pixmap {
    let (width, height) =
        ((pixmap.width() / 2).max(1), (pixmap.height() / 2).max(1));
    let mut out =
        Pixmap::new(width, height).expect("a halved texture has pixels");
    let source = pixmap.pixels();
    let stride = pixmap.width() as usize;
    for y in 0..height as usize {
        for x in 0..width as usize {
            let mut sum = [0u32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let p = source[(2 * y + dy).min(pixmap.height() as usize - 1)
                    * stride
                    + (2 * x + dx).min(stride - 1)];
                for (total, channel) in sum.iter_mut().zip([
                    p.red(),
                    p.green(),
                    p.blue(),
                    p.alpha(),
                ]) {
                    *total += u32::from(channel);
                }
            }
            let [r, g, b, a] =
                sum.map(|total| u8::try_from(total / 4).unwrap_or(u8::MAX));
            if let Some(pixel) = tiny_skia::PremultipliedColorU8::from_rgba(
                r.min(a),
                g.min(a),
                b.min(a),
                a,
            ) {
                out.pixels_mut()[y * width as usize + x] = pixel;
            }
        }
    }
    out
}

/// One block to draw, with the faces another opaque block does not cover.
struct Drawn {
    pos: Pos,
    parts: Vec<Part>,
    /// Faces to draw: top, south, east.
    faces: [bool; 3],
    /// How lit each corner of each face is, top, south, and east, each
    /// top left, top right, bottom right, and bottom left as its texture
    /// lies: shaded where blocks meet, as the game shades them.
    corners: [[f32; 4]; 3],
}

/// Every block with something to draw, far blocks first so near ones
/// paint over them.
fn visible(blocks: &Blocks) -> Vec<Drawn> {
    let shape = |pos: Pos| shape(blocks.get(pos));
    let opaque = |pos: Pos| match shape(pos) {
        Shape::Cube(color) => color[3] == 255,
        Shape::Target => true,
        Shape::Empty | Shape::Parts(_) => false,
    };
    let mut drawn: Vec<Drawn> = blocks
        .iter()
        .filter_map(|(pos, _)| {
            let (parts, cube) = match shape(pos) {
                Shape::Empty => return None,
                Shape::Cube(color) => (
                    vec![Part {
                        min: [0.0; 3],
                        max: [1.0; 3],
                        color,
                        faces: [true; 3],
                        tex: [None, None, None],
                        lean: [0.0; 2],
                    }],
                    true,
                ),
                Shape::Target => (target(), true),
                Shape::Parts(parts) => (parts, false),
            };
            let faces = if cube {
                [Direction::Up, Direction::South, Direction::East]
                    .map(|side| !opaque(schem::step(pos, side)))
            } else {
                [true; 3]
            };
            let mut parts = parts;
            dress(blocks.get(pos), &mut parts);
            let corners = if cube {
                occlusion(pos, &opaque)
            } else {
                [[1.0; 4]; 3]
            };
            faces.contains(&true).then_some(Drawn {
                pos,
                parts,
                faces,
                corners,
            })
        })
        .collect();
    drawn.sort_by_key(|d| d.pos.iter().sum::<i32>());
    drawn
}

/// How lit the corners of a cube's top, south, and east faces are, by the
/// three blocks beside each corner on the face's outside: two sides and
/// the one between them, as the game's smooth lighting goes, darkest where
/// both sides are filled.
fn occlusion(pos: Pos, opaque: &impl Fn(Pos) -> bool) -> [[f32; 4]; 3] {
    const LIGHT: [f32; 4] = [0.55, 0.7, 0.85, 1.0];
    let at = |offset: [i32; 3]| opaque(schem::add(pos, offset));
    let level = |side: [i32; 3], other: [i32; 3], corner: [i32; 3]| {
        let (a, b, c) = (at(side), at(other), at(corner));
        let filled = if a && b {
            3
        } else {
            usize::from(a) + usize::from(b) + usize::from(c)
        };
        LIGHT[3 - filled]
    };
    // Each face's corners as its texture lies: steps along its two axes.
    let top = [(-1, -1), (1, -1), (1, 1), (-1, 1)]
        .map(|(x, z)| level([x, 1, 0], [0, 1, z], [x, 1, z]));
    let south = [(-1, 1), (1, 1), (1, -1), (-1, -1)]
        .map(|(x, y)| level([x, 0, 1], [0, y, 1], [x, y, 1]));
    let east = [(1, 1), (-1, 1), (-1, -1), (1, -1)]
        .map(|(z, y)| level([1, 0, z], [1, y, 0], [1, y, z]));
    [top, south, east]
}

/// Projects a point to pixels, scaled and centred so `drawn` fills the
/// canvas inside its padding.
fn fit(
    drawn: &[Drawn],
    canvas: [f32; 2],
    padding: f32,
) -> impl Fn([f32; 3]) -> [f32; 2] + use<> {
    let (mut left, mut top, mut right, mut bottom) =
        (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for d in drawn {
        for part in &d.parts {
            for corner in corners(d.pos, part) {
                let [x, y] = project(corner);
                left = left.min(x);
                right = right.max(x);
                top = top.min(y);
                bottom = bottom.max(y);
            }
        }
    }
    let (width, height) = (right - left, bottom - top);
    let [canvas_width, canvas_height] = canvas;
    let scale = ((canvas_width - 2.0 * padding) / width)
        .min((canvas_height - 2.0 * padding) / height);
    let offset = [
        (canvas_width - width * scale) / 2.0 - left * scale,
        (canvas_height - height * scale) / 2.0 - top * scale,
    ];
    move |point| {
        let [x, y] = project(point);
        [x * scale + offset[0], y * scale + offset[1]]
    }
}

/// The eight corners of `part` placed at `pos`.
fn corners(pos: Pos, part: &Part) -> Vec<[f32; 3]> {
    let base = pos.map(coordinate);
    let mut corners = Vec::with_capacity(8);
    for x in [part.min[0], part.max[0]] {
        for y in [part.min[1], part.max[1]] {
            for z in [part.min[2], part.max[2]] {
                let rise = y - part.min[1];
                corners.push([
                    base[0] + x + part.lean[0] * rise,
                    base[1] + y,
                    base[2] + z + part.lean[1] * rise,
                ]);
            }
        }
    }
    corners
}

// A schematic's coordinates fit in 16 bits, far inside f32's exact range.
#[allow(clippy::cast_precision_loss)]
fn coordinate(n: i32) -> f32 {
    n as f32
}

/// Isometric projection: x runs down to the right, z down to the left, and
/// y straight up.
fn project([x, y, z]: [f32; 3]) -> [f32; 2] {
    const COS_30: f32 = 0.866_025_4;
    [(x - z) * COS_30, (x + z) * 0.5 - y]
}

fn face(pixmap: &mut Pixmap, corners: [[f32; 2]; 4], color: Rgba) {
    let mut path = PathBuilder::new();
    path.move_to(corners[0][0], corners[0][1]);
    for [x, y] in &corners[1..] {
        path.line_to(*x, *y);
    }
    path.close();
    let Some(path) = path.finish() else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
    paint.anti_alias = true;
    pixmap.fill_path(
        &path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );
    // Anti-aliased neighbours leave a faint seam; a hairline in the same
    // colour closes it. Translucent faces skip it, since it would darken
    // their edges.
    if color[3] == 255 {
        let stroke = Stroke {
            width: 0.75,
            ..Stroke::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

// The channel is clamped to 0..=255 before the cast.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn shade([r, g, b, a]: Rgba, light: f32) -> Rgba {
    let scale = |c: u8| (f32::from(c) * light).round().clamp(0.0, 255.0) as u8;
    [scale(r), scale(g), scale(b), a]
}

enum Shape {
    Empty,
    Cube(Rgba),
    /// A target block: a cube with rings on its faces.
    Target,
    Parts(Vec<Part>),
}

const TARGET_PALE: Rgba = [226, 216, 200, 255];
const TARGET_RED: Rgba = [196, 44, 40, 255];

/// A target block's cube, and on each face it shows, red and pale rings
/// stepping in to a red centre, each a hair above the one before so it
/// paints over it.
fn target() -> Vec<Part> {
    let mut parts = vec![Part {
        min: [0.0; 3],
        max: [1.0; 3],
        color: TARGET_PALE,
        faces: [true; 3],
        tex: [None, None, None],
        lean: [0.0; 2],
    }];
    let rings = [(0.1, TARGET_RED), (0.22, TARGET_PALE), (0.34, TARGET_RED)];
    for face in 0..3 {
        for (step, &(inset, color)) in rings.iter().enumerate() {
            let lift =
                1.0 + 0.002 * f32::from(u8::try_from(step + 1).unwrap_or(1));
            let (lo, hi) = (inset, 1.0 - inset);
            let (min, max) = match face {
                0 => ([lo, 1.0, lo], [hi, lift, hi]),
                1 => ([lo, lo, 1.0], [hi, hi, lift]),
                _ => ([1.0, lo, lo], [lift, hi, hi]),
            };
            let mut faces = [false; 3];
            faces[face] = true;
            parts.push(Part {
                min,
                max,
                color,
                faces,
                tex: [None, None, None],
                lean: [0.0; 2],
            });
        }
    }
    parts
}

fn shape(state: &str) -> Shape {
    let id = schem::id(state);
    let name = id.strip_prefix("minecraft:").unwrap_or(id);
    let property = |key| schem::property(state, key);
    let lit = property("lit") == Some("true")
        || property("powered") == Some("true")
        || property("power").is_some_and(|p| p != "0");
    let signal = if lit {
        [255, 48, 24, 255]
    } else {
        [110, 16, 10, 255]
    };
    let part = |min: [f32; 3], max: [f32; 3], color: Rgba| Part {
        min,
        max,
        color,
        faces: [true; 3],
        tex: [None, None, None],
        lean: [0.0; 2],
    };
    match name {
        "air" | "cave_air" | "void_air" => Shape::Empty,
        _ if name.contains("sign") => Shape::Empty,
        "redstone_wire" => {
            let (lo, hi, h) = (0.375, 0.625, 0.0625);
            let mut parts = vec![part([lo, 0.0, lo], [hi, h, hi], signal)];
            for (side, min, max) in [
                ("north", [lo, 0.0, 0.0], [hi, h, lo]),
                ("south", [lo, 0.0, hi], [hi, h, 1.0]),
                ("west", [0.0, 0.0, lo], [lo, h, hi]),
                ("east", [hi, 0.0, lo], [1.0, h, hi]),
            ] {
                if property(side).is_some_and(|v| v != "none") {
                    parts.push(part(min, max, signal));
                }
            }
            Shape::Parts(parts)
        }
        "redstone_torch" | "redstone_wall_torch" => torch(
            (name == "redstone_wall_torch")
                .then(|| direction(property("facing").unwrap_or(""))),
            signal,
        ),
        "lever" => lever(
            property("face").unwrap_or("wall"),
            direction(property("facing").unwrap_or("")),
            property("powered") == Some("true"),
        ),
        _ if name.ends_with("_button") => button(
            property("face").unwrap_or("wall"),
            direction(property("facing").unwrap_or("")),
            property("powered") == Some("true"),
        ),
        "redstone_lamp" => Shape::Cube(if lit {
            [240, 200, 130, 255]
        } else {
            [120, 76, 46, 255]
        }),
        "glass" => Shape::Cube([200, 225, 235, 80]),
        "target" => Shape::Target,
        _ if name.ends_with("_slab") && property("type") != Some("double") => {
            slab(name, property("type") == Some("top"))
        }
        _ => Shape::Cube(cube_color(name)),
    }
}

/// A box from `min` to `max`, leaning `lean` for each block it rises.
fn leaning(min: [f32; 3], max: [f32; 3], color: Rgba, lean: [f32; 2]) -> Part {
    Part {
        min,
        max,
        color,
        faces: [true; 3],
        tex: [None, None, None],
        lean,
    }
}

/// A torch: a stick two pixels square and ten tall with its flame on top,
/// standing, or with `out`, the way it points from the wall it hangs on,
/// leaning out from that wall at the game's angle, its foot near it.
fn torch(out: Option<[f32; 3]>, flame: Rgba) -> Shape {
    let [dx, _, dz] = out.unwrap_or([0.0; 3]);
    let (foot, y, lean) = match out {
        Some(_) => (-0.31, 0.19, [dx * 0.4, dz * 0.4]),
        None => (0.0, 0.0, [0.0; 2]),
    };
    let (lo, hi, height) = (0.4375, 0.5625, 0.625);
    let at = |lift: f32| {
        [
            0.5 + dx * foot + lean[0] * lift - 0.5,
            0.5 + dz * foot + lean[1] * lift - 0.5,
        ]
    };
    let [sx, sz] = at(0.0);
    let [hx, hz] = at(height - 0.125);
    Shape::Parts(vec![
        leaning(
            [lo + sx, y, lo + sz],
            [hi + sx, y + height - 0.125, hi + sz],
            [120, 90, 50, 255],
            lean,
        ),
        leaning(
            [lo + hx, y + height - 0.125, lo + hz],
            [hi + hx, y + height, hi + hz],
            flame,
            lean,
        ),
    ])
}

/// A lever on its block's `face`, `wall`, `floor`, or `ceiling`, `facing`
/// the way the game says: a cobblestone plate six pixels by eight, three
/// deep, and a handle tilted 45 degrees, up when off and down when on.
fn lever(face: &str, facing: [f32; 3], on: bool) -> Shape {
    let [fx, _, fz] = facing;
    let stone = [110, 110, 110, 255];
    let handle = [125, 100, 60, 255];
    let thick = 0.1875;
    let reach = 0.44;
    if face == "wall" {
        // The plate lies on the wall behind it; the handle leans out.
        let wall = [
            0.5 - fx * (0.5 - thick / 2.0),
            0.5 - fz * (0.5 - thick / 2.0),
        ];
        let half = |along: f32, across: f32| {
            if fx == 0.0 {
                [across, along]
            } else {
                [along, across]
            }
        };
        let [hx, hz] = half(thick / 2.0, 0.1875);
        let plate = leaning(
            [wall[0] - hx, 0.25, wall[1] - hz],
            [wall[0] + hx, 0.75, wall[1] + hz],
            stone,
            [0.0; 2],
        );
        let out = [fx, fz];
        let (bottom, lean, start) = if on {
            (0.5 - reach, [-out[0], -out[1]], reach)
        } else {
            (0.5, out, 0.0)
        };
        let cx = wall[0] + out[0] * (thick / 2.0 + start);
        let cz = wall[1] + out[1] * (thick / 2.0 + start);
        Shape::Parts(vec![
            plate,
            leaning(
                [cx - 0.0625, bottom, cz - 0.0625],
                [cx + 0.0625, bottom + reach, cz + 0.0625],
                handle,
                lean,
            ),
        ])
    } else {
        let ceiling = face == "ceiling";
        let (plate_low, handle_low) = if ceiling {
            (1.0 - thick, 1.0 - thick - reach)
        } else {
            (0.0, thick)
        };
        let [hx, hz] = if fx == 0.0 {
            [0.1875, 0.25]
        } else {
            [0.25, 0.1875]
        };
        let plate = leaning(
            [0.5 - hx, plate_low, 0.5 - hz],
            [0.5 + hx, plate_low + thick, 0.5 + hz],
            stone,
            [0.0; 2],
        );
        // The handle tips toward `facing` when off, away when on, and
        // hangs below a ceiling's plate.
        let tip = if on == ceiling { 1.0 } else { -1.0 };
        let lean = [fx * tip, fz * tip];
        let start = if ceiling {
            [-lean[0] * reach, -lean[1] * reach]
        } else {
            [0.0, 0.0]
        };
        let (cx, cz) = (0.5 + start[0], 0.5 + start[1]);
        Shape::Parts(vec![
            plate,
            leaning(
                [cx - 0.0625, handle_low, cz - 0.0625],
                [cx + 0.0625, handle_low + reach, cz + 0.0625],
                handle,
                lean,
            ),
        ])
    }
}

/// A button on its block's `face`, `facing` the way the game says: six
/// pixels by four, two deep, or one when pressed.
fn button(face: &str, facing: [f32; 3], pressed: bool) -> Shape {
    let [fx, _, fz] = facing;
    let depth = if pressed { 0.0625 } else { 0.125 };
    let stone = [125, 125, 125, 255];
    let (min, max) = if face == "wall" {
        let near = [0.5 - fx * 0.5, 0.5 - fz * 0.5];
        let far = [near[0] + fx * depth, near[1] + fz * depth];
        let (wide, tall) = (0.1875, 0.125);
        let [ax, az] = if fx == 0.0 { [wide, 0.0] } else { [0.0, wide] };
        (
            [
                near[0].min(far[0]) - ax,
                0.5 - tall,
                near[1].min(far[1]) - az,
            ],
            [
                near[0].max(far[0]) + ax,
                0.5 + tall,
                near[1].max(far[1]) + az,
            ],
        )
    } else {
        let low = if face == "ceiling" { 1.0 - depth } else { 0.0 };
        let [hx, hz] = if fx == 0.0 {
            [0.1875, 0.125]
        } else {
            [0.125, 0.1875]
        };
        ([0.5 - hx, low, 0.5 - hz], [0.5 + hx, low + depth, 0.5 + hz])
    };
    Shape::Parts(vec![leaning(min, max, stone, [0.0; 2])])
}

/// A slab, filling the upper half of its block if `top`, else the lower.
fn slab(name: &str, top: bool) -> Shape {
    let (low, high) = if top { (0.5, 1.0) } else { (0.0, 0.5) };
    Shape::Parts(vec![Part {
        min: [0.0, low, 0.0],
        max: [1.0, high, 1.0],
        color: cube_color(name),
        faces: [true; 3],
        tex: [None, None, None],
        lean: [0.0; 2],
    }])
}

/// A full block's colour, by its name without `minecraft:`: stained glass
/// see-through in its dye's colour, concrete in its own, and a few others.
fn cube_color(name: &str) -> Rgba {
    // A slab is drawn in its block's colour.
    let name = match name {
        "quartz_slab" | "smooth_quartz_slab" => "quartz_block",
        "smooth_stone_slab" => "smooth_stone",
        "stone_slab" => "stone",
        "stone_brick_slab" => "stone_bricks",
        "oak_slab" => "oak_planks",
        "spruce_slab" => "spruce_planks",
        "bamboo_mosaic_slab" => "bamboo_mosaic",
        name => name,
    };
    if let Some(dye) = name.strip_suffix("_stained_glass") {
        let [r, g, b, _] = dye_color(dye);
        return [r, g, b, 110];
    }
    if let Some(dye) = name.strip_suffix("_concrete") {
        return dye_color(dye);
    }
    // Wool is a little lighter than concrete, terracotta duller.
    if let Some(dye) = name.strip_suffix("_wool") {
        return shade(dye_color(dye), 1.15);
    }
    if let Some(dye) = name.strip_suffix("_terracotta") {
        let [r, g, b, a] = dye_color(dye);
        let mix = |c: u8| u8::midpoint(c, 150);
        return shade(
            [
                mix(r),
                mix(g).saturating_sub(15),
                mix(b).saturating_sub(30),
                a,
            ],
            0.85,
        );
    }
    match name {
        "terracotta" => [152, 94, 67, 255],
        "oak_planks" => [162, 130, 78, 255],
        "spruce_planks" => [114, 84, 48, 255],
        "birch_planks" => [192, 175, 121, 255],
        "jungle_planks" => [160, 115, 80, 255],
        "acacia_planks" => [168, 90, 50, 255],
        "dark_oak_planks" => [66, 43, 20, 255],
        "mangrove_planks" => [117, 54, 48, 255],
        "cherry_planks" => [226, 178, 172, 255],
        "bamboo_planks" => [193, 173, 80, 255],
        "crimson_planks" => [101, 48, 70, 255],
        "warped_planks" => [43, 104, 99, 255],
        "cobblestone" => [118, 118, 118, 255],
        "polished_granite" => [154, 106, 89, 255],
        "deepslate_tiles" => [54, 54, 55, 255],
        "polished_blackstone" => [53, 48, 56, 255],
        "smooth_sandstone" => [223, 214, 170, 255],
        "bamboo_mosaic" => [190, 170, 78, 255],
        "glass" => [200, 222, 232, 90],
        // The solid blocks a palette may build with.
        "stone" => [125, 125, 125, 255],
        "smooth_stone" => [158, 158, 158, 255],
        "stone_bricks" => [122, 121, 122, 255],
        "smooth_quartz" | "quartz_block" => [235, 229, 222, 255],
        "white_wool" => [234, 236, 237, 255],
        "white_terracotta" => [210, 178, 161, 255],
        "polished_andesite" => [132, 134, 133, 255],
        "polished_diorite" => [192, 193, 194, 255],
        "redstone_block" => [175, 25, 5, 255],
        "observer" => [95, 95, 95, 255],
        "barrel" => [134, 98, 58, 255],
        "hopper" => [62, 62, 66, 255],
        "piston" | "sticky_piston" | "piston_head" => [160, 140, 100, 255],
        _ => [150, 150, 150, 255],
    }
}

/// Minecraft's concrete colours, by dye, such as `light_blue`.
pub fn dye_color(dye: &str) -> Rgba {
    match dye {
        "white" => [207, 213, 214, 255],
        "orange" => [224, 97, 1, 255],
        "magenta" => [169, 48, 159, 255],
        "light_blue" => [36, 137, 199, 255],
        "yellow" => [241, 175, 21, 255],
        "lime" => [94, 169, 24, 255],
        "pink" => [214, 101, 143, 255],
        "gray" => [55, 58, 62, 255],
        "light_gray" => [125, 125, 115, 255],
        "cyan" => [21, 119, 136, 255],
        "purple" => [100, 32, 156, 255],
        "blue" => [45, 47, 143, 255],
        "brown" => [96, 60, 32, 255],
        "green" => [73, 91, 36, 255],
        "red" => [142, 33, 33, 255],
        "black" => [8, 10, 15, 255],
        _ => [150, 150, 150, 255],
    }
}

/// One block toward the level direction `facing` names, or nowhere for
/// any other.
fn direction(facing: &str) -> [f32; 3] {
    match Direction::parse(facing) {
        Some(Direction::North) => [0.0, 0.0, -1.0],
        Some(Direction::South) => [0.0, 0.0, 1.0],
        Some(Direction::West) => [-1.0, 0.0, 0.0],
        Some(Direction::East) => [1.0, 0.0, 0.0],
        _ => [0.0; 3],
    }
}

/// Gives the parts of the block `state` their textures: a full block its
/// top and side, dust one flat face of its lines tinted by its power, and
/// torches, repeaters, comparators, levers, and lamps their own.
fn dress(state: &str, parts: &mut Vec<Part>) {
    let id = schem::id(state);
    let name = id.strip_prefix("minecraft:").unwrap_or(id);
    let property = |key| schem::property(state, key);
    let lit = property("lit") == Some("true")
        || property("powered") == Some("true")
        || property("power").is_some_and(|p| p != "0");
    let all = |tex: Option<Tex>| [tex.clone(), tex.clone(), tex];
    let region = |name: &str, uv: [f32; 4]| {
        Some(Tex {
            name: name.to_owned(),
            uv,
            turns: 0,
        })
    };
    let torch = if lit {
        "redstone_torch"
    } else {
        "redstone_torch_off"
    };
    match name {
        "redstone_wire" => *parts = vec![dust_part(state, parts)],
        "redstone_torch" | "redstone_wall_torch" => {
            if let [stick, head] = parts.as_mut_slice() {
                stick.tex = all(region(torch, [7.0, 6.0, 9.0, 16.0]));
                head.tex = all(region(torch, [7.0, 5.0, 9.0, 7.0]));
            }
        }
        "repeater" | "comparator" => {
            let on = if lit { "_on" } else { "" };
            let turns = match property("facing") {
                Some("east") => 1,
                Some("south") => 2,
                Some("west") => 3,
                _ => 0,
            };
            if let [base, light] = parts.as_mut_slice() {
                let side = region("smooth_stone", [0.0, 14.0, 16.0, 16.0]);
                base.tex = [
                    Some(Tex {
                        name: format!("{name}{on}"),
                        uv: [0.0, 0.0, 16.0, 16.0],
                        turns,
                    }),
                    side.clone(),
                    side,
                ];
                light.tex = all(region(torch, [7.0, 5.0, 9.0, 7.0]));
            }
        }
        "lever" => {
            if let [plate, handle] = parts.as_mut_slice() {
                plate.tex = all(Some(Tex::whole("cobblestone")));
                handle.tex = all(region("lever", [7.0, 6.0, 9.0, 16.0]));
            }
        }
        _ if name.ends_with("_button") => {
            let block = match name.trim_end_matches("_button") {
                "stone" => "stone".to_owned(),
                "polished_blackstone" => "polished_blackstone".to_owned(),
                wood => format!("{wood}_planks"),
            };
            if let Some(button) = parts.first_mut() {
                button.tex = all(Some(Tex::whole(&block)));
            }
        }
        "target" => {
            parts.truncate(1);
            if let Some(cube) = parts.first_mut() {
                let side = Some(Tex::whole("target_side"));
                cube.tex = [Some(Tex::whole("target_top")), side.clone(), side];
            }
        }
        _ => {
            let (top, side) = match name {
                "redstone_lamp" if lit => {
                    ("redstone_lamp_on", "redstone_lamp_on")
                }
                "barrel" => ("barrel_top", "barrel_side"),
                _ => crate::palette::textures(name),
            };
            if let [cube] = parts.as_mut_slice() {
                let side = Some(Tex::whole(side));
                cube.tex = [Some(Tex::whole(top)), side.clone(), side];
            }
        }
    }
}

/// Dust's one flat face over its block, its texture named for the sides it
/// joins and its power, as `dust` makes it, its colour that of its `parts`.
fn dust_part(state: &str, parts: &[Part]) -> Part {
    let property = |key| schem::property(state, key);
    let joins: String = ["north", "south", "west", "east"]
        .iter()
        .map(|side| {
            if property(side).is_some_and(|v| v != "none") {
                '1'
            } else {
                '0'
            }
        })
        .collect();
    let power = property("power").unwrap_or("0");
    Part {
        min: [0.0; 3],
        max: [1.0, 0.0625, 1.0],
        color: parts.first().map_or([110, 16, 10, 255], |p| p.color),
        faces: [true, false, false],
        tex: [
            Some(Tex {
                name: format!("dust:{joins}:{power}"),
                uv: [0.0, 0.0, 16.0, 16.0],
                turns: 0,
            }),
            None,
            None,
        ],
        lean: [0.0; 2],
    }
}

/// `c`, a channel worked out in floating point, as a byte.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn byte(c: f32) -> u8 {
    c.round().clamp(0.0, 255.0) as u8
}

/// The game textures a picture of `blocks` is drawn with.
pub fn needed(blocks: &Blocks) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for d in visible(blocks) {
        for part in &d.parts {
            for tex in part.tex.iter().flatten() {
                if tex.name.starts_with("dust:") {
                    for base in [
                        "redstone_dust_dot",
                        "redstone_dust_line0",
                        "redstone_dust_line1",
                    ] {
                        names.insert(base.to_owned());
                    }
                } else {
                    names.insert(tex.name.clone());
                }
            }
        }
    }
    names
}

/// Dust's texture for `name`, `dust:` then which of north, south, west,
/// and east it joins, as `1` or `0`, then its power: the game's lines and
/// dot laid together, then tinted from dark to bright red by the power.
fn dust(textures: &Textures, name: &str) -> Option<Vec<Pixmap>> {
    let mut fields = name.split(':').skip(1);
    let joins: Vec<bool> = fields.next()?.chars().map(|c| c == '1').collect();
    let power: f32 = fields.next()?.parse().ok()?;
    let level =
        |name: &str| textures.0.get(name).and_then(|levels| levels.first());
    let (dot, ns, ew) = (
        level("redstone_dust_dot")?,
        level("redstone_dust_line0")?,
        level("redstone_dust_line1")?,
    );
    let (width, size) = (dot.width(), dot.width() as usize);
    let mut laid = Pixmap::new(width, width)?;
    let (north_south, west_east) = (joins[0] || joins[1], joins[2] || joins[3]);
    let straight = north_south != west_east;
    // Both of the game's line textures run north to south; it turns one for
    // dust running west to east.
    let middle = f32::from(u16::try_from(width).unwrap_or(16)) / 2.0;
    let layers = [
        (north_south || !west_east, ns, Transform::identity()),
        (
            west_east || !north_south,
            ew,
            Transform::from_rotate_at(90.0, middle, middle),
        ),
        (!straight, dot, Transform::identity()),
    ];
    for (shown, layer, turn) in layers {
        if shown {
            laid.draw_pixmap(
                0,
                0,
                layer.as_ref(),
                &tiny_skia::PixmapPaint::default(),
                turn,
                None,
            );
        }
    }
    // How strongly each texel shows, then a texel wider each way, so a
    // line a texel or two wide still reads at a thumbnail's size.
    let shows: Vec<u8> = laid
        .pixels()
        .iter()
        .map(|p| p.demultiply().red().min(p.alpha()))
        .collect();
    let at = |x: usize, y: usize| shows[y * size + x];
    let widened: Vec<u8> = (0..size * size)
        .map(|i| {
            let (x, y) = (i % size, i / size);
            let mut most = at(x, y);
            if x > 0 {
                most = most.max(at(x - 1, y));
            }
            if x + 1 < size {
                most = most.max(at(x + 1, y));
            }
            if y > 0 {
                most = most.max(at(x, y - 1));
            }
            if y + 1 < size {
                most = most.max(at(x, y + 1));
            }
            most
        })
        .collect();
    // The game's dust runs from a dark red unpowered to a bright one; the
    // dark end is lifted a little to read against white blocks.
    let strength = power / 15.0;
    let red = (0.45 + 0.55 * strength) * 255.0;
    let green = 0.2 * strength.powi(2) * 255.0;
    let mut pixmap = Pixmap::new(width, width)?;
    for (pixel, &alpha) in pixmap.pixels_mut().iter_mut().zip(&widened) {
        *pixel =
            tiny_skia::ColorU8::from_rgba(byte(red), byte(green), 0, alpha)
                .premultiply();
    }
    let mut levels = vec![pixmap];
    while let Some(last) = levels.last().filter(|p| p.width() > 1) {
        levels.push(halved(last));
    }
    Some(levels)
}

/// The build as a WebP, `WIDTH` by `HEIGHT` on a see-through background,
/// its faces in the game's textures where `textures` holds them and in
/// flat colours where it doesn't.
pub fn render_with(blocks: &Blocks, textures: &Textures) -> Result<Vec<u8>> {
    // Drawn at twice the size, then each four pixels averaged to one, for
    // smooth edges.
    let drawn = visible(blocks);
    let to_screen = fit(
        &drawn,
        [f32::from(WIDTH) * 2.0, f32::from(HEIGHT) * 2.0],
        PADDING * 2.0,
    );
    let mut pixmap = Pixmap::new(u32::from(WIDTH) * 2, u32::from(HEIGHT) * 2)
        .context("thumbnail size is zero")?;
    let mut made: HashMap<String, Option<Vec<Pixmap>>> = HashMap::new();
    for d in &drawn {
        let base = d.pos.map(coordinate);
        for part in &d.parts {
            let at = |dx: usize, dy: usize, dz: usize| {
                let pick = |axis: usize, high: usize| {
                    base[axis]
                        + if high == 1 {
                            part.max[axis]
                        } else {
                            part.min[axis]
                        }
                };
                // A leaning part's top stands off from its foot.
                let rise = if dy == 1 {
                    part.max[1] - part.min[1]
                } else {
                    0.0
                };
                to_screen([
                    pick(0, dx) + part.lean[0] * rise,
                    pick(1, dy),
                    pick(2, dz) + part.lean[1] * rise,
                ])
            };
            // Each face's corners as its texture lies on it: its top left,
            // top right, bottom right, and bottom left.
            let faces = [
                ([at(0, 1, 0), at(1, 1, 0), at(1, 1, 1), at(0, 1, 1)], TOP),
                ([at(0, 1, 1), at(1, 1, 1), at(1, 0, 1), at(0, 0, 1)], SOUTH),
                ([at(1, 1, 1), at(1, 1, 0), at(1, 0, 0), at(1, 0, 1)], EAST),
            ];
            for (i, (corners, light)) in faces.into_iter().enumerate() {
                if !(d.faces[i] && part.faces[i]) {
                    continue;
                }
                let levels = part.tex[i].as_ref().and_then(|tex| {
                    if tex.name.starts_with("dust:") {
                        made.entry(tex.name.clone())
                            .or_insert_with(|| dust(textures, &tex.name))
                            .as_ref()
                            .map(|levels| (tex, levels.as_slice()))
                    } else {
                        textures
                            .0
                            .get(&tex.name)
                            .map(|levels| (tex, levels.as_slice()))
                    }
                });
                let lit = d.corners[i];
                if let Some((tex, levels)) = levels {
                    textured(&mut pixmap, corners, tex, levels);
                    darken(&mut pixmap, corners, light, lit);
                } else {
                    face(&mut pixmap, corners, shade(part.color, light));
                    darken(&mut pixmap, corners, 1.0, lit);
                }
            }
        }
    }
    Ok(webp(&halved(&pixmap)))
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

/// Darkens the face with `corners`, top left, top right, bottom right, and
/// bottom left, to `light`, its corners further to `lit`, in a grid of
/// cells each as lit as its middle, between its corners.
fn darken(
    pixmap: &mut Pixmap,
    corners: [[f32; 2]; 4],
    light: f32,
    lit: [f32; 4],
) {
    const CELLS: u8 = 4;
    if light >= 1.0 && lit.iter().all(|&l| l >= 1.0) {
        return;
    }
    let [tl, tr, br, bl] = corners;
    let at = |across: f32, down: f32| {
        let mix = |top_left: f32,
                   top_right: f32,
                   bottom_right: f32,
                   bottom_left: f32| {
            top_left
                + (top_right - top_left) * across
                + (bottom_left - top_left) * down
                + (top_left - top_right + bottom_right - bottom_left)
                    * across
                    * down
        };
        [
            mix(tl[0], tr[0], br[0], bl[0]),
            mix(tl[1], tr[1], br[1], bl[1]),
        ]
    };
    let [lit_tl, lit_tr, lit_br, lit_bl] = lit;
    let step = 1.0 / f32::from(CELLS);
    for i in 0..CELLS {
        for j in 0..CELLS {
            let (across, down) = (f32::from(i) * step, f32::from(j) * step);
            let (mid_across, mid_down) =
                (across + step / 2.0, down + step / 2.0);
            let shade = lit_tl
                + (lit_tr - lit_tl) * mid_across
                + (lit_bl - lit_tl) * mid_down
                + (lit_tl - lit_tr + lit_br - lit_bl) * mid_across * mid_down;
            let darkness = 1.0 - light * shade;
            if darkness < 0.01 {
                continue;
            }
            let mut path = PathBuilder::new();
            let first = at(across, down);
            path.move_to(first[0], first[1]);
            for [x, y] in [
                at(across + step, down),
                at(across + step, down + step),
                at(across, down + step),
            ] {
                path.line_to(x, y);
            }
            path.close();
            let Some(path) = path.finish() else { continue };
            // Cells meet edge to edge, so none is smoothed, which would
            // leave seams; drawing at twice the size smooths the outline.
            let mut paint = Paint {
                anti_alias: false,
                ..Paint::default()
            };
            paint.set_color_rgba8(0, 0, 0, byte(darkness * 255.0));
            pixmap.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }
}

/// Fills the face with `corners`, its texture's top left, top right,
/// bottom right, and bottom left, with `tex`, at the size of `levels`
/// nearest its pixels on screen.
fn textured(
    pixmap: &mut Pixmap,
    corners: [[f32; 2]; 4],
    tex: &Tex,
    levels: &[Pixmap],
) {
    let mut corners = corners;
    corners.rotate_left(usize::from(tex.turns % 4));
    let [tl, tr, br, bl] = corners;
    let [u0, v0, u1, v1] = tex.uv;
    let along = |to: [f32; 2], span: f32| {
        [(to[0] - tl[0]) / span, (to[1] - tl[1]) / span]
    };
    let (u, v) = (along(tr, u1 - u0), along(bl, v1 - v0));
    // Screen pixels to a texel, the fewer of the two ways.
    let per_texel = u[0].hypot(u[1]).min(v[0].hypot(v[1]));
    let mut level = 0;
    while per_texel * f32::from(1u16 << level) < 1.0 && level + 1 < levels.len()
    {
        level += 1;
    }
    let f = f32::from(1u16 << level);
    let quality = if level == 0 && per_texel >= 1.0 {
        FilterQuality::Nearest
    } else {
        FilterQuality::Bilinear
    };
    let transform = Transform::from_row(
        u[0] * f,
        u[1] * f,
        v[0] * f,
        v[1] * f,
        tl[0] - u[0] * u0 - v[0] * v0,
        tl[1] - u[1] * u0 - v[1] * v0,
    );
    let mut path = PathBuilder::new();
    path.move_to(tl[0], tl[1]);
    for [x, y] in [tr, br, bl] {
        path.line_to(x, y);
    }
    path.close();
    let Some(path) = path.finish() else { return };
    let paint = Paint {
        anti_alias: true,
        shader: Pattern::new(
            levels[level].as_ref(),
            SpreadMode::Pad,
            quality,
            1.0,
            transform,
        ),
        ..Paint::default()
    };
    pixmap.fill_path(
        &path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );
}

/// Where the game's textures are kept once fetched, beside the database,
/// so each is fetched once.
const TEXTURE_DIR: &str = "textures";

/// Fetches into `textures` those of `names` it lacks: from the folder they
/// are kept in, else from the public copy the 3D viewer's browser fetches
/// from, kept for next time.
pub async fn fetch(
    http: &reqwest::Client,
    textures: &std::sync::RwLock<Textures>,
    names: &BTreeSet<String>,
) {
    let lacking: Vec<String> = {
        let held = textures.read().expect("the textures are whole");
        held.lacking(names).into_iter().cloned().collect()
    };
    for name in lacking {
        let kept =
            std::path::Path::new(TEXTURE_DIR).join(format!("{name}.png"));
        let png = if let Ok(png) = tokio::fs::read(&kept).await {
            Some(png)
        } else {
            let fetched = download(http, &name).await;
            if let Some(png) = &fetched {
                let _ = tokio::fs::create_dir_all(TEXTURE_DIR).await;
                let _ = tokio::fs::write(&kept, png).await;
            }
            fetched
        };
        let mut held = textures.write().expect("the textures are whole");
        match png.map(|png| held.add(&name, &png)) {
            Some(Ok(())) => {}
            _ => held.missing(&name),
        }
    }
}

/// Texture `name` from the public copy of the game's, if it has one.
async fn download(http: &reqwest::Client, name: &str) -> Option<Vec<u8>> {
    let url = format!("{}/{name}.png", crate::palette::TEXTURES);
    let response = http.get(&url).send().await.ok()?.error_for_status().ok()?;
    Some(response.bytes().await.ok()?.to_vec())
}
