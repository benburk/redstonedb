//! Block models: a model file with its parents', its texture variables
//! followed to the textures they name, and the elements of the nearest
//! model in the chain that has any.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::geometry::Dir;

/// A model as drawn: its boxes, and whether its block shades where it
/// meets others.
#[derive(Debug, Default)]
pub struct Model {
    pub elements: Vec<Element>,
    pub ambient_occlusion: bool,
}

/// A box of a model, from `from` to `to` in sixteenths of a block.
#[derive(Debug, Clone)]
pub struct Element {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub rotation: Option<Rotation>,
    /// Whether its faces are shaded by the way they face.
    pub shade: bool,
    pub faces: Vec<Face>,
}

/// An element's turn about an axis through `origin`.
#[derive(Debug, Clone, Copy)]
pub struct Rotation {
    pub origin: [f32; 3],
    /// 0, 1, or 2 for x, y, or z.
    pub axis: usize,
    /// Degrees, counter-clockwise looking down the axis toward its origin.
    pub angle: f32,
    /// Whether the faces stretch across the axis to keep their extent.
    pub rescale: bool,
}

/// One face of an element.
#[derive(Debug, Clone)]
pub struct Face {
    pub dir: Dir,
    /// The texture's part shown, left, top, right, bottom, in sixteenths;
    /// `None` takes it from the element's bounds.
    pub uv: Option<[f32; 4]>,
    /// The texture's asset path, such as `textures/block/stone.png`, or
    /// `None` when its variable leads nowhere.
    pub texture: Option<String>,
    /// The side whose full, opaque neighbour hides it.
    pub cullface: Option<Dir>,
    /// Quarter turns of the texture, clockwise.
    pub rotation: u8,
    /// Whether it takes its block's tint, such as dust's by its power.
    pub tinted: bool,
}

/// The asset path of model `name`, such as `models/block/lever.json` for
/// `minecraft:block/lever`, or `None` for another namespace's.
pub fn model_path(name: &str) -> Option<String> {
    asset(name).map(|name| format!("models/{name}.json"))
}

/// The asset path of texture `name`, such as `textures/block/stone.png`.
pub fn texture_path(name: &str) -> Option<String> {
    asset(name).map(|name| format!("textures/{name}.png"))
}

/// A resource name without its namespace, a bare one taken as a block's.
fn asset(name: &str) -> Option<&str> {
    let name = match name.split_once(':') {
        Some(("minecraft", rest)) => rest,
        Some(_) => return None,
        None => name,
    };
    Some(name)
}

/// Model `name` with its parents, each file read by `json` by its asset
/// path, or `None` when the model itself is missing. A missing parent
/// ends the chain there.
pub fn load(
    name: &str,
    json: impl Fn(&str) -> Option<Arc<Value>>,
) -> Option<Model> {
    let bare = |name: &str| {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        if name.contains('/') {
            name.to_owned()
        } else {
            format!("block/{name}")
        }
    };
    let mut chain: Vec<Arc<Value>> = Vec::new();
    let mut next = Some(bare(name));
    // A cycle of parents ends after a long chain rather than never.
    while let Some(name) = next.take().filter(|_| chain.len() < 32) {
        let Some(file) = model_path(&name).and_then(|path| json(&path)) else {
            break;
        };
        next = file.get("parent").and_then(Value::as_str).map(bare);
        chain.push(file);
    }
    if chain.is_empty() {
        return None;
    }
    // A child's textures override its parents'.
    let mut textures: HashMap<&str, &str> = HashMap::new();
    for file in chain.iter().rev() {
        if let Some(map) = file.get("textures").and_then(Value::as_object) {
            for (key, value) in map {
                if let Some(value) = value.as_str() {
                    textures.insert(key, value);
                }
            }
        }
    }
    let elements = chain
        .iter()
        .find_map(|file| file.get("elements")?.as_array())
        .map(|list| list.iter().filter_map(|e| element(e, &textures)).collect())
        .unwrap_or_default();
    let ambient_occlusion = chain
        .iter()
        .find_map(|file| file.get("ambientocclusion")?.as_bool())
        .unwrap_or(true);
    Some(Model {
        elements,
        ambient_occlusion,
    })
}

/// The texture `reference` names, `#` variables followed through
/// `textures`, as an asset path.
pub fn resolve(
    reference: &str,
    textures: &HashMap<&str, &str>,
) -> Option<String> {
    let mut name = reference;
    for _ in 0..16 {
        match name.strip_prefix('#') {
            Some(variable) => name = textures.get(variable)?,
            None => return texture_path(name),
        }
    }
    None
}

