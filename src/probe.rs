//! What a build's levers and outputs are, found by how it behaves rather than
//! by colour: each way its runs of levers and outputs could be an adder's
//! A, B, carry in, sum, and carry out, with each bus either way round, is
//! tried with the adder's test, and the one that adds is kept.

use std::collections::{BTreeMap, HashMap};

use crate::{
    colour,
    inspect::{self, Run},
    interface::{Bit, Interface, Port},
    schem::{self, Blocks, Direction, Pos, Switch},
    simulate::{self, Test},
};

/// An adder's ports as found: each bus least significant bit first.
#[derive(Debug, Clone)]
pub struct Layout {
    pub a: Vec<Pos>,
    pub b: Vec<Pos>,
    pub carry_in: Option<Pos>,
    pub sum: Vec<Pos>,
    pub carry_out: Option<Pos>,
}

/// What probing found: the layout that adds, and its test's run.
pub struct Probed {
    pub layout: Layout,
    pub run: simulate::Run,
}

impl Layout {
    /// The interface the adder's test reads, its ports labelled as the
    /// colours would mark them.
    pub fn interface(&self, blocks: &Blocks) -> Interface {
        let switches: HashMap<Pos, Switch> = blocks
            .switches()
            .into_iter()
            .map(|switch| (switch.block, switch))
            .collect();
        let input = |pos: Pos| {
            let switch = &switches[&pos];
            Bit {
                pos,
                entry: Some(switch.side),
                hard: Direction::ALL.iter().any(|&side| {
                    schem::id(blocks.get(schem::step(pos, side)))
                        == "minecraft:redstone_wire"
                }),
                inverted: switch.on,
            }
        };
        let output = |pos: Pos| Bit {
            pos,
            entry: None,
            hard: false,
            inverted: false,
        };
        let port = |name: &str, block: &str, bits: Vec<Bit>| Port {
            name: name.to_owned(),
            block: block.to_owned(),
            labelled: false,
            bits,
            strength: 1,
            words: Vec::new(),
            signal_strength: false,
        };
        let mut inputs = vec![
            port(
                "A",
                "lime concrete",
                self.a.iter().map(|&p| input(p)).collect(),
            ),
            port(
                "B",
                "light blue concrete",
                self.b.iter().map(|&p| input(p)).collect(),
            ),
        ];
        if let Some(pos) = self.carry_in {
            inputs.push(port("carry in", "magenta concrete", vec![input(pos)]));
        }
        let mut outputs = vec![port(
            "output",
            "yellow concrete",
            self.sum.iter().map(|&p| output(p)).collect(),
        )];
        if let Some(pos) = self.carry_out {
            outputs.push(port("carry out", "pink concrete", vec![output(pos)]));
        }
        Interface { inputs, outputs }
    }

    /// Where its parts are, for a message.
    pub fn describe(&self) -> String {
        let bus = |bits: &[Pos]| match bits {
            [one] => inspect::place(*one),
            [first, .., last] => format!(
                "{} bits from {} to {}",
                bits.len(),
                inspect::place(*first),
                inspect::place(*last)
            ),
            [] => "none".to_owned(),
        };
        let mut parts = vec![
            format!("A: {}", bus(&self.a)),
            format!("B: {}", bus(&self.b)),
        ];
        if let Some(pos) = self.carry_in {
            parts.push(format!("carry in: {}", inspect::place(pos)));
        }
        parts.push(format!("sum: {}", bus(&self.sum)));
        if let Some(pos) = self.carry_out {
            parts.push(format!("carry out: {}", inspect::place(pos)));
        }
        parts.join("; ")
    }
}

/// A and B, least significant bit first, and a carry in.
type Operands = (Vec<Pos>, Vec<Pos>, Option<Pos>);

/// The sum, least significant bit first, and a carry out.
type Results = (Vec<Pos>, Option<Pos>);

/// The run's bits either way round.
fn both(bits: &[Pos]) -> [Vec<Pos>; 2] {
    let forward = bits.to_vec();
    let mut back = forward.clone();
    back.reverse();
    [forward, back]
}

/// The run's bits from the end its step leaves, so parallel runs come
/// the same way round.
fn ordered(run: &Run) -> Vec<Pos> {
    let mut bits = run.bits.clone();
    if run.step.iter().sum::<i32>() < 0 {
        bits.reverse();
    }
    bits
}

/// Two parallel runs of one length as one bus whose bits alternate between
/// them: either run first, either way round.
fn interleaved(x: &Run, y: &Run) -> Vec<Vec<Pos>> {
    if x.bits.len() != y.bits.len() {
        return Vec::new();
    }
    let (xs, ys) = (ordered(x), ordered(y));
    let mut found = Vec::new();
    for (first, second) in [(&xs, &ys), (&ys, &xs)] {
        for back in [false, true] {
            let (mut f, mut s) = (first.clone(), second.clone());
            if back {
                f.reverse();
                s.reverse();
            }
            found.push(f.iter().zip(&s).flat_map(|(a, b)| [*a, *b]).collect());
        }
    }
    found
}

fn split(runs: &[Run]) -> (Vec<&Run>, Vec<Pos>) {
    let buses = runs.iter().filter(|run| run.bits.len() > 1).collect();
    let lones = runs
        .iter()
        .filter(|run| run.bits.len() == 1)
        .map(|run| run.bits[0])
        .collect();
    (buses, lones)
}

/// Where `runs`' lone bits are, such as `(1, 2, 3), (4, 5, 6)`, when one
/// or two are all that keep the rest in line.
fn strays(runs: &[Run]) -> Option<String> {
    let lone: Vec<String> = runs
        .iter()
        .filter(|run| run.bits.len() == 1)
        .map(|run| inspect::place(run.bits[0]))
        .collect();
    (1..=2).contains(&lone.len()).then(|| lone.join(", "))
}

