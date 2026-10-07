use std::{
    collections::HashMap,
    hash::BuildHasher,
    io::{Read, Write},
};

use anyhow::{Context, Result, bail};
use fastnbt::Value;
use fastnbt::{ByteArray, IntArray};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};

/// A build's size inside its corner markers, in blocks along x, y, z.
pub struct Size {
    pub x: u16,
    pub y: u16,
    pub z: u16,
}

/// The schematic's top-level fields.
fn body(data: &[u8]) -> Result<HashMap<String, Value>> {
    Ok(decode(data)?.body)
}

/// A schematic's NBT, kept whole so it can be changed and written back.
pub struct Nbt {
    pub body: HashMap<String, Value>,
    /// Version 3 nests the fields in a `Schematic` compound; version 2
    /// keeps them at the root.
    nested: bool,
}

pub fn decode(data: &[u8]) -> Result<Nbt> {
    let mut raw = Vec::new();
    GzDecoder::new(data)
        .read_to_end(&mut raw)
        .context("not a gzipped schematic")?;
    let Value::Compound(mut root) = fastnbt::from_bytes::<Value>(&raw)? else {
        bail!("schematic root is not an NBT compound");
    };
    if root.contains_key("Materials") {
        bail!("legacy MCEdit .schematic files are not supported");
    }
    Ok(match root.remove("Schematic") {
        Some(Value::Compound(nested)) => Nbt {
            body: nested,
            nested: true,
        },
        _ => Nbt {
            body: root,
            nested: false,
        },
    })
}

/// Gzipped NBT, laid out as the file it was decoded from.
pub fn encode(nbt: Nbt) -> Result<Vec<u8>> {
    let (root, name) = if nbt.nested {
        let root = HashMap::from([(
            "Schematic".to_owned(),
            Value::Compound(nbt.body),
        )]);
        (root, "")
    } else {
        (nbt.body, "Schematic")
    };
    let raw = fastnbt::to_bytes_with_opts(
        &Value::Compound(root),
        fastnbt::SerOpts::new().root_name(name),
    )?;
    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    gz.write_all(&raw)?;
    Ok(gz.finish()?)
}

// Sponge stores dimensions as unsigned shorts in NBT's signed short type.
fn dimension(body: &HashMap<String, Value>, key: &str) -> Result<u16> {
    match body.get(key) {
        Some(Value::Short(n)) => Ok(n.cast_unsigned()),
        _ => bail!("schematic has no {key}"),
    }
}

/// Where the corner markers go in a bounding box of `size`: each of its
/// eight corners, and the block next to it along each axis.
pub fn marker_positions(size: Pos) -> Vec<Pos> {
    let [w, h, l] = size;
    let mut positions = Vec::new();
    for x in [0, w - 1] {
        for y in [0, h - 1] {
            for z in [0, l - 1] {
                // One step from the corner toward the box's middle.
                let [dx, dy, dz] =
                    [x, y, z].map(|n| if n == 0 { 1 } else { -1 });
                positions.extend([
                    [x, y, z],
                    [x + dx, y, z],
                    [x, y + dy, z],
                    [x, y, z + dz],
                ]);
            }
        }
    }
    positions
}

pub const AIR: &str = "minecraft:air";

/// A block position, as x, y, z.
pub type Pos = [i32; 3];

/// A schematic's blocks and sign text, by position.
pub struct Blocks {
    /// The full bounding box, corner markers included.
    pub size: Pos,
    palette: Vec<String>,
    grid: Vec<usize>,
    /// The front text of each sign, one string per line.
    pub signs: HashMap<Pos, Vec<String>>,
}

