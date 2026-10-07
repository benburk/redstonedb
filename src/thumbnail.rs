//! A picture of a build for its card: an isometric view from the south-east
//! and above, with a flat colour for each block. Full blocks are shaded
//! cubes; dust, torches, repeaters, comparators, and levers are small boxes.
//! The corner markers are left out.

use std::collections::HashSet;

use anyhow::{Context, Result};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

use crate::schem::{self, Blocks, Pos};

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
}

/// One block to draw, with the faces another opaque block does not cover.
struct Drawn {
    pos: Pos,
    parts: Vec<Part>,
    /// Faces to draw: top, south, east.
    faces: [bool; 3],
}

/// The build as a PNG, `WIDTH` by `HEIGHT`, on a transparent background.
pub fn render(blocks: &Blocks) -> Result<Vec<u8>> {
    let drawn = visible(blocks);
    let to_screen = fit(&drawn);
    let mut pixmap = Pixmap::new(u32::from(WIDTH), u32::from(HEIGHT))
        .context("thumbnail size is zero")?;
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
                to_screen([pick(0, dx), pick(1, dy), pick(2, dz)])
            };
            let [top, south, east] = d.faces;
            if top {
                face(
                    &mut pixmap,
                    [at(0, 1, 0), at(1, 1, 0), at(1, 1, 1), at(0, 1, 1)],
                    shade(part.color, TOP),
                );
            }
            if south {
                face(
                    &mut pixmap,
                    [at(0, 0, 1), at(1, 0, 1), at(1, 1, 1), at(0, 1, 1)],
                    shade(part.color, SOUTH),
                );
            }
            if east {
                face(
                    &mut pixmap,
                    [at(1, 0, 0), at(1, 0, 1), at(1, 1, 1), at(1, 1, 0)],
                    shade(part.color, EAST),
                );
            }
        }
    }
    pixmap
        .encode_png()
        .context("could not encode the thumbnail")
}

/// Every block with something to draw, far blocks first so near ones
/// paint over them.
fn visible(blocks: &Blocks) -> Vec<Drawn> {
    let markers: HashSet<Pos> = blocks.marker_positions().into_iter().collect();
    let shape = |pos: Pos| {
        if markers.contains(&pos) {
            Shape::Empty
        } else {
            shape(blocks.get(pos))
        }
    };
    let opaque =
        |pos: Pos| matches!(shape(pos), Shape::Cube(color) if color[3] == 255);
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
                    }],
                    true,
                ),
                Shape::Parts(parts) => (parts, false),
            };
            let faces = if cube {
                ["up", "south", "east"]
                    .map(|side| !opaque(schem::step(pos, side)))
            } else {
                [true; 3]
            };
            faces.contains(&true).then_some(Drawn { pos, parts, faces })
        })
        .collect();
    drawn.sort_by_key(|d| d.pos.iter().sum::<i32>());
    drawn
}

/// Projects a point to pixels, scaled and centred so `drawn` fills the
/// canvas inside its padding.
fn fit(drawn: &[Drawn]) -> impl Fn([f32; 3]) -> [f32; 2] + use<> {
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
    let [canvas_width, canvas_height] = [WIDTH, HEIGHT].map(f32::from);
    let scale = ((canvas_width - 2.0 * PADDING) / width)
        .min((canvas_height - 2.0 * PADDING) / height);
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
                corners.push([base[0] + x, base[1] + y, base[2] + z]);
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
    Parts(Vec<Part>),
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
    let part =
        |min: [f32; 3], max: [f32; 3], color: Rgba| Part { min, max, color };
    // Toward the block a wall torch or lever hangs on.
    let wall = |reach: f32| {
        let [dx, _, dz] = direction(property("facing").unwrap_or(""));
        [-dx * reach, -dz * reach]
    };
    match name {
        "air" | "cave_air" | "void_air" => Shape::Empty,
        _ if name.contains("sign") || name.contains("button") => Shape::Empty,
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
        "redstone_torch" | "redstone_wall_torch" => {
            let ([dx, dz], y) = if name == "redstone_wall_torch" {
                (wall(0.3), 0.2)
            } else {
                ([0.0, 0.0], 0.0)
            };
            let (lo, hi) = (0.4375, 0.5625);
            Shape::Parts(vec![
                part(
                    [lo + dx, y, lo + dz],
                    [hi + dx, y + 0.5, hi + dz],
                    [120, 90, 50, 255],
                ),
                part(
                    [lo + dx - 0.03, y + 0.5, lo + dz - 0.03],
                    [hi + dx + 0.03, y + 0.65, hi + dz + 0.03],
                    signal,
                ),
            ])
        }
        "repeater" | "comparator" => Shape::Parts(vec![
            part([0.0; 3], [1.0, 0.125, 1.0], [165, 165, 165, 255]),
            part([0.4, 0.125, 0.4], [0.6, 0.3, 0.6], signal),
        ]),
        "lever" => {
            let [dx, dz] = if property("face") == Some("wall") {
                wall(0.35)
            } else {
                [0.0, 0.0]
            };
            let y = match property("face") {
                Some("wall") => 0.3,
                Some("ceiling") => 0.8,
                _ => 0.0,
            };
            Shape::Parts(vec![part(
                [0.35 + dx, y, 0.35 + dz],
                [0.65 + dx, y + 0.2, 0.65 + dz],
                [110, 110, 110, 255],
            )])
        }
        "redstone_lamp" => Shape::Cube(if lit {
            [240, 200, 130, 255]
        } else {
            [120, 76, 46, 255]
        }),
        "glass" => Shape::Cube([200, 225, 235, 80]),
        _ => {
            if let Some(dye) = name.strip_suffix("_stained_glass") {
                let [r, g, b, _] = dye_color(dye);
                Shape::Cube([r, g, b, 110])
            } else if let Some(dye) = name.strip_suffix("_concrete") {
                Shape::Cube(dye_color(dye))
            } else {
                Shape::Cube(match name {
                    "redstone_block" => [175, 25, 5, 255],
                    "observer" => [95, 95, 95, 255],
                    "piston" | "sticky_piston" | "piston_head" => {
                        [160, 140, 100, 255]
                    }
                    _ => [150, 150, 150, 255],
                })
            }
        }
    }
}

/// Minecraft's concrete colours.
fn dye_color(dye: &str) -> Rgba {
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

fn direction(facing: &str) -> [f32; 3] {
    match facing {
        "north" => [0.0, 0.0, -1.0],
        "south" => [0.0, 0.0, 1.0],
        "west" => [-1.0, 0.0, 0.0],
        "east" => [1.0, 0.0, 0.0],
        _ => [0.0; 3],
    }
}