/// `runs`' lengths, such as `7, 7, 1, 1`.
fn lengths(runs: &[Run]) -> String {
    let each: Vec<String> =
        runs.iter().map(|run| run.bits.len().to_string()).collect();
    each.join(", ")
}

/// What is wrong with outputs `runs` as a `width`-bit sum and carry out:
/// too few; one or two out of line; uneven gaps along the axis they
/// spread furthest on; or, evenly spaced, not in a line.
fn output_fault(runs: &[Run], width: usize) -> String {
    let mut bits: Vec<Pos> =
        runs.iter().flat_map(|run| run.bits.clone()).collect();
    if bits.len() < width {
        return format!("Output bus: {} outputs, need {width}", bits.len());
    }
    if let Some(at) = strays(runs) {
        return format!("Output bus: out of line at {at}");
    }
    let extent = |axis: usize| {
        let (lo, hi) = bits.iter().fold((i32::MAX, i32::MIN), |(lo, hi), b| {
            (lo.min(b[axis]), hi.max(b[axis]))
        });
        hi - lo
    };
    let axis = (0..3).max_by_key(|&axis| extent(axis)).unwrap_or(1);
    bits.sort_by_key(|bit| bit[axis]);
    let gaps: Vec<i32> =
        bits.windows(2).map(|w| w[1][axis] - w[0][axis]).collect();
    if gaps.windows(2).any(|w| w[0] != w[1]) {
        let gaps: Vec<String> = gaps.iter().map(ToString::to_string).collect();
        let name = ["x", "y", "z"][axis];
        format!(
            "Output bus: uneven spacing, {name} gaps {}",
            gaps.join(", ")
        )
    } else {
        "Output bus: not in a line".to_owned()
    }
}

/// Every way the input runs could be A, B, and a carry in: two equal buses
/// and at most one lone lever; one bus of both, halved or interleaved; a
/// bus with the carry in at one end, in line with it; four equal buses,
/// each operand two of them with its bits alternating; or, for one bit,
/// two or three lone levers.
fn operands(runs: &[Run]) -> Result<Vec<Operands>, String> {
    let (buses, lones) = split(runs);
    let mut pairs: Vec<Operands> = Vec::new();
    let mut add = |x: &[Pos], y: &[Pos], carry: Option<Pos>| {
        for (p, q) in [(x, y), (y, x)] {
            for a in both(p) {
                for b in both(q) {
                    pairs.push((a.clone(), b, carry));
                }
            }
        }
    };
    match (buses.as_slice(), lones.as_slice()) {
        ([x, y], _) if lones.len() <= 1 && x.bits.len() == y.bits.len() => {
            add(&x.bits, &y.bits, lones.first().copied());
        }
        ([x, y], []) if x.bits.len().abs_diff(y.bits.len()) == 1 => {
            let (short, long) = if x.bits.len() < y.bits.len() {
                (x, y)
            } else {
                (y, x)
            };
            let n = short.bits.len();
            add(&short.bits, &long.bits[..n], Some(long.bits[n]));
            add(&short.bits, &long.bits[1..], Some(long.bits[0]));
        }
        ([x], _) if lones.len() <= 1 && x.bits.len() % 2 == 0 => {
            let n = x.bits.len() / 2;
            let carry = lones.first().copied();
            add(&x.bits[..n], &x.bits[n..], carry);
            let even: Vec<Pos> = x.bits.iter().step_by(2).copied().collect();
            let odd: Vec<Pos> =
                x.bits.iter().skip(1).step_by(2).copied().collect();
            add(&even, &odd, carry);
        }
        // Each operand two parallel lines, its bits alternating between
        // them, as a build adding twice its height's bits does.
        ([w, x, y, z], _)
            if lones.len() <= 1
                && [x, y, z].iter().all(|r| r.bits.len() == w.bits.len()) =>
        {
            let carry = lones.first().copied();
            for ((p, q), (r, t)) in
                [((w, x), (y, z)), ((w, y), (x, z)), ((w, z), (x, y))]
            {
                for a in interleaved(p, q) {
                    for b in interleaved(r, t) {
                        pairs.push((a.clone(), b.clone(), carry));
                        pairs.push((b, a.clone(), carry));
                    }
                }
            }
        }
        ([], [p, q]) => add(&[*p], &[*q], None),
        ([], [p, q, r]) => {
            for (x, y, carry) in [(p, q, r), (p, r, q), (q, r, p)] {
                add(&[*x], &[*y], Some(*carry));
            }
        }
        _ => {
            return Err(strays(runs).map_or_else(
                || format!("Input buses: line lengths {}", lengths(runs)),
                |at| format!("Input bus: out of line at {at}"),
            ));
        }
    }
    Ok(pairs)
}

/// Every way the output runs could be an adder's sum of `width` bits and a
/// carry out: a bus and at most one lone output; a bus one longer, its last
/// bit the carry out or the sum's top bit; two equal buses, the sum's bits
/// alternating between them; or, for one bit, lone outputs.
fn results(runs: &[Run], width: usize) -> Result<Vec<Results>, String> {
    let (buses, lones) = split(runs);
    let mut found = Vec::new();
    match (buses.as_slice(), lones.as_slice()) {
        ([sum], _) if lones.len() <= 1 && sum.bits.len() == width => {
            for bits in both(&sum.bits) {
                found.push((bits, lones.first().copied()));
            }
        }
        ([sum], []) if sum.bits.len() == width + 1 => {
            for bits in both(&sum.bits) {
                found.push((bits[..width].to_vec(), Some(bits[width])));
                found.push((bits, None));
            }
        }
        ([x, y], _)
            if lones.len() <= 1
                && x.bits.len() == y.bits.len()
                && x.bits.len() * 2 == width =>
        {
            for bits in interleaved(x, y) {
                found.push((bits, lones.first().copied()));
            }
        }
        ([], [sum]) if width == 1 => {
            found.push((vec![*sum], None));
        }
        ([], [p, q]) if width == 1 => {
            found.push((vec![*p], Some(*q)));
            found.push((vec![*q], Some(*p)));
        }
        _ => {
            return Err(output_fault(runs, width));
        }
    }
    Ok(found)
}

