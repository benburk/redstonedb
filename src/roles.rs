//! What each building block of a build is for, read off its neighbours by
//! the rules of redstone, with no simulation. A block may:
//!
//! - hold something up: dust, a torch, a repeater, a comparator, or a sign
//!   on top, or a torch, lever, button, or sign hanging from it;
//! - pass power, were it solid: hard from a torch under it, a repeater or
//!   comparator into it, or a lever on it, to dust or a component beside
//!   it; soft from dust on or into it, to a component reading it;
//! - cut a step, were it solid: dust under it and dust beside it, which
//!   otherwise join;
//! - step down, were it solid: let dust on it power dust a step down beside
//!   it, which dust on glass does not.
//!
//! A block that would pass power, cut a step, or step down is needed as
//! the kind it is. Otherwise it could be either kind if it holds something,
//! and is unused if it holds nothing. Dust is never unused, even dust that
//! powers nothing, as a branch shapes the dust it joins, so a block with
//! dust on it always holds something.
//! A build that stacks is read stacked as well, as a block can matter only
//! where copies meet. [`changed`] compiles a build before and after an
//! edit, the proof that the rules missed nothing.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use anyhow::Result;

use crate::{
    interface, lint,
    models::schematic::StackDirection,
    schem::{self, Blocks, Direction, Pos},
    simulate::{Compiled, Link, Stacking, Test, compiled},
    stack,
};

/// What a block must be.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Need {
    Unused,
    Solid,
    Transparent,
    Either,
}

impl Need {
    pub fn name(self) -> &'static str {
        match self {
            Need::Unused => "unused",
            Need::Solid => "solid",
            Need::Transparent => "transparent",
            Need::Either => "either",
        }
    }
}

/// What a block would do were it solid, which makes its kind matter.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Use {
    HardPower,
    SoftPower,
    CutsStep,
    StepsDown,
}

impl Use {
    pub fn name(self) -> &'static str {
        match self {
            Use::HardPower => "hard power",
            Use::SoftPower => "soft power",
            Use::CutsStep => "cuts a step",
            Use::StepsDown => "steps down",
        }
    }
}

#[derive(Debug)]
pub struct Role {
    pub need: Need,
    pub uses: Vec<Use>,
}

/// A build's file with its unused blocks taken out, and where they were.
pub struct Pruned {
    pub file: Vec<u8>,
    pub gone: Vec<Pos>,
}

/// The build in `data`, stacking each of `ways`, without its unused
/// blocks, once compiling it before and after shows every link as it was;
/// `None` when it has none.
pub fn prune(
    data: &[u8],
    ways: &[StackDirection],
    buses: stack::Buses,
) -> Result<Option<Pruned>> {
    let gone: BTreeMap<Pos, String> = roles(data, ways, buses)?
        .into_iter()
        .filter(|(_, role)| role.need == Need::Unused)
        .map(|(pos, _)| (pos, schem::AIR.to_owned()))
        .collect();
    if gone.is_empty() {
        return Ok(None);
    }
    let file = crate::standardize::edit(data, &gone)?;
    let changed = changed(data, &file, ways, buses)?;
    if let Some(first) = changed.first() {
        anyhow::bail!(
            "{} parts of its circuit would change, such as {first}",
            changed.len()
        );
    }
    Ok(Some(Pruned {
        file,
        gone: gone.into_keys().collect(),
    }))
}

