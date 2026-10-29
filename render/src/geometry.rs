//! A block state's faces in its block: each element's faces placed where
//! the model puts them, turned by the element's rotation and then the
//! blockstate's, and kept only if they face the viewer, in the order they
//! are painted.

use std::sync::Arc;

use crate::blockstate::Placement;
use crate::model::{Element, Face, Model, Rotation};
use crate::texture::Texture;

/// The six ways a face can face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

impl Dir {
    pub const ALL: [Dir; 6] = [
        Dir::Down,
        Dir::Up,
        Dir::North,
        Dir::South,
        Dir::West,
        Dir::East,
    ];

    pub fn parse(name: &str) -> Option<Dir> {
        Some(match name {
            "down" | "bottom" => Dir::Down,
            "up" => Dir::Up,
            "north" => Dir::North,
            "south" => Dir::South,
            "west" => Dir::West,
            "east" => Dir::East,
            _ => return None,
        })
    }

    /// One block's step this way.
    pub fn step(self) -> [i32; 3] {
        match self {
            Dir::Down => [0, -1, 0],
            Dir::Up => [0, 1, 0],
            Dir::North => [0, 0, -1],
            Dir::South => [0, 0, 1],
            Dir::West => [-1, 0, 0],
            Dir::East => [1, 0, 0],
        }
    }

    /// The way nearest `v`.
    fn nearest(v: [f32; 3]) -> Dir {
        let axis = (0..3)
            .max_by(|&a, &b| v[a].abs().total_cmp(&v[b].abs()))
            .unwrap_or(1);
        let positive = v[axis] > 0.0;
        match (axis, positive) {
            (0, false) => Dir::West,
            (0, true) => Dir::East,
            (1, false) => Dir::Down,
            (1, true) => Dir::Up,
            (_, false) => Dir::North,
            (_, true) => Dir::South,
        }
    }

    /// The way it faces after the blockstate's turns.
    fn turned(self, x: i32, y: i32) -> Dir {
        let v = self.step().map(|n| n as f32);
        Dir::nearest(sub(turn(add(v, [8.0; 3]), x, y), [8.0; 3]))
    }

    /// How lit a face facing this way is, as the game shades it.
    fn light(self) -> f32 {
        match self {
            Dir::Up => 1.0,
            Dir::Down => 0.5,
            Dir::North | Dir::South => 0.8,
            Dir::West | Dir::East => 0.6,
        }
    }
}

/// A face as drawn: its corners in block units, the texture's top left,
/// top right, bottom right, and bottom left in that order, and the part
/// of its texture they show.
pub struct Quad {
    pub corners: [[f32; 3]; 4],
    /// Left, top, right, bottom, in sixteenths of the texture.
    pub uv: [f32; 4],
    pub texture: Arc<Texture>,
    /// What its texture is multiplied by: its tint and its side's light.
    pub color: [f32; 3],
    /// The side whose full, opaque neighbour hides it.
    pub cull: Option<Dir>,
    /// The side of its block it lies flat on, if any, where it is shaded
    /// by the blocks around that side.
    pub side: Option<Dir>,
}

/// A block state's faces that face the viewer, in the order they are
/// painted.
pub struct Block {
    pub id: String,
    pub quads: Vec<Quad>,
    /// Whether it fills its block and hides what is behind it.
    pub opaque: bool,
    /// Whether it fills its block, even if see-through, as glass does.
    pub cube: bool,
    /// Whether its faces shade where solid blocks meet them.
    pub ao: bool,
}

/// The way the viewer looks from: the south-east and above.
const VIEW: [f32; 3] = [1.0, 1.0, 1.0];