/// The adder `blocks` is, found from `report`'s runs of levers and outputs:
/// of every layout they could make, the one whose quick test gets fewest
/// wrong, then tested in full; or why none adds.
pub fn adder(
    blocks: &Blocks,
    report: &inspect::Report,
) -> Result<Probed, String> {
    let pairs = operands(&report.inputs)?;
    let width = pairs.first().map_or(0, |(a, _, _)| a.len());
    if Test::of("adder", i64::try_from(width).ok()).is_none() {
        return Err(format!("Width {width}: tested range 1-32"));
    }
    let outs = results(&report.outputs, width)?;
    let mut best: Option<(u64, u64, Layout)> = None;
    let mut reasons: Vec<String> = Vec::new();
    for (a, b, carry_in) in &pairs {
        for (sum, carry_out) in &outs {
            let layout = Layout {
                a: a.clone(),
                b: b.clone(),
                carry_in: *carry_in,
                sum: sum.clone(),
                carry_out: *carry_out,
            };
            match Test::Adder.check_with(blocks, layout.interface(blocks), true)
            {
                Ok(run) => {
                    let better =
                        best.as_ref().is_none_or(|(wrong, cases, _)| {
                            run.wrong * cases < wrong * run.cases
                        });
                    if better {
                        best = Some((run.wrong, run.cases, layout));
                    }
                    if run.wrong == 0 {
                        break;
                    }
                }
                Err(err) => reasons.push(err.to_string()),
            }
        }
        if best.as_ref().is_some_and(|(wrong, _, _)| *wrong == 0) {
            break;
        }
    }
    let Some((wrong, cases, layout)) = best else {
        reasons.dedup();
        return Err(format!(
            "Can't simulate: {}",
            reasons.first().map_or("no layout fits", String::as_str)
        ));
    };
    if wrong > 0 {
        return Err(format!("Doesn't add: {wrong} of {cases} cases wrong"));
    }
    // A and B the other way round may add too, read more plainly, as a
    // subtractor's A − B rather than not (B − A − 1).
    let swapped = Layout {
        a: layout.b.clone(),
        b: layout.a.clone(),
        ..layout.clone()
    };
    let inversions = |pattern: &str| pattern.matches("not").count();
    let quick = |layout: &Layout| {
        Test::Adder
            .check_with(blocks, layout.interface(blocks), true)
            .ok()
            .filter(|run| run.wrong == 0)
    };
    let layout = match (quick(&layout), quick(&swapped)) {
        (Some(run), Some(other))
            if inversions(&other.pattern) < inversions(&run.pattern) =>
        {
            swapped
        }
        _ => layout,
    };
    let run = Test::Adder
        .check_with(blocks, layout.interface(blocks), false)
        .map_err(|err| format!("Test couldn't run: {err}"))?;
    if run.wrong > 0 {
        let bits: Vec<String> = run
            .wrong_bits
            .iter()
            .map(|(bit, n)| format!("{bit} ({n})"))
            .collect();
        return Err(format!(
            "Doesn't add: {} of {} cases wrong; wrong bits {}",
            run.wrong,
            run.cases,
            bits.join(", ")
        ));
    }
    Ok(Probed { layout, run })
}

/// The colours the site marks roles with, as its colour reader takes them.
const MARKS: [&str; 9] = [
    "lime",
    "light_blue",
    "cyan",
    "blue",
    "magenta",
    "yellow",
    "orange",
    "brown",
    "pink",
];

/// The file in `data` with `layout`'s roles marked in colour, as the rest
/// of the site reads them: A lime, B light blue, carry in magenta, the sum
/// yellow, and carry out pink. Every other block of one of those colours
/// turns white, so none is taken for an input or output. A block keeps its
/// kind, stained glass staying glass, as dust treats it differently.
pub fn colour(data: &[u8], layout: &Layout) -> anyhow::Result<Vec<u8>> {
    let mut roles: HashMap<Pos, &str> = HashMap::new();
    roles.extend(layout.a.iter().map(|&p| (p, "lime")));
    roles.extend(layout.b.iter().map(|&p| (p, "light_blue")));
    roles.extend(layout.carry_in.map(|p| (p, "magenta")));
    roles.extend(layout.sum.iter().map(|&p| (p, "yellow")));
    roles.extend(layout.carry_out.map(|p| (p, "pink")));
    paint(data, &roles)
}

/// The file in `data` with each block of `roles` in its colour, and every
/// other block of a colour that marks a role turned white.
fn paint(data: &[u8], roles: &HashMap<Pos, &str>) -> anyhow::Result<Vec<u8>> {
    let blocks = Blocks::read(data)?;
    let recoloured = |pos: Pos, name: &str| -> Option<String> {
        let glass = name.ends_with("_stained_glass");
        let colour = match roles.get(&pos) {
            Some(colour) => *colour,
            None if MARKS.iter().any(|c| {
                ["_concrete", "_wool", "_terracotta", "_stained_glass"]
                    .iter()
                    .any(|kind| name == format!("{c}{kind}"))
            }) =>
            {
                "white"
            }
            None => return None,
        };
        let kind = if glass { "stained_glass" } else { "concrete" };
        Some(format!("minecraft:{colour}_{kind}"))
    };
    let changes = blocks
        .iter()
        .filter_map(|(pos, state)| {
            let name = schem::short(state);
            let want = recoloured(pos, name)?;
            (want != state).then_some((pos, want))
        })
        .collect();
    crate::standardize::edit(data, &changes)
}

/// What the check by function makes of an uploaded file: the file with its
/// roles coloured in, when it is recognized as an adder, else as it came;
/// what was found; and what only its builder can put right.
pub struct Recognition {
    pub file: Vec<u8>,
    pub found: Option<String>,
    pub problems: Vec<String>,
}

