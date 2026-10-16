//! Reading and writing Sponge `.schem` files, versions 2 and 3: a build's
//! blocks, its signs' text, and where `//paste` puts it.

use std::{
    collections::HashMap,
    hash::BuildHasher,
    io::{Read, Write},
};

use anyhow::{Context, Result, bail};
use fastnbt::Value;
use fastnbt::{ByteArray, IntArray};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use mchprs_blocks::items::Item;

/// A build's size, in blocks along x, y, z.
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

/// `data` set to paste from one block south of its south-east-bottom
/// corner; see `Nbt::place_for_paste`.
pub fn paste_from_corner(data: &[u8]) -> Result<Vec<u8>> {
    let mut nbt = decode(data)?;
    let size = Blocks::read(data)?.size;
    nbt.place_for_paste(size);
    encode(nbt)
}

/// The most a schematic may unpack to. Gzip shrinks a uniform file
/// enormously, so a small upload could otherwise unpack to gigabytes.
const MAX_UNPACKED: u64 = 16 * 1024 * 1024;

pub fn decode(data: &[u8]) -> Result<Nbt> {
    let mut raw = Vec::new();
    GzDecoder::new(data)
        .take(MAX_UNPACKED + 1)
        .read_to_end(&mut raw)
        .context("not a gzipped schematic")?;
    if raw.len() as u64 > MAX_UNPACKED {
        bail!("schematic unpacks to more than 16 MiB");
    }
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

/// NBT with every compound's fields in name order, so the same blocks
/// always encode to the same bytes and the same hash.
struct Sorted<'a>(&'a Value);

impl serde::Serialize for Sorted<'_> {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        use serde::ser::{SerializeMap as _, SerializeSeq as _};
        match self.0 {
            Value::Compound(fields) => {
                let mut sorted: Vec<_> = fields.iter().collect();
                sorted.sort_by_key(|(name, _)| *name);
                let mut map = serializer.serialize_map(Some(sorted.len()))?;
                for (name, value) in sorted {
                    map.serialize_entry(name, &Sorted(value))?;
                }
                map.end()
            }
            Value::List(items) => {
                let mut seq = serializer.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(&Sorted(item))?;
                }
                seq.end()
            }
            other => other.serialize(serializer),
        }
    }
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
        &Sorted(&Value::Compound(root)),
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

pub const AIR: &str = "minecraft:air";

/// A block position, as x, y, z.
pub type Pos = [i32; 3];

