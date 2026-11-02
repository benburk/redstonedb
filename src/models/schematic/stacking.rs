//! The ways a stackable build repeats: the directions its copies sit
//! along, the box within it later copies repeat, and the wider builds its
//! copies make.

use super::Orientation;

/// One direction a stackable build repeats along. Each copy sits `west`,
/// `up`, and `north` blocks from the one before, since copies count up from
/// the south-east-bottom corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackDirection {
    pub west: i64,
    pub up: i64,
    pub north: i64,
    /// The most copies that work; `None` for no limit.
    pub max_copies: Option<i64>,
    /// The box later copies repeat, when testing copies chose it; `None`
    /// for the one the box rule finds. See `stack`.
    pub faces: Option<Faces>,
}

/// A box within a build: how many of its layers it leaves out on each
/// face, `low` on the west, bottom, and north, `high` on the east, top,
/// and south, as in the file, whose x counts east, y up, and z south.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct Faces {
    pub low: [u8; 3],
    pub high: [u8; 3],
}

impl Faces {
    /// The box's least and greatest corner within a build of `size`.
    pub fn corners(self, size: [i32; 3]) -> [[i32; 3]; 2] {
        [
            self.low.map(i32::from),
            [0, 1, 2].map(|axis| size[axis] - 1 - i32::from(self.high[axis])),
        ]
    }

    /// The box with corners `corners` within a build of `size`.
    pub fn of(corners: [[i32; 3]; 2], size: [i32; 3]) -> Option<Self> {
        let layers = |n: i32| u8::try_from(n).ok();
        let low = corners[0].map(layers);
        let high =
            [0, 1, 2].map(|axis| layers(size[axis] - 1 - corners[1][axis]));
        Some(Faces {
            low: [low[0]?, low[1]?, low[2]?],
            high: [high[0]?, high[1]?, high[2]?],
        })
    }

    /// Such as `the bottom 2 layers and the north 2`, naming each face it
    /// leaves layers out on.
    pub fn describe(self) -> String {
        let named = [
            (self.low[0], "west"),
            (self.high[0], "east"),
            (self.low[1], "bottom"),
            (self.high[1], "top"),
            (self.low[2], "north"),
            (self.high[2], "south"),
        ];
        let parts: Vec<String> = named
            .into_iter()
            .filter(|&(n, _)| n > 0)
            .map(|(n, face)| match n {
                1 => format!("its {face} layer"),
                n => format!("its {n} {face} layers"),
            })
            .collect();
        match parts.as_slice() {
            [] => "nothing".to_owned(),
            [one] => one.clone(),
            [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
        }
    }
}

impl StackDirection {
    fn parts(&self) -> impl Iterator<Item = (i64, &'static str)> {
        [(self.west, "west"), (self.up, "up"), (self.north, "north")]
            .into_iter()
            .filter(|(n, _)| *n > 0)
    }

    /// The way copies go, such as `west` or `west-up`. It names the
    /// direction in a download link.
    pub fn name(&self) -> String {
        self.parts()
            .map(|(_, way)| way)
            .collect::<Vec<_>>()
            .join("-")
    }

    /// Such as `2 west per copy, up to 7 copies`.
    pub fn describe(&self) -> String {
        let steps: Vec<String> =
            self.parts().map(|(n, way)| format!("{n} {way}")).collect();
        let limit = match self.max_copies {
            Some(n) => format!("up to {n} copies"),
            None => "any number of copies".to_owned(),
        };
        format!("{} per copy, {limit}", steps.join(" and "))
    }

    /// The way the bits of a build run whose copies this way add bits: west
    /// alone is horizontal, up alone vertical, and west and up diagonal.
    /// `None` for any other way.
    pub fn orientation(&self) -> Option<Orientation> {
        match (self.west > 0, self.up > 0, self.north > 0) {
            (true, false, false) => Some(Orientation::Horizontal),
            (false, true, false) => Some(Orientation::Vertical),
            (true, true, false) => Some(Orientation::Diagonal),
            _ => None,
        }
    }

    /// Whether copies this way add bits, as along the way the build's bits
    /// already run, rather than add parallel copies such as more registers.
    pub fn adds_bits(&self, orientation: Orientation) -> bool {
        self.orientation() == Some(orientation)
    }
}

/// A stackable build stacked into a wider one: `copies` of it make a build
/// `width` bits wide, whose timing was measured so, as it competes on that
/// width's leaderboard; `None` for what could not be measured.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize,
)]
pub struct Widened {
    pub width: i64,
    pub copies: i64,
    pub latency: Option<i64>,
    pub synchronized: Option<bool>,
    pub throughput: Option<i64>,
}

impl std::str::FromStr for StackDirection {
    type Err = String;

    /// Parses `west=2`, `north=4,max=3`, or `west=1,up=1`.
    fn from_str(text: &str) -> Result<Self, String> {
        let mut direction = StackDirection {
            west: 0,
            up: 0,
            north: 0,
            max_copies: None,
            faces: None,
        };
        for pair in text.split(',') {
            let (key, value) = pair
                .split_once('=')
                .ok_or_else(|| format!("expected key=value, got `{pair}`"))?;
            let value: i64 = value
                .trim()
                .parse()
                .map_err(|_| format!("`{value}` is not a whole number"))?;
            match key.trim() {
                "west" => direction.west = value,
                "up" => direction.up = value,
                "north" => direction.north = value,
                "max" => direction.max_copies = Some(value),
                other => {
                    return Err(format!(
                        "unknown key `{other}`; use west, up, north, or max"
                    ));
                }
            }
        }
        if direction.west + direction.up + direction.north <= 0 {
            return Err("give a step west, up, or north".to_owned());
        }
        Ok(direction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_add_bits_along_the_way_the_bits_run() {
        let way = |west, up, north| StackDirection {
            west,
            up,
            north,
            max_copies: None,
            faces: None,
        };
        assert!(way(2, 0, 0).adds_bits(Orientation::Horizontal));
        assert!(way(0, 2, 0).adds_bits(Orientation::Vertical));
        assert!(way(1, 1, 0).adds_bits(Orientation::Diagonal));
        assert!(!way(0, 0, 3).adds_bits(Orientation::Horizontal));
        assert!(!way(2, 0, 0).adds_bits(Orientation::Vertical));
    }
}
