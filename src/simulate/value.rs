//! The numbers a test drives and reads, and how a stored report keeps them.

use std::fmt;

use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, Visitor},
};

/// A value a test drives or reads: an input's, an output's, or a bench's
/// outputs read together as one. It holds a 64-bit build's operands, their
/// sum with its carry, and a 64-bit hex adder's sum and carry out read as
/// one, 65 bits.
pub type Value = u128;

/// A value as a report stores it: a JSON number while JavaScript reads it
/// exactly, up to `EXACT`, as every report stored before values were wider
/// did, and a string of its digits past that, which neither JavaScript nor
/// a report, read through its outcome's tag, would read as a number
/// without losing its lowest bits. Either reads back.
#[derive(Clone, Copy)]
struct Stored(Value);

/// The largest integer a JavaScript number holds exactly, 2^53 - 1.
const EXACT: Value = (1 << 53) - 1;

impl Serialize for Stored {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match u64::try_from(self.0) {
            Ok(narrow) if self.0 <= EXACT => s.serialize_u64(narrow),
            _ => s.serialize_str(&self.0.to_string()),
        }
    }
}

impl<'de> Deserialize<'de> for Stored {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(Number).map(Stored)
    }
}

/// Visits a value as a number or a string of digits.
struct Number;

impl Visitor<'_> for Number {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a non-negative integer, or one as a string")
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
        Ok(v.into())
    }

    fn visit_u128<E: de::Error>(self, v: u128) -> Result<Value, E> {
        Ok(v)
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> {
        Value::try_from(v).map_err(E::custom)
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
        v.parse().map_err(E::custom)
    }
}

/// Keeps one value as [`Stored`] does, for `#[serde(with = "value")]`.
pub fn serialize<S: Serializer>(
    value: &Value,
    s: S,
) -> Result<S::Ok, S::Error> {
    Stored(*value).serialize(s)
}

pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Value, D::Error> {
    Stored::deserialize(d).map(|stored| stored.0)
}

/// Values in order, each kept as [`Stored`] keeps one.
pub mod many {
    use super::{Deserialize, Deserializer, Serializer, Stored, Value};

    pub fn serialize<S: Serializer>(
        values: &[Value],
        s: S,
    ) -> Result<S::Ok, S::Error> {
        s.collect_seq(values.iter().map(|&value| Stored(value)))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Vec<Value>, D::Error> {
        let stored = Vec::<Stored>::deserialize(d)?;
        Ok(stored.into_iter().map(|value| value.0).collect())
    }

    /// Values each from a tick, in order, each value kept as [`Stored`]
    /// keeps one.
    pub mod ticked {
        use super::{Deserialize, Deserializer, Serializer, Stored, Value};

        pub fn serialize<S: Serializer>(
            steps: &[(i32, Value)],
            s: S,
        ) -> Result<S::Ok, S::Error> {
            s.collect_seq(
                steps.iter().map(|&(tick, value)| (tick, Stored(value))),
            )
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(
            d: D,
        ) -> Result<Vec<(i32, Value)>, D::Error> {
            let stored = Vec::<(i32, Stored)>::deserialize(d)?;
            Ok(stored
                .into_iter()
                .map(|(tick, value)| (tick, value.0))
                .collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::Value;

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    #[serde(tag = "outcome")]
    enum Tagged {
        Ran {
            #[serde(with = "super")]
            one: Value,
            #[serde(with = "super::many")]
            many: Vec<Value>,
            #[serde(with = "super::many::ticked")]
            steps: Vec<(i32, Value)>,
        },
    }

    #[test]
    fn a_value_past_what_javascript_holds_is_kept_as_a_string() {
        let wide = Tagged::Ran {
            one: 1 << 64,
            many: vec![5, (1 << 53) - 1, 1 << 53, Value::MAX],
            steps: vec![(3, 1 << 70)],
        };
        let json = serde_json::to_string(&wide).unwrap();
        assert_eq!(
            json,
            r#"{"outcome":"Ran","one":"18446744073709551616","many":[5,9007199254740991,"9007199254740992","340282366920938463463374607431768211455"],"steps":[[3,"1180591620717411303424"]]}"#
        );
        assert_eq!(serde_json::from_str::<Tagged>(&json).unwrap(), wide);
    }

    #[test]
    fn a_report_stored_as_numbers_reads_as_before() {
        let json =
            r#"{"outcome":"Ran","one":7,"many":[1,2,3],"steps":[[0,4]]}"#;
        assert_eq!(
            serde_json::from_str::<Tagged>(json).unwrap(),
            Tagged::Ran {
                one: 7,
                many: vec![1, 2, 3],
                steps: vec![(0, 4)],
            }
        );
    }
}
