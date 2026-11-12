//! A schematic's blocks by position, with the levers, buttons, signs, and
//! torches attached to them, and the block states that name each block,
//! such as `minecraft:lever[face=wall,facing=south,powered=false]`, read
//! and changed a property at a time.

use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use fastnbt::Value;

use super::{
    AIR, Pos,
    direction::{Direction, step},
    entity::{comparator, container, sign},
    lines::Roles,
    nbt::{Nbt, Size, decode, dimension, encode, varints},
};

/// A schematic's blocks and sign text, by position, with the file they
/// were read from, so they can be changed and written back.
#[derive(Clone)]
pub struct Blocks {
    /// The bounding box.
    pub size: Pos,
    palette: Vec<String>,
    grid: Vec<usize>,
    /// The front text of each sign, one string per line.
    pub signs: HashMap<Pos, Vec<String>>,
    /// The signal strength each comparator was saved putting out.
    pub comparators: HashMap<Pos, u8>,
    /// What each furnace, barrel, and hopper holds: each stack's item id,
    /// such as `minecraft:redstone`, and count. A comparator reads how full
    /// it is.
    pub containers: HashMap<Pos, Vec<(String, u32)>>,
    /// The roles the file carries, which probing found or the database
    /// keeps with its version; `None` for a file carrying none, whose
    /// roles are read from its shapes. See [`super::lines`].
    pub roles: Option<Roles>,
    /// The file they were read from; `None` for blocks made from a grid.
    file: Option<Nbt>,
}

impl Blocks {
    pub fn read(data: &[u8]) -> Result<Self> {
        Self::of(decode(data)?)
    }

    /// The blocks the file `nbt` holds, which they keep to be changed and
    /// written back.
    pub fn of(nbt: Nbt) -> Result<Self> {
        let body = &nbt.body;
        let size = [
            dimension(body, "Width")?,
            dimension(body, "Height")?,
            dimension(body, "Length")?,
        ]
        .map(i32::from);
        // Version 3 gathers the block fields in a `Blocks` compound.
        let blocks = match body.get("Blocks") {
            Some(Value::Compound(nested)) => nested,
            _ => body,
        };
        let Some(Value::Compound(names)) = blocks.get("Palette") else {
            bail!("schematic has no block palette");
        };
        let mut palette = vec![String::new(); names.len()];
        for (name, index) in names {
            let Value::Int(index) = index else {
                bail!("palette entry {name} has no index");
            };
            usize::try_from(*index)
                .ok()
                .and_then(|i| palette.get_mut(i))
                .context("palette index out of range")?
                .clone_from(name);
        }
        let Some(Value::ByteArray(data)) =
            blocks.get("Data").or_else(|| blocks.get("BlockData"))
        else {
            bail!("schematic has no block data");
        };
        let grid = varints(data);
        let volume = size.iter().map(|&n| usize::try_from(n).unwrap_or(0));
        if grid.len() != volume.product::<usize>()
            || grid.iter().any(|&i| i >= palette.len())
        {
            bail!("block data does not match the schematic's size");
        }
        let entities = match blocks.get("BlockEntities") {
            Some(Value::List(entities)) => entities.as_slice(),
            _ => &[],
        };
        let signs = entities.iter().filter_map(sign).collect();
        let comparators = entities.iter().filter_map(comparator).collect();
        let containers = entities.iter().filter_map(container).collect();
        Ok(Blocks {
            size,
            palette,
            grid,
            signs,
            comparators,
            containers,
            roles: nbt.roles(),
            file: Some(nbt),
        })
    }

    /// The file they were read from, carrying their roles as they now are,
    /// to change and write back.
    pub fn nbt(&self) -> Result<Nbt> {
        let mut nbt = self
            .file
            .clone()
            .context("blocks made from a grid have no file")?;
        nbt.set_roles(self.roles.as_ref());
        Ok(nbt)
    }

    /// The file written back as it is worked on, carrying its roles; see
    /// [`super::lines`].
    pub fn encode(&self) -> Result<Vec<u8>> {
        encode(self.nbt()?)
    }

    /// A schematic of `size` holding `states` by position, air elsewhere,
    /// as a test or an example builds one.
    pub fn from_grid(size: Pos, states: &HashMap<Pos, String>) -> Self {
        let mut palette = vec![AIR.to_owned()];
        let [width, height, length] = size;
        let volume = usize::try_from(width * height * length).unwrap_or(0);
        let mut grid = vec![0; volume];
        for (&[x, y, z], state) in states {
            let index = palette.iter().position(|known| known == state);
            let index = index.unwrap_or_else(|| {
                palette.push(state.clone());
                palette.len() - 1
            });
            let at = x + z * width + y * width * length;
            grid[usize::try_from(at).unwrap_or(0)] = index;
        }
        Blocks {
            size,
            palette,
            grid,
            signs: HashMap::new(),
            comparators: HashMap::new(),
            containers: HashMap::new(),
            roles: None,
            file: None,
        }
    }

    /// The block state at `pos`, such as
    /// `minecraft:lever[face=wall,facing=south,powered=false]`. Outside the
    /// schematic, air.
    pub fn get(&self, pos: Pos) -> &str {
        let [width, height, length] = self.size;
        let [x, y, z] = pos;
        if !(0..width).contains(&x)
            || !(0..height).contains(&y)
            || !(0..length).contains(&z)
        {
            return "minecraft:air";
        }
        let index =
            usize::try_from(x + z * width + y * width * length).unwrap_or(0);
        &self.palette[self.grid[index]]
    }