/// The file in `data` with what the simulator does not model but redstone
/// treats as a plain block in its place: a slab, stairs, or glowstone as
/// white stained glass, which redstone sees through as it does them, and a
/// furnace, cauldron, or cake as a barrel giving the same strength. Every
/// colour stays, so no input or output is lost, as standardizing would
/// whiten an output it does not know.
pub fn simulatable(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    let data = crate::standardize::barrels(data, &[])?;
    let blocks = Blocks::read(&data)?;
    let changes: BTreeMap<Pos, String> = blocks
        .iter()
        .filter(|&(_, state)| {
            let name = schem::short(state);
            name.ends_with("_slab")
                || name.ends_with("_stairs")
                || name == "glowstone"
        })
        .map(|(pos, _)| (pos, "minecraft:white_stained_glass".to_owned()))
        .collect();
    if changes.is_empty() {
        return Ok(data);
    }
    crate::standardize::edit(&data, &changes)
}

/// The file in `data` inspected and, if nothing is wrong with its levers
/// and outputs, probed as an adder, then as a register, an ALU, and a
/// decoder.
pub fn recognize(data: &[u8]) -> anyhow::Result<Recognition> {
    recognize_as(data, None)
}

/// As `recognize`, probing only as `meant`, one of [`KINDS`], when given.
/// Probed as each and recognized as none, its fault is as `fault_of`
/// chooses.
pub fn recognize_as(
    data: &[u8],
    meant: Option<&str>,
) -> anyhow::Result<Recognition> {
    let simulated = simulatable(data)?;
    let data = simulated.as_slice();
    let blocks = Blocks::read(data)?;
    let report = inspect::inspect(&blocks);
    let mut problems: Vec<String> =
        report.faults().map(|p| p.message.clone()).collect();
    if problems.is_empty() {
        problems.extend(inspect::not_bus_drivable(data)?.into_iter().map(
            |lever| {
                format!("Lever at {}: not bus-drivable", inspect::place(lever))
            },
        ));
    }
    let unrecognized = |problems| Recognition {
        file: data.to_vec(),
        found: None,
        problems,
    };
    if !problems.is_empty() {
        return Ok(unrecognized(problems));
    }
    // What a probe found, unless a data input it found has dust on top or
    // a block behind its lever, which only a control line's may.
    let accept = |data_inputs: &[Pos], file: Vec<u8>, found: String| {
        let faults = report.data_faults(data_inputs);
        if faults.is_empty() {
            Recognition {
                file,
                found: Some(found),
                problems: Vec::new(),
            }
        } else {
            unrecognized(faults)
        }
    };
    // Each blessed category in turn, or only the one meant, its fault kept
    // when it isn't one.
    let mut faults: [Option<String>; 7] = Default::default();
    for (i, kind) in KINDS.into_iter().enumerate() {
        if meant.is_some_and(|meant| meant != kind) {
            continue;
        }
        match probe_as(kind, data, &blocks, &report)? {
            Ok((data_inputs, file, found)) => {
                return Ok(accept(&data_inputs, file, found));
            }
            Err(why) => faults[i] = Some(why),
        }
    }
    let fault = fault_of(&blocks, &report, meant, faults);
    problems.push(fault);
    Ok(unrecognized(problems))
}

/// The blessed categories, in the order probing tries them.
const KINDS: [&str; 7] = [
    "adder",
    "register",
    "alu",
    "decoder",
    "multiplier",
    "counter",
    "shift_register",
];

/// What a probe found: the build's data inputs, its file coloured by the
/// roles found, and what it is, in words.
pub type Found = (Vec<Pos>, Vec<u8>, String);

/// The file in `data`, with `blocks` and `report`, probed as a `kind` of
/// [`KINDS`]: its data inputs, the file coloured by the roles found, and
/// what it was found to be, such as `an adder: all 131072 cases right, A +
/// B + carry in`; or why it isn't one.
pub fn probe_as(
    kind: &str,
    data: &[u8],
    blocks: &Blocks,
    report: &inspect::Report,
) -> anyhow::Result<Result<Found, String>> {
    let found = |what: &str, run: &simulate::Run| {
        format!("{what}: all {} cases right, {}", run.cases, run.pattern)
    };
    let probed = probed_as(kind, data, blocks, report, found)?;
    Ok(match probed {
        Ok((inputs, file, found)) => {
            Ok((inputs, unlabelled(kind, &file)?, found))
        }
        Err(why) => Err(why),
    })
}

/// The words a sign may name a role by that probing finds anyway, for a
/// `kind` of [`KINDS`]; an ALU's, a decoder's, and a shift register's
/// signs are kept, as they tell what their lines do.
fn role_words(kind: &str) -> &'static [&'static str] {
    match kind {
        "adder" => &[
            "carry in",
            "carry-in",
            "cin",
            "c in",
            "carry out",
            "carry-out",
            "cout",
            "c out",
        ],
        "register" => &["write", "read", "write enable", "data in", "data out"],
        "multiplier" => &["calculate", "calc", "reset"],
        "counter" => &["clock", "clk", "write", "write enable", "load"],
        _ => &[],
    }
}

/// The file in `data`, found to be a `kind` of [`KINDS`], less each sign
/// that only names a role probing found, such as `carry in`; a sign saying
/// anything more, such as a strength, stays.
fn unlabelled(kind: &str, data: &[u8]) -> anyhow::Result<Vec<u8>> {
    let words = role_words(kind);
    let blocks = Blocks::read(data)?;
    let changes: BTreeMap<Pos, String> = blocks
        .signs
        .iter()
        .filter(|(_, lines)| {
            let said: Vec<String> = lines
                .iter()
                .map(|line| line.trim().to_lowercase())
                .filter(|line| !line.is_empty())
                .collect();
            !said.is_empty()
                && said.iter().all(|line| words.contains(&line.as_str()))
        })
        .map(|(&pos, _)| (pos, schem::AIR.to_owned()))
        .collect();
    if changes.is_empty() {
        return Ok(data.to_vec());
    }
    crate::standardize::edit(data, &changes)
}