/// What each building block of the build in `data` is for, by position,
/// read alone and stacked each of `ways`. Inputs and
/// blocks carrying a sign's text are left out, as they always stay.
pub fn roles(
    data: &[u8],
    ways: &[StackDirection],
    buses: stack::Buses,
) -> Result<BTreeMap<Pos, Role>> {
    let file = Blocks::read(data)?;
    let interface = interface::read(&file);
    let outputs: Vec<Pos> = interface
        .outputs
        .iter()
        .flat_map(|p| p.bits.iter().map(|b| b.pos))
        .collect();
    let inputs: HashSet<Pos> = interface
        .inputs
        .iter()
        .flat_map(|p| p.bits.iter().map(|b| b.pos))
        .chain(file.signs.keys().copied())
        .collect();
    let mut worlds = vec![World::new(Blocks::read(data)?, vec![[0, 0, 0]])];
    for &way in ways {
        let stacked = Blocks::read(&copies(data, way, buses)?)?;
        worlds.push(World::new(stacked, shifts(way)?));
    }
    // Where `p` stands in each copy of a world that still holds its block,
    // as a later copy may cover or leave out an earlier one's.
    let places = |w: &World, p: Pos| -> Vec<Pos> {
        let state = file.get(p);
        w.copies
            .iter()
            .map(|s| schem::add(p, *s))
            .filter(|&q| w.blocks.get(q) == state)
            .collect()
    };
    let mut found = BTreeMap::new();
    for (p, state) in file.iter() {
        let output = outputs.contains(&p);
        let building = schem::short(state).ends_with("_concrete")
            || schem::short(state).ends_with("glass");
        // A gray block marks what only a stack's first copy keeps, so it
        // does its work in a stack, as its later copies leave it out.
        if !building
            || lint::first_copy_only(schem::short(state))
            || inputs.contains(&p)
            || (output && !lint::is_output(schem::short(state)))
        {
            continue;
        }
        let mut uses = Vec::new();
        let mut holds = output;
        for w in &worlds {
            for q in places(w, p) {
                uses.extend(w.uses(q));
                holds |= !w.held(q).is_empty();
            }
        }
        uses.sort_unstable();
        uses.dedup();
        let glass = schem::short(state).ends_with("glass");
        let need = match (holds, uses.is_empty(), glass) {
            // Air passes power and steps as glass does.
            (false, _, true) | (false, true, false) => Need::Unused,
            (_, false, true) => Need::Transparent,
            (_, false, false) => Need::Solid,
            (true, true, _) => Need::Either,
        };
        found.insert(p, Role { need, uses });
    }
    Ok(found)
}

/// What differs between the circuits of the builds in `before` and
/// `after`, alone and stacked each of `ways`: each component whose kind
/// changed, and each link between components; empty when they compute the
/// same in the same ticks.
pub fn changed(
    before: &[u8],
    after: &[u8],
    ways: &[StackDirection],
    buses: stack::Buses,
) -> Result<Vec<String>> {
    let mut found = Vec::new();
    for (old, new) in circuits(before, ways, buses)?
        .into_iter()
        .zip(circuits(after, ways, buses)?)
    {
        let positions: BTreeSet<&Pos> =
            old.kinds.keys().chain(new.kinds.keys()).collect();
        for p in positions {
            if old.kinds.get(p) != new.kinds.get(p) {
                found.push(format!("the component at {p:?}"));
            }
        }
        let links: BTreeSet<&Link> =
            old.links.keys().chain(new.links.keys()).collect();
        for (from, to, _) in links
            .into_iter()
            .filter(|k| old.links.get(k) != new.links.get(k))
        {
            found.push(format!("the link from {from:?} to {to:?}"));
        }
    }
    found.dedup();
    Ok(found)
}

/// A hash of the circuit of the build in `data`, alone and stacked each of
/// `ways`: two files with the same key compute the same, stacked the same.
pub fn circuit_key(
    data: &[u8],
    ways: &[StackDirection],
    buses: stack::Buses,
) -> Result<String> {
    let keys: Vec<String> = circuits(data, ways, buses)?
        .iter()
        .map(Compiled::key)
        .collect();
    Ok(crate::import::sha256(keys.join(" ").as_bytes()))
}

/// The circuit of the build in `data` alone, then stacked each of `ways`.
fn circuits(
    data: &[u8],
    ways: &[StackDirection],
    buses: stack::Buses,
) -> Result<Vec<Compiled>> {
    let mut found = vec![compiled(&Blocks::read(data)?)?];
    for &way in ways {
        found.push(compiled(&Blocks::read(&copies(data, way, buses)?)?)?);
    }
    Ok(found)
}

/// The way copies of the build in `data`, of `category`, stack, as the
/// site's test finds it by simulating them: for a file with no stacking
/// stored, as the lab reads.
pub fn found_stacking(data: &[u8], category: &str) -> Option<StackDirection> {
    let interface = interface::read(&Blocks::read(data).ok()?);
    let width = interface.inputs.first().map_or(1, |p| p.bits.len());
    let test = Test::of(category, i64::try_from(width).ok())?;
    match test.stacking(data)? {
        Stacking::Stacks { step, .. } => Some(step.direction()),
        _ => None,
    }
}