/// Block `id` drawn as `parts`, its models each placed as the blockstate
/// file says, with textures from `texture` and its tinted faces tinted
/// `tint`; `None` if a face's texture is missing.
pub fn build(
    id: &str,
    parts: &[(Placement, Arc<Model>)],
    texture: impl Fn(&str) -> Option<Arc<Texture>>,
    tint: [f32; 3],
) -> Option<Block> {
    let mut elements = Vec::new();
    let mut opaque = false;
    let mut cube = false;
    for (placement, model) in parts {
        for element in &model.elements {
            let mut quads = Vec::new();
            let mut faces_opaque = true;
            for face in &element.faces {
                let texture = texture(face.texture.as_deref()?)?;
                faces_opaque &= texture.opaque;
                let shape = place(element, face, placement);
                let light = if element.shade {
                    Dir::nearest(shape.normal).light()
                } else {
                    1.0
                };
                let color = if face.tinted { tint } else { [1.0; 3] };
                quads.push((
                    shape.normal,
                    Quad {
                        corners: shape.corners.map(|c| c.map(|n| n / 16.0)),
                        uv: shape.uv,
                        texture,
                        color: color.map(|c| c * light),
                        cull: shape.cull,
                        side: shape.side,
                    },
                ));
            }
            let bounds = bounds(element, placement);
            if element.rotation.is_none()
                && bounds.0.iter().all(|&n| n <= 0.0)
                && bounds.1.iter().all(|&n| n >= 16.0)
            {
                let all_sides = Dir::ALL
                    .iter()
                    .all(|d| element.faces.iter().any(|f| f.dir == *d));
                cube = true;
                opaque |= all_sides && faces_opaque;
            }
            let shown = quads
                .into_iter()
                .filter(|(normal, _)| dot(*normal, VIEW) > 1e-4)
                .map(|(_, quad)| quad)
                .collect::<Vec<_>>();
            elements.push((bounds, shown));
        }
    }
    let boxes: Vec<_> = elements.iter().map(|(b, _)| *b).collect();
    let mut elements: Vec<_> = elements.into_iter().map(Some).collect();
    let quads = order(&boxes)
        .into_iter()
        .filter_map(|i| elements[i].take())
        .flat_map(|(_, quads)| quads)
        .collect();
    Some(Block {
        id: id.to_owned(),
        quads,
        opaque,
        cube,
        ao: parts.iter().all(|(_, model)| model.ambient_occlusion),
    })
}

/// A face placed in its block, in sixteenths.
pub struct Shape {
    pub corners: [[f32; 3]; 4],
    pub uv: [f32; 4],
    pub normal: [f32; 3],
    pub cull: Option<Dir>,
    pub side: Option<Dir>,
}

/// Where `face` of `element` lies once placed by `placement`. With
/// `uvlock`, an unturned element's face takes the part of its texture its
/// place in the world gives it, so the texture keeps still as it turns.
pub fn place(element: &Element, face: &Face, placement: &Placement) -> Shape {
    let (x, y) = (placement.x, placement.y);
    let from = min(element.from, element.to);
    let to = max(element.from, element.to);
    let (corners, uv) = if placement.uvlock && element.rotation.is_none() {
        let (a, b) = (turn(from, x, y), turn(to, x, y));
        let (from, to) = (min(a, b), max(a, b));
        let dir = face.dir.turned(x, y);
        (corners(dir, from, to), default_uv(dir, from, to))
    } else {
        let mut corners = corners(face.dir, from, to);
        corners.rotate_left(usize::from(face.rotation % 4));
        if let Some(rotation) = &element.rotation {
            corners = corners.map(|c| rotate(c, rotation));
        }
        let uv = face.uv.unwrap_or_else(|| default_uv(face.dir, from, to));
        (corners.map(|c| turn(c, x, y)), uv)
    };
    let normal =
        cross(sub(corners[3], corners[0]), sub(corners[1], corners[0]));
    // A face lies on its block's side if all its corners are in that
    // side's plane.
    let side = element
        .rotation
        .is_none()
        .then(|| face.dir.turned(x, y))
        .filter(|dir| {
            let axis = dir.step().iter().position(|&n| n != 0).unwrap_or(0);
            let plane = if dir.step()[axis] > 0 { 16.0 } else { 0.0 };
            corners.iter().all(|c| (c[axis] - plane).abs() < 1e-3)
        });
    Shape {
        corners,
        uv,
        normal,
        cull: face.cullface.map(|d| d.turned(x, y)),
        side,
    }
}

/// A face's corners, its texture's top left, top right, bottom right, and
/// bottom left, on a box from `f` to `t`.
fn corners(dir: Dir, f: [f32; 3], t: [f32; 3]) -> [[f32; 3]; 4] {
    let ([x1, y1, z1], [x2, y2, z2]) = (f, t);
    match dir {
        Dir::Up => [[x1, y2, z1], [x2, y2, z1], [x2, y2, z2], [x1, y2, z2]],
        Dir::Down => [[x1, y1, z2], [x2, y1, z2], [x2, y1, z1], [x1, y1, z1]],
        Dir::North => [[x2, y2, z1], [x1, y2, z1], [x1, y1, z1], [x2, y1, z1]],
        Dir::South => [[x1, y2, z2], [x2, y2, z2], [x2, y1, z2], [x1, y1, z2]],
        Dir::West => [[x1, y2, z1], [x1, y2, z2], [x1, y1, z2], [x1, y1, z1]],
        Dir::East => [[x2, y2, z2], [x2, y2, z1], [x2, y1, z1], [x2, y1, z2]],
    }
}

