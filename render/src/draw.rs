//! The picture: every face no opaque neighbour hides, seen isometrically
//! from the south-east and above, shaded where solid blocks meet it, and
//! painted far to near at twice the size, then averaged down.

use std::collections::HashMap;
use std::sync::Arc;

use tiny_skia::{
    FillRule, FilterQuality, Paint, PathBuilder, Pattern, Pixmap, SpreadMode,
    Transform,
};

use crate::geometry::{Block, Dir, Quad};
use crate::texture;
use crate::{Pos, View};

/// How many times larger than the picture it is drawn, before each square
/// of pixels is averaged to one, for smooth edges.
const SUPERSAMPLE: u32 = 2;

/// A face to paint: its corners in the world and how lit each is.
pub struct Painted<'a> {
    pub corners: [[f32; 3]; 4],
    pub quad: &'a Quad,
    pub light: [f32; 4],
}

/// The blocks `cells` in a picture `view`'s size, or `None` if it has no
/// pixels.
pub fn draw(cells: &HashMap<Pos, Arc<Block>>, view: &View) -> Option<Pixmap> {
    let faces = visible(cells);
    let (width, height) = (view.width * SUPERSAMPLE, view.height * SUPERSAMPLE);
    let mut pixmap = Pixmap::new(width, height)?;
    let to_screen = fit(
        &faces,
        [width as f32, height as f32],
        view.padding * SUPERSAMPLE as f32,
    );
    for face in &faces {
        paint(
            &mut pixmap,
            face.corners.map(&to_screen),
            face.quad,
            face.light,
        );
    }
    Some(texture::shrink(&pixmap, view.width, view.height))
}

/// The faces of `cells` to paint, far ones first: those facing the viewer
/// that no opaque neighbour hides and that don't lie between two blocks
/// of one kind of glass, each lit by the solid blocks around it.
pub fn visible(cells: &HashMap<Pos, Arc<Block>>) -> Vec<Painted<'_>> {
    let opaque = |pos: Pos| cells.get(&pos).is_some_and(|b| b.opaque);
    // Far blocks first: none can hide a block whose coordinates sum higher.
    let mut blocks: Vec<_> = cells.iter().collect();
    blocks
        .sort_unstable_by_key(|(p, _)| (p[0] + p[1] + p[2], p[1], p[2], p[0]));
    let mut faces = Vec::new();
    for (&pos, block) in blocks {
        for quad in &block.quads {
            if quad.cull.is_some_and(|d| opaque(step(pos, d))) {
                continue;
            }
            let twin = |d: Dir| {
                cells
                    .get(&step(pos, d))
                    .is_some_and(|n| n.cube && n.id == block.id)
            };
            if block.cube && quad.side.is_some_and(twin) {
                continue;
            }
            let light = match quad.side {
                Some(side) if block.ao => occlusion(pos, side, quad, &opaque),
                _ => [1.0; 4],
            };
            let base = pos.map(|n| n as f32);
            faces.push(Painted {
                corners: quad
                    .corners
                    .map(|c| [c[0] + base[0], c[1] + base[1], c[2] + base[2]]),
                quad,
                light,
            });
        }
    }
    faces
}

fn step(pos: Pos, dir: Dir) -> Pos {
    let d = dir.step();
    [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]]
}

