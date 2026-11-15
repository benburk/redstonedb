//! Rejecting at once a build whose lines no kind takes, before anything is
//! simulated: its shape, as inspecting it reads it, the levers in buses
//! and lone lines, any hex digit inputs, and its outputs in lines, set
//! against what each kind's probe and test take. A kind its shape rules
//! out is not tried; a build every kind's rules out is recognized as none
//! straight away. Each rule is one the kind's probe or test holds it to
//! anyway, so only a build that would have failed on its lines is turned
//! away, and probing reads the lines that pass as before.

use super::split;
use crate::{
    category::{Kind, MIN_POPCOUNT, Probe, Test},
    interface::inspect::Report,
    recognize::matching,
};

/// A build's lines as inspecting found them, counted.
pub struct Shape {
    /// Every lever and button, in bits.
    levers: usize,
    /// Each line of levers, in bits; each bus of them, and how many lone
    /// ones there are.
    runs: Vec<usize>,
    buses: Vec<usize>,
    lones: usize,
    /// Its hex digit inputs, one a block.
    digits: usize,
    /// Each output line, in bits.
    outputs: Vec<usize>,
}

impl Shape {
    pub fn of(report: &Report) -> Shape {
        let (buses, lones) = split(&report.inputs);
        Shape {
            levers: report.inputs.iter().map(|run| run.bits.len()).sum(),
            runs: report.inputs.iter().map(|run| run.bits.len()).collect(),
            buses: buses.iter().map(|run| run.bits.len()).collect(),
            lones: lones.len(),
            digits: report.digits.iter().map(|run| run.bits.len()).sum(),
            outputs: report.outputs.iter().map(|run| run.bits.len()).collect(),
        }
    }

    /// Its widest output, which a kind of one operand or a sequential one
    /// reads as its result, and whether every other is a lone status line,
    /// as such a kind's probe needs.
    fn result(&self) -> Option<(usize, bool)> {
        let widest = *self.outputs.iter().max()?;
        let lone = self.outputs.iter().filter(|&&w| w > 1).count() <= 1;
        Some((widest, lone))
    }

    /// Whether a build of this shape could be a `kind`, probed `how` and
    /// tested by `test`: false only where the kind's probe or test would
    /// turn its lines away.
    pub fn could_be(&self, kind: Kind, how: Probe, test: Test) -> bool {
        // Only a kind taking digits reads a digit input, and every such
        // kind reads one; a reading leaving a digit out is none.
        if kind.facts().digit_inputs != (self.digits > 0) {
            return false;
        }
        let output: usize = self.outputs.iter().sum();
        match how {
            // Every lever and output bit fills one of its slots.
            Probe::Matched => matching::could_read(kind, self.levers, output),
            // Its digits are A and B; a lone lever, its carry in.
            Probe::HexAdder => self.levers <= 1,
            // Its one input is its digits.
            Probe::Digits => self.levers == 0,
            // Two buses as wide, A and B, and perhaps a third, C; at most a
            // lone lever, its calculate; the product from as wide as A to
            // as wide as A and B together.
            Probe::Multiplier => {
                let pairs = |a: usize| {
                    self.buses.iter().filter(|&&w| w == a).count() >= 2
                        && (a..=2 * a).contains(&output)
                };
                (2..=3).contains(&self.buses.len())
                    && self.lones <= 1
                    && (1..=3).contains(&self.outputs.len())
                    && self.buses.iter().any(|&a| pairs(a))
            }
            Probe::OneOperand(_) => self.one_operand(test),
            Probe::Sequential => self.sequential(test),
            // A line of levers as wide as its data bus out: its one output
            // line, or read loosely, every output together.
            Probe::Register => {
                let single = match self.outputs.as_slice() {
                    [one] => Some(*one),
                    _ => None,
                };
                self.runs.iter().any(|&w| Some(w) == single || w == output)
            }
            Probe::Alu | Probe::Shifter => true,
        }
    }