/// The part of its texture a face shows when its model names none: the
/// box's own extent on that side.
fn default_uv(dir: Dir, f: [f32; 3], t: [f32; 3]) -> [f32; 4] {
    let ([x1, y1, z1], [x2, y2, z2]) = (f, t);
    match dir {
        Dir::Down => [x1, 16.0 - z2, x2, 16.0 - z1],
        Dir::Up => [x1, z1, x2, z2],
        Dir::North => [16.0 - x2, 16.0 - y2, 16.0 - x1, 16.0 - y1],
        Dir::South => [x1, 16.0 - y2, x2, 16.0 - y1],
        Dir::West => [z1, 16.0 - y2, z2, 16.0 - y1],
        Dir::East => [16.0 - z2, 16.0 - y2, 16.0 - z1, 16.0 - y1],
    }
}

/// `point` turned by an element's rotation, stretched first if it
/// rescales.
fn rotate(point: [f32; 3], rotation: &Rotation) -> [f32; 3] {
    let (sin, cos) = rotation.angle.to_radians().sin_cos();
    let mut offset = sub(point, rotation.origin);
    if rotation.rescale {
        for (axis, n) in offset.iter_mut().enumerate() {
            if axis != rotation.axis {
                *n /= cos;
            }
        }
    }
    let [a, b, c] = offset;
    let turned = match rotation.axis {
        0 => [a, b * cos - c * sin, b * sin + c * cos],
        1 => [a * cos + c * sin, b, c * cos - a * sin],
        _ => [a * cos - b * sin, a * sin + b * cos, c],
    };
    add(turned, rotation.origin)
}

/// `point`, in sixteenths, turned about the block's centre as a
/// blockstate turns a model: `about_x` degrees about the east-west axis,
/// then `about_y` about the vertical, each clockwise looking toward the
/// axis's negative end.
fn turn(point: [f32; 3], about_x: i32, about_y: i32) -> [f32; 3] {
    let [mut a, mut b, mut c] = sub(point, [8.0; 3]);
    for _ in 0..about_x.rem_euclid(360) / 90 {
        (b, c) = (c, -b);
    }
    for _ in 0..about_y.rem_euclid(360) / 90 {
        (a, c) = (-c, a);
    }
    add([a, b, c], [8.0; 3])
}

/// The box `element` fills once placed, in sixteenths.
fn bounds(element: &Element, placement: &Placement) -> ([f32; 3], [f32; 3]) {
    let mut low = [f32::MAX; 3];
    let mut high = [f32::MIN; 3];
    for i in 0..8 {
        let pick = |axis: usize| {
            if i >> axis & 1 == 0 {
                element.from[axis]
            } else {
                element.to[axis]
            }
        };
        let mut p = [pick(0), pick(1), pick(2)];
        if let Some(rotation) = &element.rotation {
            p = rotate(p, rotation);
        }
        p = turn(p, placement.x, placement.y);
        low = min(low, p);
        high = max(high, p);
    }
    (low, high)
}

/// The order to paint boxes in so that nearer ones cover farther ones. A
/// box wholly on the far side of another along some axis can't hide it,
/// so it goes first; boxes that overlap go by the depth of their centres.
pub fn order(boxes: &[([f32; 3], [f32; 3])]) -> Vec<usize> {
    let behind = |a: usize, b: usize| {
        (0..3).any(|i| boxes[a].1[i] <= boxes[b].0[i] + 1e-3)
    };
    let depth = |i: usize| {
        let (low, high) = boxes[i];
        dot(add(low, high), VIEW)
    };
    let mut left: Vec<usize> = (0..boxes.len()).collect();
    let mut order = Vec::with_capacity(boxes.len());
    while !left.is_empty() {
        // Among the boxes nothing left must go before, the farthest; if a
        // cycle leaves none, the farthest of all.
        let free = |&i: &usize| {
            !left
                .iter()
                .any(|&j| j != i && behind(j, i) && !behind(i, j))
        };
        let pick = left
            .iter()
            .copied()
            .min_by(|&a, &b| {
                let (fa, fb) = (free(&a), free(&b));
                fb.cmp(&fa).then(depth(a).total_cmp(&depth(b)))
            })
            .unwrap_or(left[0]);
        left.retain(|&i| i != pick);
        order.push(pick);
    }
    order
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn min(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])]
}

