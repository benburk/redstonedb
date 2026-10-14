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
                    "floor" => "up",
                    "ceiling" => "down",
                    _ => property(state, "facing")?,
                };
                Some(Switch {
                    pos,
                    block: step(pos, opposite(side)),
                    side: side.to_owned(),
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
            Some(step(pos, "up"))
        } else if name.ends_with("_wall_sign") {
            Some(step(pos, opposite(property(state, "facing")?)))
        } else {
            Some(step(pos, "down"))
        }
    }
}

/// A lever, or a button, which a control line may use instead.
pub struct Switch {
    pub pos: Pos,
    /// The block the switch is attached to, which it powers.
    pub block: Pos,
    /// The side of `block` the switch is on, such as `south` or `up`.
    pub side: String,
    pub on: bool,
    /// A button rather than a lever: it powers its block for a moment.
    pub button: bool,
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

/// A sign's block entity with the commas taken out of its text, as words on
/// a sign are separated by spaces and lines alone; any other entity as it
/// is. A line keeps its form: JSON text stays JSON, a plain string plain.
pub fn without_commas(entity: &Value) -> Value {
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
                *line = line_without_commas(line);
            }
        }
    }
    for key in ["Text1", "Text2", "Text3", "Text4"] {
        if let Some(line) = fields.get_mut(key) {
            *line = line_without_commas(line);
        }
    }
    entity
}

/// One sign line with its commas taken out, each run of commas and spaces
/// one space.
fn line_without_commas(line: &Value) -> Value {
    let plain = text(line);
    let words: Vec<&str> = plain
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|word| !word.is_empty())
        .collect();
    let cleaned = words.join(" ");
    if cleaned == plain {
        return line.clone();
    }
    match line {
        Value::String(raw) if raw.trim_start().starts_with(['"', '{']) => {
            Value::String(serde_json::to_string(&cleaned).unwrap_or_default())
        }
        Value::Compound(fields) => {
            let mut fields = fields.clone();
            fields.insert("text".to_owned(), Value::String(cleaned));
            Value::Compound(fields)
        }
        _ => Value::String(cleaned),
    }
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