impl Blocks {
    pub fn read(data: &[u8]) -> Result<Self> {
        let body = body(data)?;
        let size = [
            dimension(&body, "Width")?,
            dimension(&body, "Height")?,
            dimension(&body, "Length")?,
        ]
        .map(i32::from);
        // Version 3 gathers the block fields in a `Blocks` compound.
        let blocks = match body.get("Blocks") {
            Some(Value::Compound(nested)) => nested,
            _ => &body,
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
        let signs = match blocks.get("BlockEntities") {
            Some(Value::List(entities)) => {
                entities.iter().filter_map(sign).collect()
            }
            _ => HashMap::new(),
        };
        Ok(Blocks {
            size,
            palette,
            grid,
            signs,
        })
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
    pub fn full_size(&self) -> Result<Size> {
        let [x, y, z] = self.size.map(|n| u16::try_from(n).unwrap_or(0));
        Ok(Size { x, y, z })
    }

    /// The size inside the corner markers, one block in from every face.
    pub fn inner_size(&self) -> Result<Size> {
        let [x, y, z] = self.size.map(|n| u16::try_from(n - 2).unwrap_or(0));
        if x == 0 || y == 0 || z == 0 {
            bail!("schematic is too small to hold corner markers");
        }
        Ok(Size { x, y, z })
    }

    /// Where the corner markers go.
    pub fn marker_positions(&self) -> Vec<Pos> {
        marker_positions(self.size)
    }

    /// Marker positions that do not hold glass.
    pub fn missing_markers(&self) -> Vec<Pos> {
        self.marker_positions()
            .into_iter()
            .filter(|&pos| self.get(pos) != "minecraft:glass")
            .collect()
    }
}

impl Blocks {
    /// Each lever with the block it is attached to.
    pub fn levers(&self) -> Vec<Lever> {
        self.iter()
            .filter(|(_, state)| id(state) == "minecraft:lever")
            .filter_map(|(pos, state)| {
                // A floor or ceiling lever's `facing` is only the way its
                // handle points.
                let side = match property(state, "face")? {
                    "floor" => "up",
                    "ceiling" => "down",
                    _ => property(state, "facing")?,
                };
                Some(Lever {
                    pos,
                    block: step(pos, opposite(side)),
                    side: side.to_owned(),
                    on: property(state, "powered") == Some("true"),
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
            Some(step(pos, "up"))
        } else if name.ends_with("_wall_sign") {
            Some(step(pos, opposite(property(state, "facing")?)))
        } else {
            Some(step(pos, "down"))
        }
    }
}

pub struct Lever {
    pub pos: Pos,
    /// The block the lever is attached to, which it powers.
    pub block: Pos,
    /// The side of `block` the lever is on, such as `south` or `up`.
    pub side: String,
    pub on: bool,
}

/// The block a redstone torch at `pos` stands on or hangs from.
pub fn torch_block(pos: Pos, state: &str) -> Option<Pos> {
    match id(state) {
        "minecraft:redstone_torch" => Some(step(pos, "down")),
        "minecraft:redstone_wall_torch" => {
            Some(step(pos, opposite(property(state, "facing")?)))
        }
        _ => None,
    }
}

/// The neighbour of `pos` one block toward `direction`, such as `north`.
pub fn step(pos: Pos, direction: &str) -> Pos {
    let [x, y, z] = pos;
    match direction {
        "north" => [x, y, z - 1],
        "south" => [x, y, z + 1],
        "west" => [x - 1, y, z],
        "east" => [x + 1, y, z],
        "up" => [x, y + 1, z],
        "down" => [x, y - 1, z],
        _ => pos,
    }
}

pub fn opposite(direction: &str) -> &'static str {
    match direction {
        "north" => "south",
        "south" => "north",
        "west" => "east",
        "east" => "west",
        "up" => "down",
        _ => "up",
    }
}

pub const DIRECTIONS: [&str; 6] =
    ["north", "south", "west", "east", "up", "down"];

/// The block id of a state, such as `minecraft:lever`.
pub fn id(state: &str) -> &str {
    state.split('[').next().unwrap_or(state)
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

/// Sponge stores each palette index as an unsigned LEB128 varint.
pub fn to_varints(values: &[usize]) -> Vec<i8> {
    let mut bytes = Vec::new();
    for &value in values {
        let mut rest = value;
        loop {
            // The mask keeps seven bits, so the cast cannot truncate.
            #[allow(clippy::cast_possible_truncation)]
            let low = (rest & 0x7f) as u8;
            rest >>= 7;
            if rest == 0 {
                bytes.push(low.cast_signed());
                break;
            }
            bytes.push((low | 0x80).cast_signed());
        }
    }
    bytes
}

pub fn varints(bytes: &[i8]) -> Vec<usize> {
    let mut values = Vec::new();
    let mut value = 0;
    let mut shift = 0;
    for &byte in bytes {
        let byte = byte.cast_unsigned();
        value |= usize::from(byte & 0x7f) << shift;
        shift += 7;
        if byte & 0x80 == 0 {
            values.push(value);
            value = 0;
            shift = 0;
        }
    }
    values
}

/// A sign block entity's position and front text. Version 3 nests an
/// entity's fields in `Data`; signs before Minecraft 1.20 keep their lines
/// in `Text1` to `Text4`.
fn sign(entity: &Value) -> Option<(Pos, Vec<String>)> {
    let Value::Compound(entity) = entity else {
        return None;
    };
    // Comparators and other block entities share the list.
    match entity.get("Id") {
        Some(Value::String(id)) if id.ends_with("sign") => {}
        _ => return None,
    }
    let fields = match entity.get("Data") {
        Some(Value::Compound(data)) => data,
        _ => entity,
    };
    let pos = match entity.get("Pos") {
        Some(Value::IntArray(pos)) if pos.len() == 3 => {
            [pos[0], pos[1], pos[2]]
        }
        _ => return None,
    };
    let lines = match fields.get("front_text") {
        Some(Value::Compound(front)) => match front.get("messages") {
            Some(Value::List(messages)) => messages.iter().map(text).collect(),
            _ => return None,
        },
        _ => ["Text1", "Text2", "Text3", "Text4"]
            .iter()
            .map(|key| fields.get(*key).map(text).unwrap_or_default())
            .collect(),
    };
    Some((pos, lines))
}

/// The plain text of one sign line. Lines are stored as plain strings, as
/// JSON text such as `"lsb"` or `{"text":"lsb"}`, or since 1.21.5 as an NBT
/// text compound.
fn text(line: &Value) -> String {
    match line {
        Value::String(s) => {
            let s = s.trim();
            if let Some(rest) = s.split_once(r#""text":""#).map(|(_, r)| r) {
                rest.split('"').next().unwrap_or("").to_owned()
            } else if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
                s[1..s.len() - 1].to_owned()
            } else {
                s.to_owned()
            }
        }
        Value::Compound(c) => c.get("text").map(text).unwrap_or_default(),
        _ => String::new(),
    }
}

/// The fields that hold blocks: a `Blocks` compound in version 3, the body
/// itself in version 2.
fn block_fields(
    body: &mut HashMap<String, Value>,
) -> &mut HashMap<String, Value> {
    if matches!(body.get("Blocks"), Some(Value::Compound(_))) {
        let Some(Value::Compound(blocks)) = body.get_mut("Blocks") else {
            unreachable!("checked above");
        };
        blocks
    } else {
        body
    }
}

pub fn entity_pos(entity: &Value) -> Option<Pos> {
    let Value::Compound(fields) = entity else {
        return None;
    };
    match fields.get("Pos") {
        Some(Value::IntArray(pos)) if pos.len() == 3 => {
            Some([pos[0], pos[1], pos[2]])
        }
        _ => None,
    }
}

pub fn with_pos(entity: &Value, pos: Pos) -> Value {
    let mut entity = entity.clone();
    if let Value::Compound(fields) = &mut entity {
        fields.insert(
            "Pos".to_owned(),
            Value::IntArray(IntArray::new(pos.to_vec())),
        );
    }
    entity
}

impl Nbt {
    /// Removes and returns the block entities, such as signs.
    pub fn take_entities(&mut self) -> Vec<Value> {
        match block_fields(&mut self.body).remove("BlockEntities") {
            Some(Value::List(entities)) => entities,
            _ => Vec::new(),
        }
    }

    /// Replaces the size, palette, block data, and block entities.
    pub fn write_blocks<S: BuildHasher>(
        &mut self,
        size: Pos,
        grid: &HashMap<Pos, String, S>,
        entities: Vec<Value>,
    ) -> Result<()> {
        let body = &mut self.body;
        let [w, h, l] = size;
        let mut palette: HashMap<String, usize> = HashMap::new();
        palette.insert(AIR.to_owned(), 0);
        let mut indices = Vec::new();
        for y in 0..h {
            for z in 0..l {
                for x in 0..w {
                    let state =
                        grid.get(&[x, y, z]).map_or(AIR, String::as_str);
                    let next = palette.len();
                    indices
                        .push(*palette.entry(state.to_owned()).or_insert(next));
                }
            }
        }
        for (key, n) in [("Width", w), ("Height", h), ("Length", l)] {
            let n = u16::try_from(n)
                .context("build is too large for a schematic")?;
            body.insert(key.to_owned(), Value::Short(n.cast_signed()));
        }
        let palette_len = i32::try_from(palette.len())?;
        let palette = palette
            .into_iter()
            .map(|(state, i)| Ok((state, Value::Int(i32::try_from(i)?))))
            .collect::<Result<HashMap<_, _>>>()?;
        if body.contains_key("PaletteMax") {
            body.insert("PaletteMax".to_owned(), Value::Int(palette_len));
        }
        let fields = block_fields(body);
        let data_key = if fields.contains_key("Data") {
            "Data"
        } else {
            "BlockData"
        };
        fields.insert(
            data_key.to_owned(),
            Value::ByteArray(ByteArray::new(to_varints(&indices))),
        );
        fields.insert("Palette".to_owned(), Value::Compound(palette));
        fields.insert("BlockEntities".to_owned(), Value::List(entities));
        Ok(())
    }
}