fn max(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

// The values the tests compare are exact in binary.
#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    fn element(
        from: [f32; 3],
        to: [f32; 3],
        rotation: Option<Rotation>,
    ) -> Element {
        Element {
            from,
            to,
            rotation,
            shade: true,
            faces: Vec::new(),
        }
    }

    fn face(dir: Dir) -> Face {
        Face {
            dir,
            uv: None,
            texture: None,
            cullface: Some(dir),
            rotation: 0,
            tinted: false,
        }
    }

    fn placed(x: i32, y: i32, uvlock: bool) -> Placement {
        Placement {
            model: String::new(),
            x,
            y,
            uvlock,
        }
    }

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-3)
    }

    #[test]
    fn an_element_rotation_leans_it_about_its_origin() {
        // A wall torch's stick, leaning east away from its wall to the west.
        let stick = element(
            [-1.0, 3.5, 7.0],
            [1.0, 13.5, 9.0],
            Some(Rotation {
                origin: [0.0, 3.5, 8.0],
                axis: 2,
                angle: -22.5,
                rescale: false,
            }),
        );
        let top = place(&stick, &face(Dir::Up), &placed(0, 0, false));
        let (sin, cos) = 22.5_f32.to_radians().sin_cos();
        // The top's north-west corner, ten up and one west of the origin.
        let expected = [-cos + 10.0 * sin, 3.5 + sin + 10.0 * cos, 7.0];
        assert!(close(top.corners[0], expected), "{:?}", top.corners[0]);
        assert_eq!(top.side, None);
        // Its top still faces mostly up, tipped east.
        assert!(top.normal[1] > 0.0 && top.normal[0] > 0.0);
    }

    #[test]
    fn rescaling_stretches_across_the_axis() {
        let plane = element(
            [0.8, 0.0, 8.0],
            [15.2, 16.0, 8.0],
            Some(Rotation {
                origin: [8.0, 8.0, 8.0],
                axis: 1,
                angle: 45.0,
                rescale: true,
            }),
        );
        let south = place(&plane, &face(Dir::South), &placed(0, 0, false));
        // The plane's west end, 7.2 from the middle, reaches 7.2 along each
        // of x and z once stretched by the square root of two and turned.
        assert!(
            close(south.corners[0], [0.8, 16.0, 15.2]),
            "{:?}",
            south.corners[0]
        );
    }

    #[test]
    fn a_blockstate_turn_moves_faces_and_their_cullfaces() {
        let slab = element([0.0, 0.0, 0.0], [16.0, 8.0, 16.0], None);
        // Turned 90 about x, a bottom slab's top faces north.
        let up = place(&slab, &face(Dir::Up), &placed(90, 0, false));
        assert_eq!(Dir::nearest(up.normal), Dir::North);
        assert_eq!(up.cull, Some(Dir::North));
        assert_eq!(up.side, None);
        // Its bottom lies on the block's south side, then turned 90 about
        // the vertical, its west.
        let down = place(&slab, &face(Dir::Down), &placed(90, 90, false));
        assert_eq!(down.cull, Some(Dir::West));
        assert_eq!(down.side, Some(Dir::West));
        // East turned 90 about the vertical is south, clockwise from above.
        assert_eq!(Dir::East.turned(0, 90), Dir::South);
    }

    #[test]
    fn uvlock_keeps_a_turned_top_texture_still() {
        let step = element([0.0, 8.0, 8.0], [16.0, 16.0, 16.0], None);
        let top = place(&step, &face(Dir::Up), &placed(0, 90, true));
        // Turned to the west half, its top shows the texture's west half.
        assert_eq!(top.uv, [0.0, 0.0, 8.0, 16.0]);
        assert!(close(top.corners[0], [0.0, 16.0, 0.0]));
        let unlocked = place(&step, &face(Dir::Up), &placed(0, 90, false));
        assert_eq!(unlocked.uv, [0.0, 8.0, 16.0, 16.0]);
    }

    #[test]
    fn a_face_rotation_turns_its_texture_clockwise() {
        let cube = element([0.0; 3], [16.0; 3], None);
        let mut up = face(Dir::Up);
        up.rotation = 1;
        let top = place(&cube, &up, &placed(0, 0, false));
        // The texture's top left now lies at the face's north-east corner.
        assert!(close(top.corners[0], [16.0, 16.0, 0.0]));
        assert_eq!(top.side, Some(Dir::Up));
    }

    #[test]
    fn boxes_are_painted_far_ones_first() {
        // A repeater's base and a torch standing on it: the torch's centre
        // is nearer the viewer than the base's top, but goes after it.
        let base = ([0.0, 0.0, 0.0], [16.0, 2.0, 16.0]);
        let torch = ([7.0, 2.0, 2.0], [9.0, 7.0, 4.0]);
        assert_eq!(order(&[torch, base]), [1, 0]);
        // Of two crossing boxes, the one centred farther goes first.
        let low = ([0.0, 0.0, 6.0], [16.0, 4.0, 10.0]);
        let tall = ([6.0, 0.0, 6.0], [10.0, 16.0, 10.0]);
        assert_eq!(order(&[tall, low]), [1, 0]);
    }
}
