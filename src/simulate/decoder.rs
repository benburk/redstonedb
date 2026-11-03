//! Testing a decoder: input A, of some bits, into one output with a bit for
//! each of A's values, the one A names off and the rest on. A decoder's
//! outputs are inverted, as a row of torches gives them; one whose
//! selected bit is on and the rest off gets every case wrong.

use anyhow::Result;

use super::{
    Broken, Test,
    bench::{Bench, Cases, Kind, Spec, mask},
    rng::Rng,
    stacking::{Apart, Stacking, Step},
};
use crate::{
    models::schematic::Orientation,
    recognize::interface::{self, Interface},
    schem::{Blocks, stack},
};

/// A decoder's cases, as they are named.
const OFF: &str = "bit A off";

/// The widest A a decoder is tested at: its output has a bit for each value.
const WIDEST: usize = 6;

/// A decoder's every case: each value of A, its bit off and the rest on.
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
        bench: Bench::new(vec![a], vec![out], OFF),
        cases: Cases::Combinations(Box::new(move || {
            ((0..1 << width).map(|a| [a, 0, 0]).collect(), 0)
        })),
        kinds: vec![Kind::new(OFF, move |case| !(1 << case[0]) & all)],
        by_digit: false,
    })
}

/// The most bits of A a stack of decoders is tried on every value of.
const EVERY: usize = 16;

/// How copies of the decoder in `data`, whose own cases came out right,
/// stack: each way, west and up, the shortest step, as far as the build
/// reaches and two past, at which two copies each decode their own bits of
/// A onto their own outputs, then whether a third does too, so any number
/// do. `None` when it is no decoder.
pub fn stacking(data: &[u8]) -> Option<Stacking> {
    let blocks = Blocks::read(data).ok()?;
    let interface = interface::read(&blocks);
    spec(&interface).ok()?;
    let (low, high) = interface::measured(&blocks);
    let reach = |axis: usize| i64::from(high[axis] - low[axis] + 1) + 2;
    let mut steps: Vec<Step> = (1..=reach(0))
        .map(|west| Step::new(west, 0, 0))
        .chain((1..=reach(1)).map(|up| Step::new(0, up, 0)))
        .collect();
    steps.sort_by_key(|step| step.west + step.up);
    let buses = buses();
    let works = |step: Step, copies: usize| {
        copies_apart(data, &interface, step, copies, buses)
    };
    // The shortest step each way, west and up, at which copies work.
    let ways: Vec<Apart> = [true, false]
        .into_iter()
        .filter_map(|west| {
            let step = steps
                .iter()
                .copied()
                .filter(|step| (step.west > 0) == west)
                .find_map(|step| {
                    step.find(data, true, buses, |step| {
                        works(step, 2).ok_or(())
                    })?
                    .ok()
                    .map(|(step, _)| step)
                })?;
            Some(match works(step, 3) {
                Some(cases) => Apart {
                    step,
                    copies: 3,
                    cases,
                    most: None,
                },
                None => Apart {
                    step,
                    copies: 2,
                    cases: works(step, 2).unwrap_or_default(),
                    most: Some(2),
                },
            })
        })
        .collect();
    if ways.is_empty() {
        return Some(Stacking::Not {
            step: None,
            reason: "No step west or up, as far as it reaches and two past, \
                     makes copies that each decode their own bits of A."
                .to_owned(),
        });
    }
    Some(Stacking::Separate { ways })
}

/// How a decoder's copies share their buses: they don't, each keeping its
/// own input and outputs.
fn buses() -> stack::Buses {
    stack::Buses::of(Test::Decoder.kind(), Orientation::Horizontal)
}

/// The cases `copies` of the decoder in `data`, whose interface is
/// `single`, run when stacked `step` and each decodes its own bits.
fn copies_apart(
    data: &[u8],
    single: &Interface,
    step: Step,
    copies: usize,
    buses: stack::Buses,
) -> Option<u64> {
    let (stacked, _) =
        stack::stack(data, &[(step.direction(), copies)], buses).ok()?;
    let stacked = Blocks::read(&stacked).ok()?;
    apart(&stacked, single, (step, copies))
}