/// The block `state` as the other kind, its colour kept: concrete for
/// glass, stained glass for concrete; `None` for any other block.
pub fn other_kind(state: &str) -> Option<String> {
    let name = schem::short(state);
    if name == "glass" {
        return Some("minecraft:white_concrete".to_owned());
    }
    if let Some(colour) = name.strip_suffix("_stained_glass") {
        return Some(format!("minecraft:{colour}_concrete"));
    }
    name.strip_suffix("_concrete")
        .map(|colour| format!("minecraft:{colour}_stained_glass"))
}

/// Whether dust `state` is saved as a dot, pointing no way.
pub fn is_dot(state: &str) -> bool {
    ["north", "south", "east", "west"]
        .iter()
        .all(|side| schem::property(state, side) == Some("none"))
}

/// How many copies a build that stacks is read in, at most.
const COPIES: i64 = 3;

/// How many copies of a build stacked `way` are read.
fn count(way: StackDirection) -> i64 {
    way.max_copies.map_or(COPIES, |most| most.min(COPIES))
}

/// Where `stack` moves each copy of a build stacked `way` that is read.
fn shifts(way: StackDirection) -> Result<Vec<Pos>> {
    let [west, up, north] = [way.west, way.up, way.north].map(i32::try_from);
    let (west, up, north) = (west?, up?, north?);
    let last = i32::try_from(count(way))? - 1;
    Ok((0..=last)
        .map(|i| [(last - i) * west, i * up, (last - i) * north])
        .collect())
}

/// Copies of the build in `data` stacked `way`, as the site stacks them,
/// sharing its buses as `buses` says, as a register file's copies share
/// their data bus.
fn copies(
    data: &[u8],
    way: StackDirection,
    buses: stack::Buses,
) -> Result<Vec<u8>> {
    let copies = usize::try_from(count(way))?;
    Ok(stack::stack(data, &[(way, copies)], buses)?.0)
}

const HORIZONTAL: [Direction; 4] = [
    Direction::North,
    Direction::South,
    Direction::East,
    Direction::West,
];

/// A world the rules are read in: the build alone, or copies of it
/// stacked, copy `i` shifted by `copies[i]`, with the blocks found unused
/// taken out.
struct World {
    blocks: Blocks,
    copies: Vec<Pos>,
}

impl World {
    fn new(blocks: Blocks, copies: Vec<Pos>) -> Self {
        World { blocks, copies }
    }

    fn get(&self, p: Pos) -> &str {
        self.blocks.get(p)
    }

    fn dust(&self, p: Pos) -> bool {
        schem::short(self.get(p)) == "redstone_wire"
    }

    /// What rests on or hangs from the block at `p`. The simulator runs
    /// such things floating, but the game drops them.
    fn held(&self, p: Pos) -> Vec<Pos> {
        let mut found = Vec::new();
        let above = schem::step(p, Direction::Up);
        let name = schem::short(self.get(above));
        if matches!(name, "redstone_wire" | "repeater" | "comparator")
            || (name.ends_with("_sign") && !name.ends_with("_wall_sign"))
        {
            found.push(above);
        }
        for d in Direction::ALL {
            let q = schem::step(p, d);
            let state = self.get(q);
            let on = match schem::short(state) {
                "lever" | "stone_button" => {
                    match schem::property(state, "face") {
                        Some("floor") => Some(schem::step(q, Direction::Down)),
                        Some("ceiling") => Some(schem::step(q, Direction::Up)),
                        _ => Direction::facing(state)
                            .map(|f| schem::step(q, f.opposite())),
                    }
                }
                name if name.ends_with("_wall_sign") => {
                    Direction::facing(state)
                        .map(|f| schem::step(q, f.opposite()))
                }
                _ => schem::torch_block(q, state),
            };
            if on == Some(p) {
                found.push(q);
            }
        }
        found
    }

    /// Whether dust at `q` points `d`, into a block or up its side, by
    /// what it joins as Redpiler reads it rather than the shape saved: it
    /// points to what it joins, along a line when it joins one thing, and
    /// every way when it joins nothing, unless saved as a dot.
    fn points(&self, q: Pos, d: Direction) -> bool {
        if !self.dust(q) {
            return false;
        }
        let joined = HORIZONTAL.map(|d| self.joins(q, d));
        match joined.iter().filter(|&&j| j).count() {
            0 => !is_dot(self.get(q)),
            1 => HORIZONTAL
                .iter()
                .zip(joined)
                .any(|(&e, j)| j && (e == d || e == d.opposite())),
            _ => HORIZONTAL.iter().zip(joined).any(|(&e, j)| j && e == d),
        }
    }