/// As `probe_as`, the file as probing coloured it, its signs as they were.
fn probed_as(
    kind: &str,
    data: &[u8],
    blocks: &Blocks,
    report: &inspect::Report,
    found: impl Fn(&str, &simulate::Run) -> String,
) -> anyhow::Result<Result<Found, String>> {
    Ok(match kind {
        "adder" => match adder(blocks, report) {
            Ok(probed) => Ok((
                [probed.layout.a.clone(), probed.layout.b.clone()].concat(),
                colour(data, &probed.layout)?,
                found("an adder", &probed.run),
            )),
            Err(why) => Err(why),
        },
        "register" => register(data, report).map(|probed| {
            let found = found("a register", &probed.run);
            (probed.data_in, probed.file, found)
        }),
        "alu" => alu(data, report).map(|probed| {
            let found = found("an ALU", &probed.run);
            ([probed.a, probed.b].concat(), probed.file, found)
        }),
        "decoder" => decoder(data, report).map(|probed| {
            let found = found("a decoder", &probed.run);
            (probed.a, probed.file, found)
        }),
        "multiplier" => multiplier(data, report).map(|probed| {
            let found = found("a multiplier", &probed.run);
            ([probed.a, probed.b].concat(), probed.file, found)
        }),
        "counter" => sequential(data, report, Test::Counter).map(|probed| {
            let found = found("a counter", &probed.run);
            (probed.data, probed.file, found)
        }),
        _ => sequential(data, report, Test::ShiftRegister).map(|probed| {
            let found = found("a shift register", &probed.run);
            (probed.data, probed.file, found)
        }),
    })
}

/// Of the faults probing found as each of [`KINDS`], the one to show: the
/// one it is meant as, if any; else
/// the decoder's when it has one input line; the register's when it has a
/// button, which only a register's writes and a sequential multiplier's
/// lines are; the ALU's when it has more than one lone lever, its control
/// lines; else the adder's.
fn fault_of(
    blocks: &Blocks,
    report: &inspect::Report,
    meant: Option<&str>,
    [adder, register, alu, decoder, multiplier, counter, shift]: [Option<String>;
        7],
) -> String {
    let buttons = blocks.switches().iter().any(|switch| switch.button);
    let lone = report
        .inputs
        .iter()
        .filter(|run| run.bits.len() == 1)
        .count();
    match meant {
        Some("adder") => adder,
        Some("register") => register,
        Some("alu") => alu,
        Some("decoder") => decoder,
        Some("multiplier") => multiplier,
        Some("counter") => counter,
        Some("shift_register") => shift,
        _ if report.inputs.len() == 1 => decoder.or(adder).or(register),
        _ if buttons => register.or(multiplier).or(adder).or(alu),
        _ if lone > 1 => alu.or(adder).or(register),
        _ => adder.or(register).or(alu),
    }
    .unwrap_or_default()
}

/// A register as probing found it: its data bus in, its controls, writes
/// on buttons and reads on levers, its data bus out, the file with them
/// coloured as the register test reads them, and that test's run.
pub struct RegisterProbed {
    pub data_in: Vec<Pos>,
    pub controls: Vec<Pos>,
    pub data_out: Vec<Pos>,
    pub file: Vec<u8>,
    pub run: simulate::Run,
}

/// The register the file in `data` is, from `report`'s levers and outputs:
/// its one output line the data bus out, an input line as wide the data
/// bus in, and every other switch a control; each way of choosing the bus
/// in coloured and run through the register test, quick cases first. Or
/// why none holds.
pub fn register(
    data: &[u8],
    report: &inspect::Report,
) -> Result<RegisterProbed, String> {
    let [out] = report.outputs.as_slice() else {
        return Err(format!(
            "Output bus: line lengths {}",
            lengths(&report.outputs)
        ));
    };
    let width = out.bits.len();
    let blocks = Blocks::read(data).map_err(|err| err.to_string())?;
    let buttons: Vec<Pos> = blocks
        .switches()
        .into_iter()
        .filter(|switch| switch.button)
        .map(|switch| switch.block)
        .collect();
    let every: Vec<Pos> = report
        .inputs
        .iter()
        .flat_map(|run| run.bits.clone())
        .collect();
    // A data line is levers; a lone button is a write.
    let buses: Vec<&Run> = report
        .inputs
        .iter()
        .filter(|run| {
            run.bits.len() == width
                && run.bits.iter().all(|bit| !buttons.contains(bit))
        })
        .collect();
    if buses.is_empty() {
        return Err(format!("Data bus: no line of {width} levers"));
    }
    let mut why = String::new();
    for bus in buses {
        let controls: Vec<Pos> = every
            .iter()
            .filter(|bit| !bus.bits.contains(bit))
            .copied()
            .collect();
        if !controls.iter().any(|bit| buttons.contains(bit)) {
            "Controls: no write button".clone_into(&mut why);
            continue;
        }
        let mut roles: HashMap<Pos, &str> = HashMap::new();
        roles.extend(bus.bits.iter().map(|&p| (p, "lime")));
        roles.extend(controls.iter().map(|&p| (p, "magenta")));
        roles.extend(out.bits.iter().map(|&p| (p, "yellow")));
        let file = paint(data, &roles).map_err(|err| err.to_string())?;
        let coloured = Blocks::read(&file).map_err(|err| err.to_string())?;
        match Test::Register.quick(&coloured) {
            Ok(run) if run.wrong == 0 => {}
            Ok(run) => {
                why = format!(
                    "Doesn't hold: {} of {} cases wrong",
                    run.wrong, run.cases
                );
                continue;
            }
            Err(err) => {
                why = format!("Test couldn't run: {err}");
                continue;
            }
        }
        match Test::Register.run(&coloured) {
            simulate::Report::Ran(run) if run.wrong == 0 => {
                return Ok(RegisterProbed {
                    data_in: bus.bits.clone(),
                    controls,
                    data_out: out.bits.clone(),
                    file,
                    run: *run,
                });
            }
            simulate::Report::Ran(run) => {
                why = format!(
                    "Doesn't hold: {} of {} cases wrong",
                    run.wrong, run.cases
                );
            }
            simulate::Report::Broken { reason }
            | simulate::Report::Skipped { reason } => {
                why = format!("Test couldn't run: {reason}");
            }
        }
    }
    Err(why)
}

