//! The buses a slot may take from a build's lines: whole runs, as the
//! conventions lay a bus, or a run but its top bit, a carry out above a
//! sum; or, at a cost, a run's end, a run's alternate bits, two parallel
//! runs' bits alternating, a run and a stray bit beside its end, or every
//! bit left, in order along the way they spread, as builds that bend them
//! lay them, as a staircase of short runs. A bus's bits are always in order along their runs, one way or
//! the other, never shuffled, so a run of `n` bits is read only two ways.
//! Each bus costs more the further it strays from the conventions, and the
//! cheapest reading that holds wins a tie.

use crate::{schem::Pos, simulate::Value};

/// A bit of a build's lines: its run, by its place among the runs, and its
/// place along that run.
pub type BitAt = (usize, usize);

/// A bus a slot takes, its bits least significant first, and how far it
/// strays from the conventions: 1 for counted the other way round from the
/// site's count, and for a run but its top bit; 2 for any other run it is
/// not the whole of, but for a lone line taken from a row of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bus {
    pub bits: Vec<BitAt>,
    pub cost: u32,
    /// Whether it bends the conventions, as a run's alternate bits or a
    /// stray bit do, rather than only counting the other way round.
    pub bent: bool,
}

impl Bus {
    /// Its value in a case whose runs were `runs`, each run's bits its
    /// value's, the first lowest.
    pub fn value(&self, runs: &[Value]) -> Value {
        self.bits
            .iter()
            .enumerate()
            .fold(0, |value, (k, &(run, bit))| {
                value | (runs[run] >> bit & 1) << k
            })
    }
}

/// The bits of a build's lines no slot has taken yet, as stretches of its
/// runs, each in its run's order.
#[derive(Clone, Debug, Default)]
pub struct Pool {
    stretches: Vec<Vec<BitAt>>,
    /// Where each run's bits are, whatever is left of it.
    runs: Vec<Vec<Pos>>,
}

/// Whether bits, least significant first, are counted the way the site
/// counts a bus.
pub type Conventional<'c> = &'c dyn Fn(&[BitAt]) -> bool;

/// What a bus that bends the conventions costs.
const BENT: u32 = 2;

/// The farthest a stray bit may be from the end of the run it joins, in
/// blocks along each axis together.
const STRAY_REACH: i32 = 3;

impl Pool {
    /// Every bit of `runs`, each run a stretch.
    pub fn of(runs: &[Vec<Pos>]) -> Pool {
        Pool {
            stretches: runs
                .iter()
                .enumerate()
                .map(|(run, bits)| {
                    (0..bits.len()).map(|bit| (run, bit)).collect()
                })
                .collect(),
            runs: runs.to_vec(),
        }
    }

    /// How many bits are left.
    pub fn bits(&self) -> usize {
        self.stretches.iter().map(Vec::len).sum()
    }

    /// The pool with the stretches `taken` taken out and `left` put in.
    fn without(&self, taken: &[usize], left: Vec<Vec<BitAt>>) -> Pool {
        let mut stretches: Vec<Vec<BitAt>> = self
            .stretches
            .iter()
            .enumerate()
            .filter(|(i, _)| !taken.contains(i))
            .map(|(_, stretch)| stretch.clone())
            .collect();
        stretches.extend(left.into_iter().filter(|s| !s.is_empty()));
        Pool {
            stretches,
            runs: self.runs.clone(),
        }
    }

    /// Every bit left, as one bus, in the pool's order, and the pool
    /// emptied: a selector's lines, whose order nothing depends on. Only
    /// whole runs narrower than `n` bits may be one, so no operand's bits
    /// are taken for control lines; `None` when any other is left.
    pub fn rest(&self, n: usize) -> Option<(Bus, Pool)> {
        let whole = self.stretches.iter().all(|stretch| {
            stretch.len() == self.runs[stretch[0].0].len() && stretch.len() < n
        });
        let bus = Bus {
            bits: self.stretches.concat(),
            cost: 0,
            bent: false,
        };
        whole.then(|| {
            (
                bus,
                Pool {
                    stretches: Vec::new(),
                    runs: self.runs.clone(),
                },
            )
        })
    }