    /// Whether dust at `q` joins what is `d` of it: dust beside it, a step
    /// up, or a step down, or a component it turns to.
    fn joins(&self, q: Pos, d: Direction) -> bool {
        let side = schem::step(q, d);
        let up = schem::step(q, Direction::Up);
        if self.dust(side)
            || (!self.conductor(up)
                && self.dust(schem::step(side, Direction::Up)))
            || (!self.conductor(side)
                && self.dust(schem::step(side, Direction::Down)))
        {
            return true;
        }
        let state = self.get(side);
        match schem::short(state) {
            "repeater" => Direction::facing(state)
                .is_some_and(|f| f == d || f == d.opposite()),
            "observer" => Direction::facing(state) == Some(d),
            name => {
                name.ends_with("torch")
                    || name.ends_with("_button")
                    || matches!(
                        name,
                        "comparator"
                            | "lever"
                            | "redstone_block"
                            | "target"
                            | "tripwire_hook"
                    )
            }
        }
    }

    /// Whether the block at `p` passes power and cuts steps: not air,
    /// glass, a component, or a block of redstone.
    fn conductor(&self, p: Pos) -> bool {
        let name = schem::short(self.get(p));
        !(name == "air"
            || name.ends_with("glass")
            || name == "redstone_wire"
            || name == "repeater"
            || name == "comparator"
            || name.ends_with("torch")
            || name == "lever"
            || name.ends_with("_button")
            || name.ends_with("sign")
            || name == "tripwire_hook"
            || name == "redstone_block"
            || name == "hopper")
    }

    /// Whether a component reads the block at `p`: a torch on it, or a
    /// repeater or comparator with its back to it.
    fn read(&self, p: Pos) -> bool {
        Direction::ALL.into_iter().any(|d| {
            let q = schem::step(p, d);
            let state = self.get(q);
            match schem::short(state) {
                "repeater" | "comparator" => Direction::facing(state)
                    .is_some_and(|f| schem::step(q, f) == p),
                _ => schem::torch_block(q, state) == Some(p),
            }
        })
    }

    /// Whether the block at `p`, were it solid, would be powered hard: a
    /// torch under it, a repeater or comparator facing into it, or a lever
    /// or button on it.
    fn hard(&self, p: Pos) -> bool {
        let below = schem::short(self.get(schem::step(p, Direction::Down)));
        below.ends_with("torch")
            || HORIZONTAL.into_iter().any(|d| {
                let q = schem::step(p, d);
                let state = self.get(q);
                matches!(schem::short(state), "repeater" | "comparator")
                    && Direction::facing(state)
                        .is_some_and(|f| schem::step(q, f.opposite()) == p)
            })
            || self.held(p).into_iter().any(|q| {
                matches!(schem::short(self.get(q)), "lever" | "stone_button")
            })
    }

    /// Whether the block at `p`, were it solid, would be powered soft: dust
    /// on it or pointing into it.
    fn soft(&self, p: Pos) -> bool {
        self.dust(schem::step(p, Direction::Up))
            || HORIZONTAL
                .into_iter()
                .any(|d| self.points(schem::step(p, d), d.opposite()))
    }

    /// What a block at `p` would do were it solid.
    fn uses(&self, p: Pos) -> Vec<Use> {
        let mut found = Vec::new();
        let read = self.read(p);
        let near_dust = Direction::ALL
            .into_iter()
            .any(|d| self.dust(schem::step(p, d)));
        if self.hard(p) && (read || near_dust) {
            found.push(Use::HardPower);
        }
        if self.soft(p) && read {
            found.push(Use::SoftPower);
        }
        let under = schem::step(p, Direction::Down);
        let above = schem::step(p, Direction::Up);
        for d in HORIZONTAL {
            let side = schem::step(p, d);
            if self.dust(under) && self.dust(side) {
                found.push(Use::CutsStep);
            }
            if self.dust(above)
                && self.dust(side)
                && !self.conductor(schem::step(above, d))
            {
                found.push(Use::StepsDown);
            }
        }
        found
    }
}