fn element(value: &Value, textures: &HashMap<&str, &str>) -> Option<Element> {
    let rotation = value.get("rotation").and_then(|r| {
        Some(Rotation {
            origin: triple(r.get("origin")?)?,
            axis: match r.get("axis")?.as_str()? {
                "x" => 0,
                "y" => 1,
                "z" => 2,
                _ => return None,
            },
            angle: r.get("angle")?.as_f64()? as f32,
            rescale: r.get("rescale").and_then(Value::as_bool).unwrap_or(false),
        })
    });
    let faces = value
        .get("faces")
        .and_then(Value::as_object)
        .map(|faces| {
            faces
                .iter()
                .filter_map(|(dir, face)| {
                    Some(Face {
                        dir: Dir::parse(dir)?,
                        uv: face.get("uv").and_then(|uv| {
                            let uv = numbers(uv)?;
                            uv.try_into().ok()
                        }),
                        texture: face
                            .get("texture")
                            .and_then(Value::as_str)
                            .and_then(|t| resolve(t, textures)),
                        cullface: face
                            .get("cullface")
                            .and_then(Value::as_str)
                            .and_then(Dir::parse),
                        rotation: face
                            .get("rotation")
                            .and_then(Value::as_i64)
                            .map_or(0, |r| (r.rem_euclid(360) / 90) as u8),
                        tinted: face.get("tintindex").is_some(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Element {
        from: triple(value.get("from")?)?,
        to: triple(value.get("to")?)?,
        rotation,
        shade: value.get("shade").and_then(Value::as_bool).unwrap_or(true),
        faces,
    })
}

fn numbers(value: &Value) -> Option<Vec<f32>> {
    value
        .as_array()?
        .iter()
        .map(|n| n.as_f64().map(|n| n as f32))
        .collect()
}

fn triple(value: &Value) -> Option<[f32; 3]> {
    numbers(value)?.try_into().ok()
}

// The values the tests compare are exact in binary.
#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use serde_json::json;

    fn files() -> HashMap<String, Arc<Value>> {
        [
            ("block/block", json!({ "display": {} })),
            (
                "block/cube",
                json!({ "parent": "block/block", "elements": [{
                    "from": [0, 0, 0], "to": [16, 16, 16],
                    "faces": {
                        "up": { "texture": "#up", "cullface": "up" },
                        "north": { "texture": "#north", "cullface": "north", "rotation": 90, "uv": [0, 0, 8, 8] },
                    }
                }]}),
            ),
            (
                "block/cube_all",
                json!({ "parent": "block/cube", "textures": { "up": "#all", "north": "#all" } }),
            ),
            (
                "block/glass",
                json!({ "parent": "minecraft:block/cube_all", "textures": { "all": "minecraft:block/glass" } }),
            ),
            (
                "block/torch",
                json!({ "parent": "block/missing", "ambientocclusion": false, "textures": { "torch": "block/torch" },
                    "elements": [{ "from": [7, 0, 7], "to": [9, 10, 9], "shade": false,
                        "rotation": { "origin": [8, 0, 8], "axis": "z", "angle": -22.5 },
                        "faces": { "up": { "texture": "#torch", "tintindex": 0 }, "east": { "texture": "#nowhere" } } }] }),
            ),
        ]
        .into_iter()
        .map(|(name, file)| (format!("models/{name}.json"), Arc::new(file)))
        .collect()
    }

    #[test]
    fn a_model_takes_its_parents_elements_and_textures() {
        let files = files();
        let glass =
            load("minecraft:block/glass", |path| files.get(path).cloned())
                .unwrap();
        assert!(glass.ambient_occlusion);
        let [cube] = glass.elements.as_slice() else {
            panic!("one element");
        };
        assert_eq!(cube.to, [16.0; 3]);
        let up = cube.faces.iter().find(|f| f.dir == Dir::Up).unwrap();
        assert_eq!(up.texture.as_deref(), Some("textures/block/glass.png"));
        assert_eq!(up.cullface, Some(Dir::Up));
        let north = cube.faces.iter().find(|f| f.dir == Dir::North).unwrap();
        assert_eq!(north.texture.as_deref(), Some("textures/block/glass.png"));
        assert_eq!((north.rotation, north.uv), (1, Some([0.0, 0.0, 8.0, 8.0])));
    }

    #[test]
    fn an_element_keeps_its_rotation_shade_and_tint() {
        let files = files();
        let torch =
            load("block/torch", |path| files.get(path).cloned()).unwrap();
        assert!(!torch.ambient_occlusion);
        let stick = &torch.elements[0];
        assert!(!stick.shade);
        let rotation = stick.rotation.unwrap();
        assert_eq!((rotation.axis, rotation.angle), (2, -22.5));
        let up = stick.faces.iter().find(|f| f.dir == Dir::Up).unwrap();
        assert!(up.tinted);
        assert_eq!(up.texture.as_deref(), Some("textures/block/torch.png"));
        // A variable naming nothing leaves the face without a texture.
        let east = stick.faces.iter().find(|f| f.dir == Dir::East).unwrap();
        assert_eq!(east.texture, None);
        assert!(
            load("block/absent", |path| files.get(path).cloned()).is_none()
        );
    }
}