/// An ALU as probing found it: its operands A and B, its control lines,
/// its output and status lines, the file with them coloured as the ALU
/// test reads them, and that test's run.
pub struct AluProbed {
    pub a: Vec<Pos>,
    pub b: Vec<Pos>,
    pub controls: Vec<Pos>,
    pub output: Vec<Pos>,
    pub status: Vec<Pos>,
    pub file: Vec<u8>,
    pub run: simulate::Run,
}

/// The ALU the file in `data` is, from `report`'s levers and outputs: its
/// widest output line its output and each lone output a status line, named
/// by its sign; two input lines as wide its operands, A first either way
/// round; and every other lever a control line, each coloured and run
/// through the ALU test, quick cases first. Or why none holds.
pub fn alu(data: &[u8], report: &inspect::Report) -> Result<AluProbed, String> {
    let Some(out) = report.outputs.iter().max_by_key(|run| run.bits.len())
    else {
        return Err("Outputs: none".to_owned());
    };
    let width = out.bits.len();
    let status: Vec<Pos> = report
        .outputs
        .iter()
        .filter(|run| run.bits.len() == 1 && run.bits != out.bits)
        .flat_map(|run| run.bits.clone())
        .collect();
    if report
        .outputs
        .iter()
        .any(|run| run.bits != out.bits && run.bits.len() > 1)
    {
        return Err(format!(
            "Outputs: line lengths {}, need one bus and lone status lines",
            lengths(&report.outputs)
        ));
    }
    let operands: Vec<&Run> = report
        .inputs
        .iter()
        .filter(|run| run.bits.len() == width)
        .collect();
    let [first, second] = operands.as_slice() else {
        return Err(format!(
            "Operands: {} lines of {width} levers, need A and B",
            operands.len()
        ));
    };
    let controls: Vec<Pos> = report
        .inputs
        .iter()
        .filter(|run| run.bits.len() != width)
        .flat_map(|run| run.bits.clone())
        .collect();
    let mut why = String::new();
    for (a, b) in [(first, second), (second, first)] {
        let mut roles: HashMap<Pos, &str> = HashMap::new();
        roles.extend(a.bits.iter().map(|&p| (p, "lime")));
        roles.extend(b.bits.iter().map(|&p| (p, "light_blue")));
        roles.extend(controls.iter().map(|&p| (p, "magenta")));
        roles.extend(out.bits.iter().map(|&p| (p, "yellow")));
        roles.extend(status.iter().map(|&p| (p, "pink")));
        let file = paint(data, &roles).map_err(|err| err.to_string())?;
        let coloured = Blocks::read(&file).map_err(|err| err.to_string())?;
        match Test::Alu.quick(&coloured) {
            Ok(run) if run.wrong == 0 => {}
            Ok(run) => {
                why = format!(
                    "Doesn't hold: {} of {} cases wrong",
                    run.wrong, run.cases
                );
                continue;
            }
            Err(err) => {
                why = format!("Test couldn't run: {err}");
                continue;
            }
        }
        match Test::Alu.run(&coloured) {
            simulate::Report::Ran(run) if run.wrong == 0 => {
                return Ok(AluProbed {
                    a: a.bits.clone(),
                    b: b.bits.clone(),
                    controls,
                    output: out.bits.clone(),
                    status,
                    file,
                    run: *run,
                });
            }
            simulate::Report::Ran(run) => {
                why = format!(
                    "Doesn't hold: {} of {} cases wrong",
                    run.wrong, run.cases
                );
            }
            simulate::Report::Broken { reason }
            | simulate::Report::Skipped { reason } => {
                why = format!("Test couldn't run: {reason}");
            }
        }
    }
    Err(why)
}

/// A multiplier as probing found it: its operands A and B, its control
/// lines, its product's lines, lowest first, the file with them coloured
/// as the multiplier test reads them, and that test's run.
pub struct MultiplierProbed {
    pub a: Vec<Pos>,
    pub b: Vec<Pos>,
    pub controls: Vec<Pos>,
    pub product: Vec<Vec<Pos>>,
    pub file: Vec<u8>,
    pub run: simulate::Run,
}