    /// Every bus of `width` bits that can be taken from the pool, each
    /// with the pool left after, and each either way round, the way
    /// `conventional` says the site counts costing nothing more: a whole
    /// stretch, a lone line from one, or either end of one a bit longer;
    /// and when `bent`, either end of a longer one, the alternate bits of
    /// one twice as long, the bits of two stretches half as long,
    /// alternating, a whole run a bit shorter and a bit beside one of its
    /// ends, joined there, as when a build's top bits stand out of line, or
    /// every bit left, when they are `width` bits, in order along the way
    /// they spread furthest, no two level on it, as a staircase of short
    /// runs lays one bus.
    pub fn take(
        &self,
        width: usize,
        conventional: Conventional,
        bent: bool,
    ) -> Vec<(Bus, Pool)> {
        let mut found: Vec<(Vec<BitAt>, u32, Pool)> = Vec::new();
        for (i, stretch) in self.stretches.iter().enumerate() {
            let len = stretch.len();
            if len == width {
                found.push((
                    stretch.clone(),
                    0,
                    self.without(&[i], Vec::new()),
                ));
            }
            // A lone line from a row is as plain as any, and a carry out
            // above a sum nearly so.
            if len > width && (bent || width == 1 || len == width + 1) {
                let bent = match () {
                    () if width == 1 => 0,
                    () if len == width + 1 => 1,
                    () => BENT,
                };
                let (low, high) = stretch.split_at(width);
                found.push((
                    low.to_vec(),
                    bent,
                    self.without(&[i], vec![high.to_vec()]),
                ));
                let (low, high) = stretch.split_at(len - width);
                found.push((
                    high.to_vec(),
                    bent,
                    self.without(&[i], vec![low.to_vec()]),
                ));
            }
            if bent && width > 1 && len == 2 * width {
                let even: Vec<BitAt> =
                    stretch.iter().step_by(2).copied().collect();
                let odd: Vec<BitAt> =
                    stretch.iter().skip(1).step_by(2).copied().collect();
                found.push((
                    even.clone(),
                    BENT,
                    self.without(&[i], vec![odd.clone()]),
                ));
                found.push((odd, BENT, self.without(&[i], vec![even])));
            }
        }
        if bent && width > 1 && width.is_multiple_of(2) {
            for (i, x) in self.stretches.iter().enumerate() {
                for (j, y) in self.stretches.iter().enumerate() {
                    if i == j || x.len() != width / 2 || y.len() != width / 2 {
                        continue;
                    }
                    let bits =
                        x.iter().zip(y).flat_map(|(a, b)| [*a, *b]).collect();
                    found.push((bits, BENT, self.without(&[i, j], Vec::new())));
                }
            }
        }
        if bent && width > 1 {
            found.extend(self.strays(width));
            found.extend(self.spread(width));
        }
        let mut buses: Vec<(Bus, Pool)> = Vec::new();
        for (bits, cost, rest) in found {
            let mut back = bits.clone();
            back.reverse();
            let ways = if width == 1 {
                vec![bits]
            } else {
                vec![bits, back]
            };
            for bits in ways {
                let bus = Bus {
                    cost: cost + u32::from(!conventional(&bits)),
                    bent: cost >= BENT,
                    bits,
                };
                if !buses.iter().any(|(b, _)| b.bits == bus.bits) {
                    buses.push((bus, rest.clone()));
                }
            }
        }
        buses
    }
}

impl Pool {
    /// Where bit `at` is.
    fn at(&self, (run, bit): BitAt) -> Pos {
        self.runs[run][bit]
    }

    /// Every bit left as one bus of `width` bits, in order along the axis
    /// they spread furthest, the pool emptied; none when they are more or
    /// fewer, already one stretch, or two are level on that axis.
    fn spread(&self, width: usize) -> Option<(Vec<BitAt>, u32, Pool)> {
        if self.stretches.len() < 2 || self.bits() != width {
            return None;
        }
        let mut bits = self.stretches.concat();
        let extent = |axis: usize| {
            let along = bits.iter().map(|&bit| self.at(bit)[axis]);
            along.clone().max().unwrap_or(0) - along.min().unwrap_or(0)
        };
        let axis = (0..3).max_by_key(|&axis| extent(axis))?;
        bits.sort_by_key(|&bit| self.at(bit)[axis]);
        if bits
            .windows(2)
            .any(|w| self.at(w[0])[axis] == self.at(w[1])[axis])
        {
            return None;
        }
        let rest = Pool {
            stretches: Vec::new(),
            runs: self.runs.clone(),
        };
        Some((bits, BENT, rest))
    }