/// Whether `copies` of the decoder in `data` stacked `step` each decode
/// their own bits, as [`stacking`] tries each step: the box rule's box
/// first, then boxes leaving out more of the first copy, the step with
/// the box that works and the cases run, or why none does. `None` when it
/// is no decoder.
pub fn stacks_at(
    data: &[u8],
    step: Step,
    copies: usize,
) -> Option<Result<(Step, u64), String>> {
    let blocks = Blocks::read(data).ok()?;
    let interface = interface::read(&blocks);
    spec(&interface).ok()?;
    step.find(data, true, buses(), |step| {
        copies_apart(data, &interface, step, copies, buses()).ok_or_else(|| {
            format!("{copies} copies don't each decode their own bits of A")
        })
    })
}

/// Whether `copies` of the decoder whose interface is `single`, stacked
/// every `step` in `blocks`, each decode their own bits of A onto their own
/// outputs, every value of A or, with too many, a sample: the cases run
/// when every one came out right. Each bit of the stack's A and output is
/// its copy's by where it sits, the copy's own bit moved by the step, as
/// copies stacked up lie their bits between each other's.
fn apart(
    blocks: &Blocks,
    single: &Interface,
    (step, copies): (Step, usize),
) -> Option<u64> {
    let interface = interface::read(blocks);
    let (&[a], &[out]) = (
        (0..interface.inputs.len()).collect::<Vec<_>>().as_slice(),
        (0..interface.outputs.len()).collect::<Vec<_>>().as_slice(),
    ) else {
        return None;
    };
    let width = single.inputs[0].bits.len();
    let one = 1 << width;
    let bits = width * copies;
    if interface.inputs[a].bits.len() != bits
        || interface.outputs[out].bits.len() != one * copies
        || one * copies > 64
    {
        return None;
    }
    // Where copy `k` moves its blocks to, as `stack` lays copies out.
    let last = i32::try_from(copies).ok()? - 1;
    let [west, up] =
        [step.west, step.up].map(|n| i32::try_from(n).unwrap_or(0));
    let shift = |k: i32| [(last - k) * west, k * up, 0];
    // Each stacked bit's copy and its place in that copy.
    let owner =
        |pos: [i32; 3], own: &[interface::Bit]| -> Option<(usize, usize)> {
            (0..=last).find_map(|k| {
                let [dx, dy, dz] = shift(k);
                let at = [pos[0] - dx, pos[1] - dy, pos[2] - dz];
                let j = own.iter().position(|bit| bit.pos == at)?;
                Some((usize::try_from(k).ok()?, j))
            })
        };
    let a_bits: Vec<(usize, usize)> = interface.inputs[a]
        .bits
        .iter()
        .map(|bit| owner(bit.pos, &single.inputs[0].bits))
        .collect::<Option<_>>()?;
    let out_bits: Vec<(usize, usize)> = interface.outputs[out]
        .bits
        .iter()
        .map(|bit| owner(bit.pos, &single.outputs[0].bits))
        .collect::<Option<_>>()?;
    let decode = move |value: u64| {
        let mut each = vec![0; copies];
        for (i, &(k, j)) in a_bits.iter().enumerate() {
            each[k] |= (value >> i & 1) << j;
        }
        out_bits.iter().enumerate().fold(0, |output, (i, &(k, j))| {
            let on = u64::try_from(j).unwrap_or(u64::MAX) != each[k];
            output | u64::from(on) << i
        })
    };
    let cases = move || {
        let values: Vec<u64> = if bits <= EVERY {
            (0..1 << bits).collect()
        } else {
            let mut rng = Rng::default();
            (0..4096).map(|_| rng.draw() & mask(bits)).collect()
        };
        (values.into_iter().map(|a| [a, 0, 0]).collect(), 0)
    };
    let spec = Spec {
        bench: Bench::new(vec![a], vec![out], OFF),
        cases: Cases::Combinations(Box::new(cases)),
        kinds: vec![Kind::new(OFF, move |case| decode(case[0]))],
        by_digit: false,
    };
    let run = spec.check(blocks, &interface, false).ok()?;
    (run.wrong == 0).then_some(run.cases)
}