/// The multiplier the file in `data` is, from `report`'s levers and
/// outputs: its two widest input lines of one width its operands, A and B,
/// whichever is which, as A × B is B × A; a lone lever or button, if
/// any, a sequential one's calculate; and its output lines, up to three, its product, tried in each
/// order, lowest first, each coloured and run through the multiplier
/// test, quick cases first. Or why none holds.
pub fn multiplier(
    data: &[u8],
    report: &inspect::Report,
) -> Result<MultiplierProbed, String> {
    let (buses, controls) = split(&report.inputs);
    let [a, b] = buses.as_slice() else {
        return Err(format!(
            "Operands: line lengths {}, need A and B",
            lengths(&report.inputs)
        ));
    };
    if a.bits.len() != b.bits.len() {
        return Err(format!(
            "Operands: {} and {} levers, need A and B as wide",
            a.bits.len(),
            b.bits.len()
        ));
    }
    let lines: Vec<&Run> = report.outputs.iter().collect();
    if lines.is_empty() || lines.len() > colour::OUTPUT_COLOURS.len() {
        return Err(format!(
            "Product: line lengths {}, need one to three lines",
            lengths(&report.outputs)
        ));
    }
    let mut why = String::new();
    // Of the orders that ran, the share of cases the closest got wrong,
    // which `why` tells, rather than one that couldn't run.
    let mut closest = f64::INFINITY;
    let mut wrong = |run: &simulate::Run, why: &mut String| {
        #[allow(clippy::cast_precision_loss)]
        let share = run.wrong as f64 / run.cases.max(1) as f64;
        if share < closest {
            closest = share;
            *why = format!(
                "Doesn't hold: {} of {} cases wrong",
                run.wrong, run.cases
            );
        }
    };
    for order in orders(lines.len()) {
        let product: Vec<Vec<Pos>> =
            order.iter().map(|&i| lines[i].bits.clone()).collect();
        let mut roles: HashMap<Pos, &str> = HashMap::new();
        roles.extend(a.bits.iter().map(|&p| (p, "lime")));
        roles.extend(b.bits.iter().map(|&p| (p, "light_blue")));
        roles.extend(controls.iter().map(|&p| (p, colour::CONTROL)));
        for (bits, colour) in product.iter().zip(colour::OUTPUT_COLOURS) {
            roles.extend(bits.iter().map(|&p| (p, colour)));
        }
        let file = paint(data, &roles).map_err(|err| err.to_string())?;
        let coloured = Blocks::read(&file).map_err(|err| err.to_string())?;
        match Test::Multiplier.quick(&coloured) {
            Ok(run) if run.wrong == 0 => {}
            Ok(run) => {
                wrong(&run, &mut why);
                continue;
            }
            Err(err) => {
                if !why.starts_with("Doesn't hold") {
                    why = format!("Test couldn't run: {err}");
                }
                continue;
            }
        }
        match Test::Multiplier.run(&coloured) {
            simulate::Report::Ran(run) if run.wrong == 0 => {
                return Ok(MultiplierProbed {
                    a: a.bits.clone(),
                    b: b.bits.clone(),
                    controls,
                    product,
                    file,
                    run: *run,
                });
            }
            simulate::Report::Ran(run) => wrong(&run, &mut why),
            simulate::Report::Broken { reason }
            | simulate::Report::Skipped { reason } => {
                if !why.starts_with("Doesn't hold") {
                    why = format!("Test couldn't run: {reason}");
                }
            }
        }
    }
    Err(why)
}

/// A counter or shift register as probing found it: its input A, if any,
/// the file with its lines coloured as its test reads them, and that
/// test's run.
pub struct SequentialProbed {
    pub data: Vec<Pos>,
    pub file: Vec<u8>,
    pub run: simulate::Run,
}

/// The counter or shift register the file in `data` is, as `test` checks
/// it, from `report`'s levers and outputs: its widest output the value it
/// holds and each lone output a status line, such as a shift out; an input
/// line as wide as that value its input A, to load; and every other lever
/// or button a control line of its own, each coloured and run through the
/// test, which finds what each control line does. Or why it isn't one.
pub fn sequential(
    data: &[u8],
    report: &inspect::Report,
    test: Test,
) -> Result<SequentialProbed, String> {
    let Some(held) = report.outputs.iter().max_by_key(|run| run.bits.len())
    else {
        return Err("Outputs: none".to_owned());
    };
    let width = held.bits.len();
    if report
        .outputs
        .iter()
        .any(|run| run.bits != held.bits && run.bits.len() > 1)
    {
        return Err(format!(
            "Outputs: line lengths {}, need one line and lone status lines",
            lengths(&report.outputs)
        ));
    }
    let status: Vec<Pos> = report
        .outputs
        .iter()
        .filter(|run| run.bits != held.bits)
        .flat_map(|run| run.bits.clone())
        .collect();
    let a: Vec<Pos> = report
        .inputs
        .iter()
        .find(|run| run.bits.len() == width && width > 1)
        .map(|run| run.bits.clone())
        .unwrap_or_default();
    let controls: Vec<Pos> = report
        .inputs
        .iter()
        .filter(|run| run.bits != a)
        .flat_map(|run| run.bits.clone())
        .collect();
    let mut roles: HashMap<Pos, &str> = HashMap::new();
    roles.extend(a.iter().map(|&p| (p, "lime")));
    roles.extend(controls.iter().map(|&p| (p, colour::CONTROL)));
    roles.extend(held.bits.iter().map(|&p| (p, "yellow")));
    roles.extend(status.iter().map(|&p| (p, colour::STATUS)));
    let file = paint(data, &roles).map_err(|err| err.to_string())?;
    let coloured = Blocks::read(&file).map_err(|err| err.to_string())?;
    match test.run(&coloured) {
        simulate::Report::Ran(run) if run.wrong == 0 => Ok(SequentialProbed {
            data: a,
            file,
            run: *run,
        }),
        simulate::Report::Ran(run) => Err(format!(
            "Doesn't hold: {} of {} cases wrong",
            run.wrong, run.cases
        )),
        simulate::Report::Broken { reason }
        | simulate::Report::Skipped { reason } => {
            Err(format!("Test couldn't run: {reason}"))
        }
    }
}

/// Every order of `n` things, by their places, as `[1, 0]` for two the
/// other way round.
fn orders(n: usize) -> Vec<Vec<usize>> {
    if n == 0 {
        return vec![Vec::new()];
    }
    let mut all = Vec::new();
    for rest in orders(n - 1) {
        for at in 0..=rest.len() {
            let mut order = rest.clone();
            order.insert(at, n - 1);
            all.push(order);
        }
    }
    all
}

/// A decoder as probing found it: its input A, its output, the file with
/// them coloured as the decoder test reads them, and that test's run.
pub struct DecoderProbed {
    pub a: Vec<Pos>,
    pub output: Vec<Pos>,
    pub file: Vec<u8>,
    pub run: simulate::Run,
}

