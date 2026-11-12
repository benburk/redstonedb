//! The `.schem` file itself: unpacking its gzipped NBT, writing it back
//! laid out as it came, with every compound's fields in name order, and
//! changing the blocks it holds and where `//paste` puts them.

use std::{
    collections::HashMap,
    hash::BuildHasher,
    io::{Read, Write},
};

use anyhow::{Context, Result, bail};
use fastnbt::Value;
use fastnbt::{ByteArray, IntArray};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};

use super::{AIR, Pos, blocks::Blocks};

/// A build's size, in blocks along x, y, z.
pub struct Size {
    pub x: u16,
    pub y: u16,
    pub z: u16,
}

/// A schematic's NBT, kept whole so it can be changed and written back.
#[derive(Clone)]
pub struct Nbt {
    pub body: HashMap<String, Value>,
    /// Version 3 nests the fields in a `Schematic` compound; version 2
    /// keeps them at the root.
    nested: bool,
}

/// `data` set to paste from one block south of its south-east-bottom
/// corner; see `Nbt::place_for_paste`.
pub fn paste_from_corner(data: &[u8]) -> Result<Vec<u8>> {
    Blocks::read(data)?.pasting_from_corner()?.encode()
}

impl Blocks {
    /// The blocks set to paste from one block south of their
    /// south-east-bottom corner; see `Nbt::place_for_paste`.
    pub fn pasting_from_corner(&self) -> Result<Blocks> {
        let mut nbt = self.nbt()?;
        nbt.place_for_paste(self.size);
        Blocks::of(nbt)
    }
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
pub(super) fn dimension(
    body: &HashMap<String, Value>,
    key: &str,
) -> Result<u16> {
    match body.get(key) {
        Some(Value::Short(n)) => Ok(n.cast_unsigned()),
        _ => bail!("schematic has no {key}"),
    }
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

pub(super) fn varints(bytes: &[i8]) -> Vec<usize> {
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
        // A role goes with its block, which the caller has put in the new
        // box with its roles.
        if let Some(roles) = self.roles() {
            let kept =
                roles.moved(|pos| grid.contains_key(&pos).then_some(pos));
            self.set_roles(Some(&kept));
        }
        Ok(())
    }

    /// Moves the roles the file carries by `to`, as its blocks move, those
    /// it sends nowhere left out.
    pub fn move_roles(&mut self, to: impl Fn(Pos) -> Option<Pos>) {
        if let Some(roles) = self.roles() {
            self.set_roles(Some(&roles.moved(to)));
        }
    }

    /// The roles the file carries while it is worked on, if any; see
    /// [`super::lines`].
    pub fn roles(&self) -> Option<super::lines::Roles> {
        let Some(Value::Compound(metadata)) = self.body.get("Metadata") else {
            return None;
        };
        let Some(Value::String(json)) = metadata.get(super::lines::KEY) else {
            return None;
        };
        serde_json::from_str(json).ok()
    }

    /// Makes the file carry `roles`, or none with `None`.
    pub fn set_roles(&mut self, roles: Option<&super::lines::Roles>) {
        let json = roles.map(|roles| {
            serde_json::to_string(roles).expect("positions and roles are JSON")
        });
        match json {
            Some(json) => {
                self.metadata()
                    .insert(super::lines::KEY.to_owned(), Value::String(json));
            }
            None => {
                if let Some(Value::Compound(metadata)) =
                    self.body.get_mut("Metadata")
                {
                    metadata.remove(super::lines::KEY);
                }
            }
        }
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
