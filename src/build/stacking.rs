//! What a stackable build's copies make: the blocks each adds, and the
//! wider builds they stack into.

use super::{Build, File, Specs};
use crate::{category::Measure, stack::StackDirection};

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

impl Build {
    /// The blocks a build takes, as it is compared: for a stackable build,
    /// what each copy adds, since its copies may overlap; else its whole
    /// box.
    pub fn volume(&self) -> i64 {
        let (x, y, z) = self.per_copy().unwrap_or(self.file.size);
        x * y * z
    }

    /// The number of blocks inside the build's bounding box.
    pub fn box_volume(&self) -> i64 {
        let (x, y, z) = self.file.size;
        x * y * z
    }

    /// The box a stack grows by with each copy: the build's size, but its
    /// step along each way it stacks, so the parts copies overlap on count
    /// once. `None` for a build that does not stack, or stacks diagonally,
    /// whose stack does not grow by a box.
    pub fn per_copy(&self) -> Option<(i64, i64, i64)> {
        if self.stacking.is_empty() {
            return None;
        }
        let (mut x, mut y, z) = self.file.size;
        for d in &self.stacking {
            match (d.west, d.up) {
                (west, 0) => x = west,
                (0, up) => y = up,
                _ => return None,
            }
        }
        Some((x, y, z))
    }

    /// This build stacked into a box of inner `size`, with `copies` along
    /// each direction. Copies that add bits widen it, and every copy of a
    /// memory holds its bytes. A stack's latency and timing are not its
    /// slice's, so they are unknown, as is the throughput of any but a serial
    /// build.
    #[must_use]
    pub fn stacked(
        &self,
        copies: &[(StackDirection, usize)],
        size: [i32; 3],
    ) -> Build {
        let count = |n: &usize| i64::try_from(*n).unwrap_or(i64::MAX);
        let factor: i64 = copies
            .iter()
            .filter(|(direction, _)| {
                direction.adds_bits(self.specs.orientation)
            })
            .map(|(_, n)| count(n))
            .product();
        let all: i64 = copies.iter().map(|(_, n)| count(n)).product();
        let specs = &self.specs;
        Build {
            specs: Specs {
                width: specs.width.map(|w| w * factor),
                bytes: specs.bytes.map(|b| b.saturating_mul(all)),
                latency: None,
                synchronized: None,
                throughput: specs
                    .throughput
                    .filter(|_| self.has(Measure::Throughput)),
                ..specs.clone()
            },
            file: File {
                size: (size[0].into(), size[1].into(), size[2].into()),
                ..self.file.clone()
            },
            ..self.clone()
        }
    }

    /// Copies of this build stacked into the wider build `w`, as a
    /// leaderboard ranks it among builds of `w`'s width: with the timing
    /// measured so, and the box the copies take, the step its bits run
    /// along counting once a copy. `None` for a build with no way of
    /// stacking that adds bits.
    #[must_use]
    pub fn widened_to(&self, w: &Widened) -> Option<Build> {
        let along = *self
            .stacking
            .iter()
            .find(|d| d.adds_bits(self.specs.orientation))?;
        let more = w.copies - 1;
        let (x, y, z) = self.file.size;
        let size = (x + along.west * more, y + along.up * more, z);
        let stacking = self
            .stacking
            .iter()
            .map(|&d| {
                if d == along {
                    StackDirection {
                        west: d.west * w.copies,
                        up: d.up * w.copies,
                        max_copies: None,
                        ..d
                    }
                } else {
                    d
                }
            })
            .collect();
        Some(Build {
            specs: Specs {
                width: Some(w.width),
                latency: w.latency,
                synchronized: w.synchronized,
                throughput: w.throughput,
                ..self.specs.clone()
            },
            file: File {
                size,
                ..self.file.clone()
            },
            stacking,
            widened: Vec::new(),
            stacked_from: self.specs.width,
            ..self.clone()
        })
    }
}