/// The decoder the file in `data` is, from `report`'s levers and outputs:
/// its one input line A and its one output line, a bit for each of A's
/// values, coloured and run through the decoder test. Or why it isn't.
pub fn decoder(
    data: &[u8],
    report: &inspect::Report,
) -> Result<DecoderProbed, String> {
    let [a] = report.inputs.as_slice() else {
        return Err(format!(
            "Input: line lengths {}, need one line A",
            lengths(&report.inputs)
        ));
    };
    let [out] = report.outputs.as_slice() else {
        return Err(format!(
            "Output: line lengths {}, need one line",
            lengths(&report.outputs)
        ));
    };
    if out.bits.len() != 1 << a.bits.len().min(16) {
        return Err(format!(
            "Output: {} bits, need one for each of A's {} values",
            out.bits.len(),
            1_usize << a.bits.len().min(16)
        ));
    }
    let mut roles: HashMap<Pos, &str> = HashMap::new();
    roles.extend(a.bits.iter().map(|&p| (p, "lime")));
    roles.extend(out.bits.iter().map(|&p| (p, "yellow")));
    let file = paint(data, &roles).map_err(|err| err.to_string())?;
    let coloured = Blocks::read(&file).map_err(|err| err.to_string())?;
    match Test::Decoder.run(&coloured) {
        simulate::Report::Ran(run) if run.wrong == 0 => Ok(DecoderProbed {
            a: a.bits.clone(),
            output: out.bits.clone(),
            file,
            run: *run,
        }),
        simulate::Report::Ran(run) => Err(format!(
            "Doesn't hold: {} of {} cases wrong",
            run.wrong, run.cases
        )),
        simulate::Report::Broken { reason }
        | simulate::Report::Skipped { reason } => {
            Err(format!("Test couldn't run: {reason}"))
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    /// The fixture `name` with its input colours, lime, light blue, and
    /// magenta, turned white, as a build without colour conventions is.
    fn uncoloured(name: &str) -> Vec<u8> {
        let data =
            std::fs::read(format!("tests/fixtures/{name}.schem")).unwrap();
        let blocks = Blocks::read(&data).unwrap();
        let changes: BTreeMap<Pos, String> = blocks
            .iter()
            .filter(|&(_, state)| {
                ["lime", "light_blue", "magenta"]
                    .iter()
                    .any(|c| schem::short(state).starts_with(c))
            })
            .map(|(pos, _)| (pos, "minecraft:white_concrete".to_owned()))
            .collect();
        crate::standardize::edit(&data, &changes).unwrap()
    }

    #[test]
    fn an_alu_is_found_from_its_levers_and_control_lines() {
        let data = uncoloured("alu-8-and-xor");
        let found = recognize(&data).unwrap();
        assert!(
            found
                .found
                .as_deref()
                .is_some_and(|f| f.starts_with("an ALU")),
            "{:?} {:?}",
            found.found,
            found.problems
        );
    }

    /// A counter whose clock button counts and loads A with its write enable
    /// lever on, and a shift register that loads A and shifts either way,
    /// found by what its lines do.
    #[test]
    fn a_counter_and_a_shift_register_are_found_from_their_lines() {
        for (name, kind) in [
            ("counter-8", "a counter"),
            ("shift-register-8", "a shift register"),
        ] {
            let found = recognize(&uncoloured(name)).unwrap();
            assert!(
                found.found.as_deref().is_some_and(|f| f.starts_with(kind)),
                "{name}: {:?} {:?}",
                found.found,
                found.problems
            );
        }
    }

    /// A counter has one clock button and one write enable lever, so one
    /// loading A on a button of its own isn't one.
    #[test]
    fn a_counter_loading_on_a_second_button_is_not_one() {
        let found =
            recognize_as(&uncoloured("counter-8-two-buttons"), Some("counter"))
                .unwrap();
        assert!(found.found.is_none(), "{:?}", found.found);
        assert!(
            found
                .problems
                .iter()
                .any(|p| p.contains("write enable lever")),
            "{:?}",
            found.problems
        );
    }

    /// A sign that only names a role probing finds, such as `carry in`, is
    /// taken out; one that says more, such as a carry out's strength,
    /// stays.
    #[test]
    fn signs_naming_only_a_role_are_taken_out() {
        let found = recognize(&uncoloured("diag-3t-cca")).unwrap();
        assert!(found.found.is_some(), "{:?}", found.problems);
        let blocks = Blocks::read(&found.file).unwrap();
        let mut said: Vec<String> = blocks
            .signs
            .values()
            .map(|lines| {
                lines
                    .iter()
                    .map(|line| line.trim())
                    .filter(|line| !line.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        said.sort();
        assert_eq!(said, ["carry out 1ss"]);
    }

    /// A sequential multiplier, its one control line its calculate.
    #[test]
    fn a_multiplier_is_found_from_its_operands_and_product() {
        let data = uncoloured("mult-8-calculate");
        let found = recognize_as(&data, Some("multiplier")).unwrap();
        assert!(
            found
                .found
                .as_deref()
                .is_some_and(|f| f.starts_with("a multiplier")),
            "{:?} {:?}",
            found.found,
            found.problems
        );
    }

    /// A multiplier has one control line, its calculate, so one with a
    /// reset button too isn't one.
    #[test]
    fn a_multiplier_with_a_reset_is_not_one() {
        // Raw, with blocks the simulator doesn't model until they are
        // standardized.
        let raw = std::fs::read("tests/fixtures/mult-8-buttons.schem").unwrap();
        let standard = crate::standardize::standardize(&raw).unwrap();
        let found = recognize_as(&standard, Some("multiplier")).unwrap();
        assert!(found.found.is_none(), "{:?}", found.found);
        assert!(
            found
                .problems
                .iter()
                .any(|p| p.contains("at most one control line")),
            "{:?}",
            found.problems
        );
    }

    #[test]
    fn a_register_is_found_from_its_levers_and_buttons() {
        let data = uncoloured("register-8-2-cells");
        let found = recognize(&data).unwrap();
        assert!(
            found
                .found
                .as_deref()
                .is_some_and(|f| f.starts_with("a register")),
            "{:?} {:?}",
            found.found,
            found.problems
        );
        let as_adder = recognize_as(&data, Some("adder")).unwrap();
        assert!(as_adder.found.is_none());
    }
}