/// How lit each corner of `quad`, lying on its block's `side`, is: at each
/// of the side's corners, by the three blocks around it beyond the side,
/// darkest where both of the two beside it are solid, as the game's smooth
/// lighting goes; and between those corners, blended by where it lies.
fn occlusion(
    pos: Pos,
    side: Dir,
    quad: &Quad,
    solid: &impl Fn(Pos) -> bool,
) -> [f32; 4] {
    const LIGHT: [f32; 4] = [0.55, 0.7, 0.85, 1.0];
    let normal = side.step();
    let axis = normal.iter().position(|&n| n != 0).unwrap_or(1);
    let (a, b) = match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    let beyond = step(pos, side);
    let at = |da: i32, db: i32| {
        let mut p = beyond;
        p[a] += da;
        p[b] += db;
        solid(p)
    };
    let corner = |sa: i32, sb: i32| {
        let (one, other, both) = (at(sa, 0), at(0, sb), at(sa, sb));
        let filled = if one && other {
            3
        } else {
            usize::from(one) + usize::from(other) + usize::from(both)
        };
        LIGHT[3 - filled]
    };
    let lights = [corner(-1, -1), corner(1, -1), corner(-1, 1), corner(1, 1)];
    quad.corners.map(|c| {
        let (s, t) = (c[a].clamp(0.0, 1.0), c[b].clamp(0.0, 1.0));
        (lights[0] * (1.0 - s) + lights[1] * s) * (1.0 - t)
            + (lights[2] * (1.0 - s) + lights[3] * s) * t
    })
}

/// Isometric projection: x runs down to the right, z down to the left, and
/// y straight up.
fn project([x, y, z]: [f32; 3]) -> [f32; 2] {
    const COS_30: f32 = 0.866_025_4;
    [(x - z) * COS_30, (x + z) * 0.5 - y]
}

/// Projects a point to pixels, scaled and centred so `faces` fill the
/// canvas inside its padding.
fn fit(
    faces: &[Painted],
    canvas: [f32; 2],
    padding: f32,
) -> impl Fn([f32; 3]) -> [f32; 2] + use<> {
    let (mut left, mut top, mut right, mut bottom) =
        (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for corner in faces.iter().flat_map(|f| f.corners) {
        let [x, y] = project(corner);
        left = left.min(x);
        right = right.max(x);
        top = top.min(y);
        bottom = bottom.max(y);
    }
    let (width, height) = ((right - left).max(1.0), (bottom - top).max(1.0));
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

/// Paints `quad` at `corners` on screen, lit `light` at each: its texels
/// sharp where each covers a pixel or more, else averaged down to the
/// pixels it covers.
fn paint(
    pixmap: &mut Pixmap,
    corners: [[f32; 2]; 4],
    quad: &Quad,
    light: [f32; 4],
) {
    let [tl, tr, _, bl] = corners;
    let across = [tr[0] - tl[0], tr[1] - tl[1]];
    let down = [bl[0] - tl[0], bl[1] - tl[1]];
    if (across[0] * down[1] - across[1] * down[0]).abs() < 0.01 {
        return;
    }
    let Some(image) = texture::cut(&quad.texture, quad.uv, quad.color, light)
    else {
        return;
    };
    let fits =
        |texels: u32, pixels: f32| (pixels.ceil().max(1.0) as u32).min(texels);
    let (width, height) = (
        fits(image.width(), across[0].hypot(across[1])),
        fits(image.height(), down[0].hypot(down[1])),
    );
    let (image, quality) = if (width, height) == (image.width(), image.height())
    {
        (image, FilterQuality::Nearest)
    } else {
        (
            texture::shrink(&image, width, height),
            FilterQuality::Bilinear,
        )
    };
    let (w, h) = (width as f32, height as f32);
    let transform = Transform::from_row(
        across[0] / w,
        across[1] / w,
        down[0] / h,
        down[1] / h,
        tl[0],
        tl[1],
    );
    let mut path = PathBuilder::new();
    path.move_to(tl[0], tl[1]);
    for [x, y] in &corners[1..] {
        path.line_to(*x, *y);
    }
    path.close();
    let Some(path) = path.finish() else { return };
    // Faces meet edge to edge, so none is smoothed, which would leave
    // seams; drawing at twice the size smooths the outline.
    let paint = Paint {
        shader: Pattern::new(
            image.as_ref(),
            SpreadMode::Pad,
            quality,
            1.0,
            transform,
        ),
        anti_alias: false,
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