    /// Whether a kind of one operand tested by `test` takes it: every
    /// lever A, its widest output its result.
    fn one_operand(&self, test: Test) -> bool {
        let Some((result, lone)) = self.result() else {
            return false;
        };
        let a = self.levers;
        match test {
            // Each bit of A drives its own output bit.
            Test::Monostable | Test::DualEdgeMonostable => {
                self.outputs == [a]
                    || self.outputs.iter().all(|&w| w == 1)
                        && self.outputs.len() == a
            }
            // Wide enough to hold A's count, or one block showing it as its
            // strength.
            Test::Popcount => {
                let needed = (usize::BITS - a.leading_zeros()) as usize;
                lone && a >= MIN_POPCOUNT
                    && (result >= needed || result == 1 && a <= 15)
            }
            // A of at most 4 bits as the strength of one output block.
            Test::BinaryToHex => a <= 4 && self.outputs == [1],
            _ => lone,
        }
    }

    /// Whether a sequential kind tested by `test` takes it: its widest
    /// output the value it holds, an input bus as wide its A, and every
    /// other lever a control line, though some may lie in a line.
    fn sequential(&self, test: Test) -> bool {
        let Some((held, lone)) = self.result() else {
            return false;
        };
        let a = if held > 1 && self.buses.contains(&held) {
            held
        } else {
            0
        };
        let controls = self.levers - a;
        let ok = lone && controls >= 1;
        match test {
            // A clock and a write enable.
            Test::Counter => ok && controls == 2,
            // One input, its toggle, and one output bit.
            Test::TFlipFlop => self.levers == 1 && self.outputs == [1],
            _ => ok,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shape of lever lines `runs`, `digits` hex digit inputs, and output
    /// lines `outputs`.
    fn shape(runs: &[usize], digits: usize, outputs: &[usize]) -> Shape {
        Shape {
            levers: runs.iter().sum(),
            runs: runs.to_vec(),
            buses: runs.iter().copied().filter(|&w| w > 1).collect(),
            lones: runs.iter().filter(|&&w| w == 1).count(),
            digits,
            outputs: outputs.to_vec(),
        }
    }

    /// The kinds probing would try on `shape`.
    fn fitting(shape: &Shape) -> Vec<Kind> {
        Kind::probed()
            .into_iter()
            .filter(|&(kind, how, test)| shape.could_be(kind, how, test))
            .map(|(kind, ..)| kind)
            .collect()
    }

    /// The 24-bit multiplier as uploaded: its product on lamps the rule
    /// for outputs doesn't read, two buses of 24 levers, a stray comparator
    /// with air behind read as a hex digit, and a stray one facing air read
    /// as one output bit. No kind takes those lines.
    #[test]
    fn a_build_whose_lines_no_kind_takes_fits_none() {
        assert!(fitting(&shape(&[24, 24], 1, &[1])).is_empty());
    }

    /// The same with its outputs and strays fixed is a multiplier's shape,
    /// and no adder's, gate's, or comparison's.
    #[test]
    fn a_multipliers_lines_fit_a_multiplier_and_no_adder() {
        let found = fitting(&shape(&[24, 24], 0, &[48]));
        assert!(found.contains(&Kind::Multiplier));
        for kind in [Kind::Adder, Kind::XorGate, Kind::Equality, Kind::Decoder]
        {
            assert!(!found.contains(&kind), "{kind:?}");
        }
    }

    /// An 8-bit adder with a carry in and out fits an adder; a counter's
    /// value, A as wide, and two levers fit a counter; one lever and one
    /// output bit, a T flip-flop.
    #[test]
    fn the_usual_builds_fit_their_kinds() {
        assert!(fitting(&shape(&[8, 8, 1], 0, &[8, 1])).contains(&Kind::Adder));
        assert!(fitting(&shape(&[8, 1, 1], 0, &[8])).contains(&Kind::Counter));
        assert!(fitting(&shape(&[1], 0, &[1])).contains(&Kind::TFlipFlop));
        // A hex adder: digits, a carry in, no other lever.
        let hex = fitting(&shape(&[1], 16, &[8, 1]));
        assert!(hex.contains(&Kind::HexAdder));
        assert!(!hex.contains(&Kind::Adder));
    }
}