/// A schematic's blocks and sign text, by position.
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
        let entities = match blocks.get("BlockEntities") {
            Some(Value::List(entities)) => entities.as_slice(),
            _ => &[],
        };
        Ok(Blocks {
            size,
            palette,
            grid,
            signs: entities.iter().filter_map(sign).collect(),
            comparators: entities.iter().filter_map(comparator).collect(),
            containers: entities.iter().filter_map(container).collect(),
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

/// One of the six ways from a block to a neighbour, as a block state's
/// `facing` and the side a lever or sign is on name them.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[must_use]
pub enum Direction {
    North,
    South,
    West,
    East,
    Up,
    Down,
}

impl Direction {
    /// Every direction.
    pub const ALL: [Direction; 6] = [
        Direction::North,
        Direction::South,
        Direction::West,
        Direction::East,
        Direction::Up,
        Direction::Down,
    ];

    /// The four level directions, the sides of a block as dust joins them.
    pub const SIDES: [Direction; 4] = [
        Direction::North,
        Direction::South,
        Direction::West,
        Direction::East,
    ];

    /// The direction a block state's name for it gives, such as `north`.
    pub fn parse(name: &str) -> Option<Self> {
        Direction::ALL.into_iter().find(|d| d.name() == name)
    }

    /// The way the block in `state` faces, if it has a `facing`.
    pub fn facing(state: &str) -> Option<Self> {
        property(state, "facing").and_then(Direction::parse)
    }

    /// Its name in a block state, such as `north`.
    pub fn name(self) -> &'static str {
        match self {
            Direction::North => "north",
            Direction::South => "south",
            Direction::West => "west",
            Direction::East => "east",
            Direction::Up => "up",
            Direction::Down => "down",
        }
    }

    /// One block toward it: north is toward negative z, west toward
    /// negative x.
    pub fn offset(self) -> Pos {
        match self {
            Direction::North => [0, 0, -1],
            Direction::South => [0, 0, 1],
            Direction::West => [-1, 0, 0],
            Direction::East => [1, 0, 0],
            Direction::Up => [0, 1, 0],
            Direction::Down => [0, -1, 0],
        }
    }

    fn opposite(self) -> Self {
        match self {
            Direction::North => Direction::South,
            Direction::South => Direction::North,
            Direction::West => Direction::East,
            Direction::East => Direction::West,
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
        }
    }

    /// A quarter turn clockwise seen from above; up and down stay.
    pub fn turned(self) -> Self {
        match self {
            Direction::North => Direction::East,
            Direction::East => Direction::South,
            Direction::South => Direction::West,
            Direction::West => Direction::North,
            other => other,
        }
    }

    /// Mirrored east for west.
    pub fn mirrored(self) -> Self {
        match self {
            Direction::East => Direction::West,
            Direction::West => Direction::East,
            other => other,
        }
    }

    /// Whether it runs north and south rather than east and west, up, or
    /// down.
    pub fn north_south(self) -> bool {
        matches!(self, Direction::North | Direction::South)
    }
}

impl std::fmt::Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// The neighbour of `pos` one block toward `direction`.
pub fn step(pos: Pos, direction: Direction) -> Pos {
    add(pos, direction.offset())
}

/// `a` moved by `b`.
pub fn add(a: Pos, b: Pos) -> Pos {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
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
fn to_varints(values: &[usize]) -> Vec<i8> {
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

fn varints(bytes: &[i8]) -> Vec<usize> {
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
/// A block entity's position and fields, when its id ends with `kind`.
/// Version 3 nests the fields in `Data`.
fn entity<'a>(
    entity: &'a Value,
    kind: &str,
) -> Option<(Pos, &'a HashMap<String, Value>)> {
    let Value::Compound(entity) = entity else {
        return None;
    };
    match entity.get("Id") {
        Some(Value::String(id)) if id.ends_with(kind) => {}
        _ => return None,
    }
    let fields = match entity.get("Data") {
        Some(Value::Compound(data)) => data,
        _ => entity,
    };
    match entity.get("Pos") {
        Some(Value::IntArray(pos)) if pos.len() == 3 => {
            Some(([pos[0], pos[1], pos[2]], fields))
        }
        _ => None,
    }
}

fn comparator(value: &Value) -> Option<(Pos, u8)> {
    let (pos, fields) = entity(value, "comparator")?;
    match fields.get("OutputSignal") {
        Some(Value::Int(n)) => Some((pos, u8::try_from(*n).ok()?)),
        _ => None,
    }
}

fn container(value: &Value) -> Option<(Pos, Vec<(String, u32)>)> {
    let (pos, fields) = ["furnace", "barrel", "hopper"]
        .into_iter()
        .find_map(|kind| entity(value, kind))?;
    let items = match fields.get("Items") {
        Some(Value::List(items)) => items.iter().filter_map(item).collect(),
        _ => Vec::new(),
    };
    Some((pos, items))
}

/// How many slots a container a comparator reads has: a furnace 3, a
/// barrel 27, a hopper 5. `None` for any other block.
pub fn slots(state: &str) -> Option<u32> {
    match id(state) {
        "minecraft:furnace" => Some(3),
        "minecraft:barrel" => Some(27),
        "minecraft:hopper" => Some(5),
        _ => None,
    }
}

/// The signal strength a comparator reads from a container of `slots`
/// slots holding `items`, as the game works it out: 0 when empty, else 1
/// plus 14 for each container's worth of full stacks, rounded down.
pub fn container_strength(items: &[(String, u32)], slots: u32) -> u8 {
    if items.is_empty() {
        return 0;
    }
    let full: f64 = items
        .iter()
        .map(|(id, count)| {
            let stack = Item::from_name(id).map_or(64, Item::max_stack_size);
            f64::from(*count) / f64::from(stack)
        })
        .sum();
    let strength = 1.0 + full / f64::from(slots) * 14.0;
    // At most 15, as a container holds at most its slots' worth.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let strength = strength.floor().clamp(0.0, 15.0) as u8;
    strength
}

/// The fewest redstone dust that make a barrel give `strength`, in stacks
/// of up to 64: none for 0, and at least one.
pub fn barrel_redstone(strength: u8) -> Vec<u32> {
    let per_strength = 27 * 64;
    let total = (u32::from(strength.min(15)).saturating_sub(1) * per_strength)
        .div_ceil(14)
        .max(u32::from(strength > 0));
    let mut stacks = vec![64; usize::try_from(total / 64).unwrap_or(0)];
    if total % 64 > 0 {
        stacks.push(total % 64);
    }
    stacks
}

/// An item stack's id and count. Since 1.20.5 they are `id` and an int
/// `count`; before, `id` and a byte `Count`.
fn item(value: &Value) -> Option<(String, u32)> {
    let Value::Compound(item) = value else {
        return None;
    };
    let Some(Value::String(id)) = item.get("id") else {
        return None;
    };
    let count = match item.get("count").or_else(|| item.get("Count")) {
        Some(Value::Int(n)) => u32::try_from(*n).ok()?,
        Some(Value::Byte(n)) => u32::try_from(*n).ok()?,
        _ => 1,
    };
    Some((id.clone(), count))
}

fn sign(value: &Value) -> Option<(Pos, Vec<String>)> {
    let (pos, fields) = entity(value, "sign")?;
    let side = |key: &str| match fields.get(key) {
        Some(Value::Compound(side)) => match side.get("messages") {
            Some(Value::List(messages)) => {
                Some(messages.iter().map(text).collect::<Vec<String>>())
            }
            _ => None,
        },
        _ => None,
    };
    let lines = match side("front_text") {
        // Written on the back alone, the back names it.
        Some(front) if front.iter().all(String::is_empty) => {
            side("back_text").unwrap_or(front)
        }
        Some(front) => front,
        None if fields.contains_key("front_text") => return None,
        _ => ["Text1", "Text2", "Text3", "Text4"]
            .iter()
            .map(|key| fields.get(*key).map(text).unwrap_or_default())
            .collect(),
    };
    Some((pos, lines))
}

/// A sign's block entity with each line plain text, as the site writes
/// signs: its words alone, however the line held them, such as in a text
/// component's `extra` parts, with commas taken out, as words on a sign are
/// separated by spaces and lines alone. A line written as JSON, as files
/// before 1.21.5 need, stays a JSON string; any other is a plain string.
/// Any other entity is as it is.
pub fn plain_sign(entity: &Value) -> Value {
    let mut entity = entity.clone();
    let Value::Compound(outer) = &mut entity else {
        return entity;
    };
    if !matches!(outer.get("Id"), Some(Value::String(id)) if id.ends_with("sign"))
    {
        return entity;
    }
    let fields = if matches!(outer.get("Data"), Some(Value::Compound(_))) {
        let Some(Value::Compound(data)) = outer.get_mut("Data") else {
            unreachable!("checked above");
        };
        data
    } else {
        outer
    };
    for side in ["front_text", "back_text"] {
        if let Some(Value::Compound(text)) = fields.get_mut(side)
            && let Some(Value::List(lines)) = text.get_mut("messages")
        {
            for line in lines.iter_mut() {
                *line = plain_line(line);
            }
        }
    }
    for key in ["Text1", "Text2", "Text3", "Text4"] {
        if let Some(line) = fields.get_mut(key) {
            *line = plain_line(line);
        }
    }
    entity
}

/// One sign line as plain text, each run of commas and spaces one space:
/// a JSON string for a line written as JSON, else a plain string.
fn plain_line(line: &Value) -> Value {
    let words: Vec<String> = text(line)
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect();
    let plain = words.join(" ");
    match line {
        Value::String(raw) if raw.trim_start().starts_with(['"', '{', '[']) => {
            Value::String(serde_json::to_string(&plain).unwrap_or_default())
        }
        _ => Value::String(plain),
    }
}

/// The plain text of one sign line. Lines are stored as plain strings, as
/// JSON text such as `"lsb"`, `{"text":"lsb"}`, or
/// `{"text":"","extra":["ls","b"]}`, or since 1.21.5 as an NBT text
/// compound, whose words may likewise sit in its `extra` parts.
fn text(line: &Value) -> String {
    match line {
        Value::String(s) => {
            let s = s.trim();
            if s.starts_with(['"', '{', '[']) {
                match serde_json::from_str::<serde_json::Value>(s) {
                    Ok(json) => json_text(&json),
                    Err(_) => s.to_owned(),
                }
            } else {
                s.to_owned()
            }
        }
        Value::Compound(c) => {
            // An unnamed field holds plain text where a list mixes kinds.
            let own = c.get("text").or_else(|| c.get("")).map(text);
            let extra = match c.get("extra") {
                Some(Value::List(parts)) => parts.iter().map(text).collect(),
                _ => String::new(),
            };
            own.unwrap_or_default() + &extra
        }
        Value::List(parts) => parts.iter().map(text).collect(),
        _ => String::new(),
    }
}

/// The plain text of a JSON text component: its own text, then its
/// `extra` parts', in order.
fn json_text(json: &serde_json::Value) -> String {
    match json {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(parts) => {
            parts.iter().map(json_text).collect()
        }
        serde_json::Value::Object(fields) => {
            let own = fields.get("text").map(json_text).unwrap_or_default();
            let extra: String = fields
                .get("extra")
                .and_then(serde_json::Value::as_array)
                .map(|parts| parts.iter().map(json_text).collect())
                .unwrap_or_default();
            own + &extra
        }
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
    /// A barrel's block entity at `pos` holding redstone dust in `stacks`,
    /// laid out as this file lays them out: version 3 nests its fields in
    /// `Data`, and since 1.20.5 an item's count is an int `count`.
    pub fn barrel_entity(&self, pos: Pos, stacks: &[u32]) -> Value {
        let int = |key: &str| match self.body.get(key) {
            Some(Value::Int(n)) => *n,
            _ => 0,
        };
        let modern_items = int("DataVersion") >= 3837;
        let items = stacks
            .iter()
            .zip(0i8..)
            .map(|(&count, slot)| {
                let mut item = HashMap::from([
                    ("Slot".to_owned(), Value::Byte(slot)),
                    (
                        "id".to_owned(),
                        Value::String("minecraft:redstone".to_owned()),
                    ),
                ]);
                let count = if modern_items {
                    ("count".to_owned(), Value::Int(count.cast_signed()))
                } else {
                    (
                        "Count".to_owned(),
                        Value::Byte(i8::try_from(count).unwrap_or(64)),
                    )
                };
                item.insert(count.0, count.1);
                Value::Compound(item)
            })
            .collect();
        let fields = HashMap::from([("Items".to_owned(), Value::List(items))]);
        let mut entity = HashMap::from([
            (
                "Id".to_owned(),
                Value::String("minecraft:barrel".to_owned()),
            ),
            (
                "Pos".to_owned(),
                Value::IntArray(IntArray::new(pos.to_vec())),
            ),
        ]);
        if int("Version") >= 3 {
            entity.insert("Data".to_owned(), Value::Compound(fields));
        } else {
            entity.extend(fields);
        }
        Value::Compound(entity)
    }

    /// Removes and returns the block entities, such as signs.
    pub fn take_entities(&mut self) -> Vec<Value> {
        match block_fields(&mut self.body).remove("BlockEntities") {
            Some(Value::List(entities)) => entities,
            _ => Vec::new(),
        }
    }

    /// Replaces the size, palette, block data, and block entities. `low` is
    /// where the new box's first block sat in the old one, so the paste
    /// offset moves with it and `//paste` still puts the build where it
    /// did, relative to the player.
    pub fn write_blocks<S: BuildHasher>(
        &mut self,
        low: Pos,
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
        self.shift_paste(low);
        Ok(())
    }

    /// The `Metadata` compound, made if missing.
    fn metadata(&mut self) -> &mut HashMap<String, Value> {
        let entry = self
            .body
            .entry("Metadata".to_owned())
            .or_insert_with(|| Value::Compound(HashMap::new()));
        if !matches!(entry, Value::Compound(_)) {
            *entry = Value::Compound(HashMap::new());
        }
        let Value::Compound(metadata) = entry else {
            unreachable!("made a compound above");
        };
        metadata
    }

    /// Moves the paste offset, and the world position, by `low`, as the
    /// box's first block moved: version 2 keeps the paste offset in its
    /// metadata and the world position as `Offset`, and version 3 the
    /// paste offset as `Offset`.
    fn shift_paste(&mut self, low: Pos) {
        let shifted = |value: Option<&Value>| -> Value {
            let old = match value {
                Some(Value::IntArray(v)) if v.len() == 3 => [v[0], v[1], v[2]],
                _ => [0, 0, 0],
            };
            Value::IntArray(IntArray::new(
                [0, 1, 2].map(|axis| old[axis] + low[axis]).to_vec(),
            ))
        };
        let offset = shifted(self.body.get("Offset"));
        self.body.insert("Offset".to_owned(), offset);
        if self.nested {
            return;
        }
        let metadata = self.metadata();
        for (axis, key) in ["WEOffsetX", "WEOffsetY", "WEOffsetZ"]
            .into_iter()
            .enumerate()
        {
            let old = match metadata.get(key) {
                Some(Value::Int(n)) => *n,
                _ => 0,
            };
            metadata.insert(key.to_owned(), Value::Int(old + low[axis]));
        }
        if let Some(Value::Compound(worldedit)) = metadata.get_mut("WorldEdit")
        {
            let offset = shifted(worldedit.get("Offset"));
            worldedit.insert("Offset".to_owned(), offset);
        }
    }

    /// Sets where `//paste` puts the build: the player stands one block
    /// south of its south-east-bottom corner, outside it, and the build
    /// reaches west and north from in front of them, from their feet up.
    /// The paste offset is from the player to the box's north-west-bottom
    /// block. The world position the build was copied from, which only
    /// says where its uploader built it, becomes zero.
    fn place_for_paste(&mut self, [width, _, length]: Pos) {
        let paste = [-(width - 1), 0, -length];
        let ints = |v: [i32; 3]| Value::IntArray(IntArray::new(v.to_vec()));
        let nested = self.nested;
        let metadata = self.metadata();
        let worldedit = match metadata.get_mut("WorldEdit") {
            Some(Value::Compound(worldedit)) => Some(worldedit),
            _ => None,
        };
        if nested {
            // Version 3: `Offset` is the paste offset, and WorldEdit keeps
            // the world origin.
            if let Some(worldedit) = worldedit {
                worldedit.insert("Origin".to_owned(), ints([0, 0, 0]));
            }
            self.body.insert("Offset".to_owned(), ints(paste));
        } else {
            // Version 2: `Offset` is the world position, and WorldEdit
            // keeps the paste offset among the metadata.
            if let Some(worldedit) = worldedit {
                worldedit.insert("Offset".to_owned(), ints(paste));
            }
            for (key, n) in ["WEOffsetX", "WEOffsetY", "WEOffsetZ"]
                .into_iter()
                .zip(paste)
            {
                metadata.insert(key.to_owned(), Value::Int(n));
            }
            self.body.insert("Offset".to_owned(), ints([0, 0, 0]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sign line's words, however the line stores them: a plain string,
    /// JSON text, or an NBT compound, with words in its `extra` parts too.
    #[test]
    fn a_sign_lines_words_are_read_from_every_form() {
        let compound = |pairs: Vec<(&str, Value)>| {
            Value::Compound(
                pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
            )
        };
        let string = |s: &str| Value::String(s.to_owned());
        for (line, words) in [
            (string("shr"), "shr"),
            (string(r#""shr""#), "shr"),
            (string(r#"{"text":"shr"}"#), "shr"),
            (string(r#"{"text":"","extra":["sh","r"]}"#), "shr"),
            (string(r#"{"text":"a","extra":[{"text":"n"},"d"]}"#), "and"),
            (compound(vec![("text", string("shr"))]), "shr"),
            (
                compound(vec![
                    ("text", string("")),
                    ("extra", Value::List(vec![string("ror")])),
                ]),
                "ror",
            ),
            (compound(vec![("", string(""))]), ""),
        ] {
            assert_eq!(text(&line), words, "{line:?}");
        }
        // Written plain, each word once, a JSON line as a JSON string.
        let parted = compound(vec![
            ("text", string("")),
            ("extra", Value::List(vec![string("and,"), string(" or")])),
        ]);
        assert_eq!(plain_line(&parted), string("and or"));
        assert_eq!(
            plain_line(&string(r#"{"text":"","extra":["shr"]}"#)),
            string(r#""shr""#)
        );
    }

    #[test]
    fn a_barrel_takes_the_fewest_redstone_for_each_strength() {
        let items = |stacks: &[u32]| -> Vec<(String, u32)> {
            stacks
                .iter()
                .map(|&n| ("minecraft:redstone".to_owned(), n))
                .collect()
        };
        for strength in 0..=15 {
            let stacks = barrel_redstone(strength);
            assert_eq!(container_strength(&items(&stacks), 27), strength);
            let total: u32 = stacks.iter().sum();
            if total > 1 {
                assert!(
                    container_strength(&items(&[total - 1]), 27) < strength
                );
            }
        }
        assert_eq!(barrel_redstone(1), [1]);
        assert_eq!(barrel_redstone(2), [64, 60]);
        // The adder fixture's furnace: 13 redstone in 3 slots.
        assert_eq!(container_strength(&items(&[13]), 3), 1);
    }

    #[test]
    fn version_3_keeps_the_paste_offset_as_its_offset() {
        let worldedit = HashMap::from([(
            "Origin".to_owned(),
            Value::IntArray(IntArray::new(vec![300, 64, -80])),
        )]);
        let mut nbt = Nbt {
            body: HashMap::from([(
                "Metadata".to_owned(),
                Value::Compound(HashMap::from([(
                    "WorldEdit".to_owned(),
                    Value::Compound(worldedit),
                )])),
            )]),
            nested: true,
        };
        nbt.place_for_paste([5, 3, 4]);
        let ints = |value: Option<&Value>| match value {
            Some(Value::IntArray(v)) => v.iter().copied().collect::<Vec<_>>(),
            other => panic!("not an int array: {other:?}"),
        };
        assert_eq!(ints(nbt.body.get("Offset")), [-4, 0, -4]);
        let Some(Value::Compound(metadata)) = nbt.body.get("Metadata") else {
            panic!("no metadata");
        };
        let Some(Value::Compound(worldedit)) = metadata.get("WorldEdit") else {
            panic!("no WorldEdit section");
        };
        assert_eq!(ints(worldedit.get("Origin")), [0, 0, 0]);
    }
}
