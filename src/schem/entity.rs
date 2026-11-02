//! Block entities: the signal each comparator was saved putting out, what
//! each container holds and the strength a comparator reads from it, and
//! each sign's text, read from whatever form its lines take and written
//! back plain.

use std::collections::HashMap;

use fastnbt::{IntArray, Value};
use mchprs_blocks::items::Item;

use super::{Pos, blocks::id};

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

pub(super) fn comparator(value: &Value) -> Option<(Pos, u8)> {
    let (pos, fields) = entity(value, "comparator")?;
    match fields.get("OutputSignal") {
        Some(Value::Int(n)) => Some((pos, u8::try_from(*n).ok()?)),
        _ => None,
    }
}

pub(super) fn container(value: &Value) -> Option<(Pos, Vec<(String, u32)>)> {
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
        .map(|(id, count)| f64::from(*count) / f64::from(max_stack(id)))
        .sum();
    let strength = 1.0 + full / f64::from(slots) * 14.0;
    // At most 15, as a container holds at most its slots' worth.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let strength = strength.floor().clamp(0.0, 15.0) as u8;
    strength
}

/// How many of item `id` stack in a slot, as the game has it: as the
/// simulator's own list says, and where it lacks the item, 1 for a tool,
/// weapon, armour, or anything else that never stacks, as a builder may
/// fill a barrel with wooden shovels, 16 for one that stacks only so far,
/// such as an ender pearl or a sign, and 64 for the rest.
fn max_stack(id: &str) -> u32 {
    const ALONE_ENDINGS: [&str; 17] = [
        "_shovel",
        "_pickaxe",
        "_axe",
        "_hoe",
        "_sword",
        "_helmet",
        "_chestplate",
        "_leggings",
        "_boots",
        "_horse_armor",
        "_bucket",
        "_boat",
        "_raft",
        "_minecart",
        "_shulker_box",
        "_bed",
        "_stew",
    ];
    const ALONE: [&str; 26] = [
        "bow",
        "crossbow",
        "trident",
        "shield",
        "elytra",
        "mace",
        "totem_of_undying",
        "potion",
        "splash_potion",
        "lingering_potion",
        "saddle",
        "flint_and_steel",
        "shears",
        "fishing_rod",
        "carrot_on_a_stick",
        "warped_fungus_on_a_stick",
        "enchanted_book",
        "written_book",
        "writable_book",
        "cake",
        "minecart",
        "brush",
        "spyglass",
        "bundle",
        "goat_horn",
        "beetroot_soup",
    ];
    const SIXTEEN_ENDINGS: [&str; 3] = ["_sign", "_banner", "_egg"];
    const SIXTEEN: [&str; 6] = [
        "ender_pearl",
        "snowball",
        "egg",
        "bucket",
        "honey_bottle",
        "armor_stand",
    ];
    if let Some(item) = Item::from_name(id) {
        return item.max_stack_size();
    }
    let name = id.strip_prefix("minecraft:").unwrap_or(id);
    let ends = |endings: &[&str]| endings.iter().any(|e| name.ends_with(e));
    if ALONE.contains(&name)
        || name.starts_with("music_disc_")
        || ends(&ALONE_ENDINGS)
    {
        1
    } else if SIXTEEN.contains(&name)
        || ends(&SIXTEEN_ENDINGS) && !name.ends_with("_spawn_egg")
    {
        16
    } else {
        64
    }
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

pub(super) fn sign(value: &Value) -> Option<(Pos, Vec<String>)> {
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

/// A comparator's block entity putting out `strength`; any other entity
/// as it is.
pub fn with_output_signal(entity: &Value, strength: u8) -> Value {
    let mut entity = entity.clone();
    let Value::Compound(outer) = &mut entity else {
        return entity;
    };
    if !matches!(outer.get("Id"), Some(Value::String(id)) if id.ends_with("comparator"))
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
    fields.insert("OutputSignal".to_owned(), Value::Int(i32::from(strength)));
    entity
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
    fn a_barrel_of_things_that_never_stack_counts_each_as_a_full_slot() {
        let shovels = vec![("minecraft:wooden_shovel".to_owned(), 1); 2];
        assert_eq!(container_strength(&shovels, 27), 2);
        let pearls = vec![("minecraft:ender_pearl".to_owned(), 16); 2];
        assert_eq!(container_strength(&pearls, 27), 2);
        assert_eq!(max_stack("minecraft:redstone"), 64);
        assert_eq!(max_stack("minecraft:pig_spawn_egg"), 64);
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
}
