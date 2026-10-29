//! Which models a block state shows, as its blockstate file says: the
//! first variant whose properties all match, or every multipart case whose
//! condition holds.

use serde_json::Value;

/// One model a block state shows, turned as its blockstate file says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    /// The model's name, such as `minecraft:block/lever`.
    pub model: String,
    /// Quarter turns about the east-west axis, then about the vertical,
    /// in degrees.
    pub x: i32,
    pub y: i32,
    /// Whether textures keep their place in the world as the model turns.
    pub uvlock: bool,
}

/// A state such as `minecraft:lever[face=wall,facing=north]` split into its
/// id and its properties.
pub fn parse(state: &str) -> (&str, Vec<(&str, &str)>) {
    let Some((id, rest)) = state.split_once('[') else {
        return (state, Vec::new());
    };
    let properties = rest
        .trim_end_matches(']')
        .split(',')
        .filter_map(|pair| pair.split_once('='))
        .collect();
    (id, properties)
}

/// The models the blockstate file `file` shows for `properties`, or `None`
/// when it has neither variants nor multipart cases.
pub fn select(
    file: &Value,
    properties: &[(&str, &str)],
) -> Option<Vec<Placement>> {
    let value = |key: &str| {
        properties
            .iter()
            .find(|(k, _)| *k == key)
            .map_or("", |(_, v)| v)
    };
    if let Some(variants) = file.get("variants").and_then(Value::as_object) {
        // Keys are compared in the file's sorted order, so a state matching
        // two of them always takes the same one.
        let (_, chosen) = variants.iter().find(|(key, _)| {
            key.split(',').filter(|pair| !pair.is_empty()).all(
                |pair| match pair.split_once('=') {
                    Some((k, v)) => value(k) == v,
                    // A legacy key such as `normal` names no property.
                    None => true,
                },
            )
        })?;
        return placement(chosen).map(|p| vec![p]);
    }
    let cases = file.get("multipart").and_then(Value::as_array)?;
    Some(
        cases
            .iter()
            .filter(|case| {
                case.get("when").is_none_or(|when| holds(when, &value))
            })
            .filter_map(|case| placement(case.get("apply")?))
            .collect(),
    )
}

/// Whether a multipart `when` holds: every property it names has one of
/// its `|`-separated values, or, for `OR` and `AND`, any or all of its
/// conditions hold.
fn holds<'a>(when: &Value, value: &impl Fn(&str) -> &'a str) -> bool {
    let Some(conditions) = when.as_object() else {
        return false;
    };
    if let Some(any) = conditions.get("OR").and_then(Value::as_array) {
        return any.iter().any(|c| holds(c, value));
    }
    if let Some(all) = conditions.get("AND").and_then(Value::as_array) {
        return all.iter().all(|c| holds(c, value));
    }
    conditions.iter().all(|(key, allowed)| {
        let allowed = match allowed {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        allowed.split('|').any(|v| v == value(key))
    })
}

/// A variant or multipart `apply`: one model, or a list of weighted ones
/// of which the first is taken, so a picture never changes.
fn placement(value: &Value) -> Option<Placement> {
    let value = match value {
        Value::Array(choices) => choices.first()?,
        one => one,
    };
    let degrees = |key| {
        value
            .get(key)
            .and_then(Value::as_i64)
            .and_then(|d| i32::try_from(d).ok())
            .unwrap_or(0)
    };
    Some(Placement {
        model: value.get("model")?.as_str()?.to_owned(),
        x: degrees("x"),
        y: degrees("y"),
        uvlock: value
            .get("uvlock")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn models(file: &Value, state: &str) -> Vec<(String, i32, i32)> {
        let (_, properties) = parse(state);
        select(file, &properties)
            .unwrap()
            .into_iter()
            .map(|p| (p.model, p.x, p.y))
            .collect()
    }

    #[test]
    fn a_variant_matches_every_property_it_names() {
        let file = json!({ "variants": {
            "facing=east,lit=false": { "model": "torch_off" },
            "facing=north,lit=false": { "model": "torch_off", "y": 270 },
            "facing=north,lit=true": [{ "model": "torch_on", "y": 270 }, { "model": "other" }],
        }});
        assert_eq!(
            models(&file, "minecraft:wall_torch[facing=north,lit=true]"),
            [("torch_on".to_owned(), 0, 270)]
        );
        assert_eq!(
            models(&file, "minecraft:wall_torch[lit=false,facing=north]"),
            [("torch_off".to_owned(), 0, 270)]
        );
        let any = json!({ "variants": { "": { "model": "glass", "x": 90 } } });
        assert_eq!(
            models(&any, "minecraft:glass"),
            [("glass".to_owned(), 90, 0)]
        );
    }

    #[test]
    fn redstone_wire_takes_each_multipart_case_that_holds() {
        let file = json!({ "multipart": [
            { "apply": { "model": "dot" }, "when": { "OR": [
                { "east": "none", "north": "none", "south": "none", "west": "none" },
                { "east": "side|up", "north": "side|up" },
            ]}},
            { "apply": { "model": "side0" }, "when": { "north": "side|up" } },
            { "apply": { "model": "side_alt1", "y": 270 }, "when": { "east": "side|up" } },
            { "apply": { "model": "up", "y": 90 }, "when": { "east": "up" } },
            { "apply": { "model": "both" }, "when": { "AND": [{ "north": "side" }, { "south": "side" }] } },
        ]});
        let wire = |joins: &str| {
            models(&file, &format!("minecraft:redstone_wire[{joins},power=0]"))
                .into_iter()
                .map(|(model, _, y)| format!("{model}@{y}"))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            wire("east=none,north=none,south=none,west=none"),
            ["dot@0"]
        );
        assert_eq!(
            wire("east=up,north=side,south=none,west=none"),
            ["dot@0", "side0@0", "side_alt1@270", "up@90"]
        );
        assert_eq!(
            wire("east=none,north=side,south=none,west=none"),
            ["side0@0"]
        );
        assert_eq!(
            wire("east=none,north=side,south=side,west=none"),
            ["side0@0", "both@0"]
        );
    }
}