    /// Each bus of `width` bits that is a whole run one shorter and a bit
    /// of another stretch, an end of it or any bit of a lone line or a
    /// short run, within [`STRAY_REACH`] of one of the run's ends, joined
    /// at that end, with the pool left after.
    fn strays(&self, width: usize) -> Vec<(Vec<BitAt>, u32, Pool)> {
        let mut found = Vec::new();
        let apart =
            |x: Pos, y: Pos| (0..3).map(|i| (x[i] - y[i]).abs()).sum::<i32>();
        for (i, run) in self.stretches.iter().enumerate() {
            let whole = run.len() == self.runs[run[0].0].len();
            if !whole || run.len() + 1 != width {
                continue;
            }
            let (first, last) = (self.at(run[0]), self.at(run[run.len() - 1]));
            for (j, other) in self.stretches.iter().enumerate() {
                if i == j {
                    continue;
                }
                let short = other.len() <= 3;
                for (k, &stray) in other.iter().enumerate() {
                    if !short && k != 0 && k + 1 != other.len() {
                        continue;
                    }
                    let (low, high) = (
                        apart(self.at(stray), first),
                        apart(self.at(stray), last),
                    );
                    if low.min(high) > STRAY_REACH {
                        continue;
                    }
                    let mut bits = run.clone();
                    if high <= low {
                        bits.push(stray);
                    } else {
                        bits.insert(0, stray);
                    }
                    let left =
                        vec![other[..k].to_vec(), other[k + 1..].to_vec()];
                    found.push((bits, BENT, self.without(&[i, j], left)));
                }
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A run of `len` bits up the column at `x`, every other block.
    fn column(x: i32, len: usize) -> Vec<Pos> {
        (0..len)
            .map(|i| [x, 2 * i32::try_from(i).unwrap(), 0])
            .collect()
    }

    fn widths(pool: &Pool, width: usize) -> Vec<(Vec<BitAt>, u32)> {
        pool.take(width, &|bits| bits.first().is_none_or(|b| b.1 == 0), true)
            .into_iter()
            .map(|(bus, _)| (bus.bits, bus.cost))
            .collect()
    }

    /// A run is read whole either way round, the site's count costing
    /// nothing, and never shuffled.
    #[test]
    fn a_run_is_a_bus_either_way_round() {
        let pool = Pool::of(&[column(0, 3)]);
        assert_eq!(
            widths(&pool, 3),
            [
                (vec![(0, 0), (0, 1), (0, 2)], 0),
                (vec![(0, 2), (0, 1), (0, 0)], 1),
            ]
        );
    }

    /// A run one longer than a bus gives the bus either end, the bit left
    /// over another slot's, as a sum with its carry out at its top, as
    /// plainly as nearly any run.
    #[test]
    fn a_longer_run_gives_up_an_end() {
        let pool = Pool::of(&[column(0, 3)]);
        let found = pool.take(2, &|_| true, true);
        let bits: Vec<Vec<BitAt>> =
            found.iter().map(|(bus, _)| bus.bits.clone()).collect();
        assert!(bits.contains(&vec![(0, 0), (0, 1)]));
        assert!(bits.contains(&vec![(0, 1), (0, 2)]));
        assert!(
            found
                .iter()
                .all(|(bus, rest)| bus.cost < BENT && rest.bits() == 1)
        );
    }

    /// Two runs of 7 and their top bits standing out of line beside them,
    /// read as a run of their own: each run takes the stray beside its
    /// end, never the other's.
    #[test]
    fn a_stray_bit_joins_the_run_beside_it() {
        let strays = vec![[0, 14, 1], [3, 14, 1]];
        let pool = Pool::of(&[column(0, 7), column(3, 7), strays]);
        let a: Vec<Vec<BitAt>> = pool
            .take(8, &|_| true, true)
            .into_iter()
            .map(|(bus, _)| bus.bits)
            .filter(|bits| bits[0] == (0, 0) || bits[0] == (2, 0))
            .collect();
        assert!(
            a.contains(
                &[
                    (0, 0),
                    (0, 1),
                    (0, 2),
                    (0, 3),
                    (0, 4),
                    (0, 5),
                    (0, 6),
                    (2, 0)
                ]
                .to_vec()
            )
        );
        assert!(
            a.iter()
                .all(|bits| !bits.contains(&(2, 1)) || bits.contains(&(1, 0)))
        );
        assert!(pool.take(8, &|_| true, false).is_empty());
    }

    /// A staircase of short runs, each a step up and over from the one
    /// before, is one bus, in order up the stairs.
    #[test]
    fn a_staircase_of_short_runs_is_one_bus() {
        let pool = Pool::of(&[
            vec![[2, 1, 0], [4, 3, 0]],
            vec![[5, 6, 1], [5, 8, 1]],
            vec![[7, 10, 1], [7, 12, 1]],
        ]);
        let bits: Vec<Vec<BitAt>> = pool
            .take(6, &|_| true, true)
            .into_iter()
            .map(|(bus, _)| bus.bits)
            .collect();
        assert!(bits.contains(&vec![
            (0, 0),
            (0, 1),
            (1, 0),
            (1, 1),
            (2, 0),
            (2, 1)
        ]));
        assert!(pool.take(6, &|_| true, false).is_empty());
    }

    /// Two parallel runs make one bus, their bits alternating.
    #[test]
    fn two_runs_interleave() {
        let pool = Pool::of(&[column(0, 2), column(1, 2)]);
        let bits: Vec<Vec<BitAt>> = pool
            .take(4, &|_| true, true)
            .into_iter()
            .map(|(bus, _)| bus.bits)
            .collect();
        assert!(bits.contains(&vec![(0, 0), (1, 0), (0, 1), (1, 1)]));
        assert!(bits.contains(&vec![(1, 0), (0, 0), (1, 1), (0, 1)]));
    }

    #[test]
    fn a_bus_reads_its_bits_value() {
        let bus = Bus {
            bits: vec![(1, 0), (0, 1)],
            cost: 0,
            bent: false,
        };
        assert_eq!(bus.value(&[0b10, 0b0]), 0b10);
        assert_eq!(bus.value(&[0b00, 0b1]), 0b01);
    }
}
