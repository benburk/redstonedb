//! The buses a slot may take from a build's lines: whole runs, as the
//! conventions lay a bus, or, at a cost, a run's end, a run's alternate
//! bits, or two parallel runs' bits alternating, as builds that bend them
//! lay them. A bus's bits are always in order along their runs, one way or
//! the other, never shuffled, so a run of `n` bits is read only two ways.
//! Each bus costs more the further it strays from the conventions, and the
//! cheapest reading that holds wins a tie.

use crate::simulate::Value;

/// A bit of a build's lines: its run, by its place among the runs, and its
/// place along that run.
pub type BitAt = (usize, usize);

/// A bus a slot takes, its bits least significant first, and how far it
/// strays from the conventions: 1 for counted the other way round from the
/// site's count, 2 for any run it is not the whole of, but for a lone line
/// taken from a row of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bus {
    pub bits: Vec<BitAt>,
    pub cost: u32,
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
    /// How many bits each run has, whatever is left of it.
    runs: Vec<usize>,
}

/// Whether bits, least significant first, are counted the way the site
/// counts a bus.
pub type Conventional<'c> = &'c dyn Fn(&[BitAt]) -> bool;

/// What a bus that bends the conventions costs.
const BENT: u32 = 2;

impl Pool {
    /// Every bit of `runs`, each run a stretch.
    pub fn of<T>(runs: &[Vec<T>]) -> Pool {
        Pool {
            stretches: runs
                .iter()
                .enumerate()
                .map(|(run, bits)| {
                    (0..bits.len()).map(|bit| (run, bit)).collect()
                })
                .collect(),
            runs: runs.iter().map(Vec::len).collect(),
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
            stretch.len() == self.runs[stretch[0].0] && stretch.len() < n
        });
        let bus = Bus {
            bits: self.stretches.concat(),
            cost: 0,
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
    /// stretch, or a lone line from one; and when `bent`, either end of a
    /// longer one, the alternate bits of one twice as long, or the bits of
    /// two stretches half as long, alternating.
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
            // A lone line from a row is as plain as any.
            if len > width && (bent || width == 1) {
                let bent = if width == 1 { 0 } else { BENT };
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
                let cost = cost + u32::from(!conventional(&bits));
                let bus = Bus { bits, cost };
                if !buses.iter().any(|(b, _)| b.bits == bus.bits) {
                    buses.push((bus, rest.clone()));
                }
            }
        }
        buses
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let pool = Pool::of(&[vec![(); 3]]);
        assert_eq!(
            widths(&pool, 3),
            [
                (vec![(0, 0), (0, 1), (0, 2)], 0),
                (vec![(0, 2), (0, 1), (0, 0)], 1),
            ]
        );
    }

    /// A run one longer than a bus gives the bus either end, the bit left
    /// over another slot's, as a sum with its carry out at its top.
    #[test]
    fn a_longer_run_gives_up_an_end() {
        let pool = Pool::of(&[vec![(); 3]]);
        let found = pool.take(2, &|_| true, true);
        let bits: Vec<Vec<BitAt>> =
            found.iter().map(|(bus, _)| bus.bits.clone()).collect();
        assert!(bits.contains(&vec![(0, 0), (0, 1)]));
        assert!(bits.contains(&vec![(0, 1), (0, 2)]));
        assert!(
            found
                .iter()
                .all(|(bus, rest)| bus.cost >= BENT && rest.bits() == 1)
        );
    }

    /// Two parallel runs make one bus, their bits alternating.
    #[test]
    fn two_runs_interleave() {
        let pool = Pool::of(&[vec![(); 2], vec![(); 2]]);
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
        };
        assert_eq!(bus.value(&[0b10, 0b0]), 0b10);
        assert_eq!(bus.value(&[0b00, 0b1]), 0b01);
    }
}