    /// Every position with its block state, x fastest.
    pub fn iter(&self) -> impl Iterator<Item = (Pos, &str)> {
        let [w, h, l] = self.size;
        (0..h).flat_map(move |y| {
            (0..l).flat_map(move |z| {
                (0..w).map(move |x| ([x, y, z], self.get([x, y, z])))
            })
        })
    }

    /// The whole bounding box.
    pub fn full_size(&self) -> Size {
        let [x, y, z] = self.size.map(|n| u16::try_from(n).unwrap_or(0));
        Size { x, y, z }
    }
}

impl Blocks {
    /// Each lever and button with the block it is attached to.
    pub fn switches(&self) -> Vec<Switch> {
        self.iter()
            .filter(|(_, state)| {
                id(state) == "minecraft:lever" || id(state).ends_with("_button")
            })
            .filter_map(|(pos, state)| {
                // A floor or ceiling lever's or button's `facing` is only the
                // way it points.
                let side = match property(state, "face")? {
                    "floor" => Direction::Up,
                    "ceiling" => Direction::Down,
                    _ => Direction::facing(state)?,
                };
                Some(Switch {
                    pos,
                    block: step(pos, side.opposite()),
                    side,
                    on: property(state, "powered") == Some("true"),
                    button: id(state).ends_with("_button"),
                })
            })
            .collect()
    }

    /// Each sign's text, by the block the sign is attached to.
    pub fn sign_texts(&self) -> HashMap<Pos, &[String]> {
        self.signs
            .iter()
            .filter_map(|(&pos, lines)| {
                Some((self.sign_block(pos)?, lines.as_slice()))
            })
            .collect()
    }

    /// The block the sign at `pos` is attached to. Wall hanging signs hang
    /// from no block.
    pub fn sign_block(&self, pos: Pos) -> Option<Pos> {
        let state = self.get(pos);
        let name = id(state);
        if name.ends_with("_wall_hanging_sign") {
            None
        } else if name.ends_with("_hanging_sign") {
            Some(step(pos, Direction::Up))
        } else if name.ends_with("_wall_sign") {
            Some(step(pos, Direction::facing(state)?.opposite()))
        } else {
            Some(step(pos, Direction::Down))
        }
    }
}

/// A lever, or a button, which a control line may use instead.
pub struct Switch {
    pub pos: Pos,
    /// The block the switch is attached to, which it powers.
    pub block: Pos,
    /// The side of `block` the switch is on, such as south or up.
    pub side: Direction,
    pub on: bool,
    /// A button rather than a lever: it powers its block for a moment.
    pub button: bool,
}

/// The block a redstone torch at `pos` stands on or hangs from.
pub fn torch_block(pos: Pos, state: &str) -> Option<Pos> {
    match id(state) {
        "minecraft:redstone_torch" => Some(step(pos, Direction::Down)),
        "minecraft:redstone_wall_torch" => {
            Some(step(pos, Direction::facing(state)?.opposite()))
        }
        _ => None,
    }
}

/// The block id of a state, such as `minecraft:lever`.
pub fn id(state: &str) -> &str {
    state.split('[').next().unwrap_or(state)
}

/// A block state's id without its namespace, such as `lever`.
pub fn short(state: &str) -> &str {
    let id = id(state);
    id.strip_prefix("minecraft:").unwrap_or(id)
}

/// A block state without the properties that follow its power, such as
/// `minecraft:redstone_wire[east=side,north=none,south=up,west=none]`, so
/// two saves of one build compare alike however their power stood. A
/// lever keeps `powered`, being how it was set rather than powered.
pub fn unpowered(state: &str) -> String {
    let Some((id, properties)) = state.split_once('[') else {
        return state.to_owned();
    };
    let lever = id.ends_with(":lever");
    let kept: Vec<&str> = properties
        .trim_end_matches(']')
        .split(',')
        .filter(|pair| {
            let key = pair.split('=').next().unwrap_or_default();
            !matches!(key, "power" | "lit" | "locked")
                && (key != "powered" || lever)
        })
        .collect();
    format!("{id}[{}]", kept.join(","))
}

/// A block state as a player places it, before the game works out what
/// depends on its neighbours: dust as its bare id, its strength and joins
/// left out, and any other block without whether it is powered or lit,
/// a lever included. Stacked copies whose blocks differ only so do not
/// collide, as unlike `unpowered`, which tells two saves apart by how
/// their dust runs.
pub fn as_placed(state: &str) -> String {
    if short(state) == "redstone_wire" {
        return id(state).to_owned();
    }
    let Some((id, properties)) = state.split_once('[') else {
        return state.to_owned();
    };
    let kept: Vec<&str> = properties
        .trim_end_matches(']')
        .split(',')
        .filter(|p| !p.starts_with("powered=") && !p.starts_with("lit="))
        .collect();
    format!("{id}[{}]", kept.join(","))
}

/// `state` with its property `key` set to `value`.
pub fn with_property(state: &str, key: &str, value: &str) -> String {
    let Some((id, rest)) = state.split_once('[') else {
        return state.to_owned();
    };
    let properties: Vec<String> = rest
        .trim_end_matches(']')
        .split(',')
        .map(|pair| match pair.split_once('=') {
            Some((k, _)) if k == key => format!("{k}={value}"),
            _ => pair.to_owned(),
        })
        .collect();
    format!("{id}[{}]", properties.join(","))
}

/// A property of a block state, such as `facing`.
pub fn property<'a>(state: &'a str, key: &str) -> Option<&'a str> {
    let properties = state.split_once('[')?.1.strip_suffix(']')?;
    properties
        .split(',')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v)
}
