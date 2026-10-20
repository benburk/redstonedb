//! Testing a decoder: input A, of some bits, into one output with a bit for
//! each of A's values, the one A names on and the rest off, or, for an
//! inverted one, that one off and the rest on.

use anyhow::Result;

use super::{
    Broken,
    bench::{Bench, Cases, Hint, Kind, Spec, mask},
};
use crate::interface::Interface;

/// A decoder's cases, as they are named.
const ON: &str = "bit A on";

/// An inverted decoder's cases, as they are named, which its flag follows.
pub const INVERTED: &str = "bit A off";

/// The widest A a decoder is tested at: its output has a bit for each value.
const WIDEST: usize = 6;

/// A decoder's every case: each value of A, its bit on and the rest off,
/// or every bit of that inverted, whichever the build gives.
pub(super) fn spec(interface: &Interface) -> Result<Spec> {
    let &[a] = (0..interface.inputs.len()).collect::<Vec<_>>().as_slice()
    else {
        broken!(
            "A decoder takes one input, A, but this has {}.",
            interface.inputs.len()
        );
    };
    let &[out] = (0..interface.outputs.len()).collect::<Vec<_>>().as_slice()
    else {
        broken!(
            "A decoder has one output, but this has {}.",
            interface.outputs.len()
        );
    };
    let width = interface.inputs[a].bits.len();
    let out_bits = interface.outputs[out].bits.len();
    if width > WIDEST || out_bits != 1 << width {
        broken!(
            "A has {width} bits, so a decoder's output has {}, but this has \
             {out_bits}.",
            1_usize << width.min(63)
        );
    }
    let all = mask(out_bits);
    Ok(Spec {
        bench: Bench::new(vec![a], vec![out], ON),
        cases: Cases::Combinations(Box::new(move || {
            ((0..1 << width).map(|a| [a, 0, 0]).collect(), 0)
        })),
        kinds: vec![
            Kind::new(ON, |case| 1 << case[0]),
            Kind::new(INVERTED, move |case| !(1 << case[0]) & all),
        ],
        hint: Hint::None,
        by_digit: false,
    })
}
