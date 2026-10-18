//! What each building block of a build is for, read off its neighbours by
//! the rules of redstone, with no simulation:
//!
//! - It holds something up: dust, a torch, a repeater, or a comparator on
//!   top, or a torch, lever, or button hanging from it. Glass can, though
//!   it may need a later release than the build works in.
//! - It passes power, so must be solid: hard from a torch under it, a
//!   repeater or comparator into it, or a lever on it, to dust or a
//!   component beside it; soft from dust on or into it, to a torch,
//!   repeater, or comparator reading it.
//! - It cuts a step: dust under it and dust beside it, diagonally up from
//!   that, join unless it is solid.
//! - It lets dust on it power dust a step down beside it, which glass does
//!   not.
//!
//! Glass that would do any of these were it solid must stay transparent,
//! unless it holds nothing, when air does as well. A solid block doing none
//! of them could be either if it holds something, and is unused if not.
//! Dust that powers nothing a component reads and joins at most one thing
//! is unused too, from the ends inwards. For a build that stacks, the rules
//! are read in stacked copies as well, as a block can matter only where
//! copies meet.
//!
//! The lab's `roles` command compiles a build before and after, the test
//! that the rules missed nothing; `rsdb prune` checks that the site's test
//! and timing come out the same.

use std::collections::HashSet;

use anyhow::Result;

use crate::{
    interface::{self, Interface},
    lint,
    models::schematic::StackDirection,
    schem::{self, Blocks, Direction, Pos},
    simulate::{Link, Stacking, Test, links},
    stack, standardize, works_from,
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

/// Why a block's kind matters, named for what it is now.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Use {
    HardPowered,
    SoftPowered,
    Cuts,
    StepsDown,
    /// Glass keeping power from what would read it.
    Insulates,
    /// Glass letting dust step past it.
    LetsStep,
    /// Glass under dust, keeping it from powering dust a step down.
    OneWay,
}

impl Use {
    pub fn name(self) -> &'static str {
        match self {
            Use::HardPowered => "hard-powered",
            Use::SoftPowered => "soft-powered",
            Use::Cuts => "cuts a step",
            Use::StepsDown => "steps down",
            Use::Insulates => "insulates",
            Use::LetsStep => "lets a step by",
            Use::OneWay => "one-way step",
        }
    }

    /// What a use of a solid block is for glass in its place.
    fn as_glass(self) -> Use {
        match self {
            Use::HardPowered | Use::SoftPowered => Use::Insulates,
            Use::Cuts => Use::LetsStep,
            Use::StepsDown => Use::OneWay,
            other => other,
        }
    }
}

pub struct Role {
    pub pos: Pos,
    pub state: String,
    pub need: Need,
    pub uses: Vec<Use>,
    /// What rests on it or hangs from it, in any copy, by short name.
    pub holds: Vec<String>,
    /// The later release the build would need were it glass.
    pub release: Option<&'static str>,
}

impl Role {
    fn new(pos: Pos, state: &str, need: Need) -> Self {
        Role {
            pos,
            state: state.to_owned(),
            need,
            uses: Vec::new(),
            holds: Vec::new(),
            release: None,
        }
    }
}

pub struct Roles {
    /// Every building block, and unused dust, by position.
    pub blocks: Vec<Role>,
    /// The ways copies were read stacked.
    pub ways: Vec<StackDirection>,
}

const HORIZONTAL: [Direction; 4] = [
    Direction::North,
    Direction::South,
    Direction::East,
    Direction::West,
];

/// A world the rules are read in: the build alone, or copies of it
/// stacked, copy `i` shifted by `copies[i]`.
struct World {
    blocks: Blocks,
    copies: Vec<Pos>,
    /// Output blocks; dust on one is how the site reads it.
    outputs: HashSet<Pos>,
}

/// A world's blocks with some taken out.
struct View<'a> {
    blocks: &'a Blocks,
    gone: &'a HashSet<Pos>,
}

impl View<'_> {
    fn get(&self, p: Pos) -> &str {
        if self.gone.contains(&p) {
            schem::AIR
        } else {
            self.blocks.get(p)
        }
    }

    fn dust(&self, p: Pos) -> bool {
        schem::short(self.get(p)) == "redstone_wire"
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
            || held(|q| self.get(q), p).into_iter().any(|q| {
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
            found.push(Use::HardPowered);
        }
        if self.soft(p) && read {
            found.push(Use::SoftPowered);
        }
        let under = schem::step(p, Direction::Down);
        let above = schem::step(p, Direction::Up);
        for d in HORIZONTAL {
            let side = schem::step(p, d);
            if self.dust(under) && self.dust(side) {
                found.push(Use::Cuts);
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

    /// Whether dust at `q` does something itself: it is on an output block,
    /// feeds a repeater's back or a comparator, or powers a block a
    /// component reads.
    fn dust_used(&self, q: Pos, outputs: &HashSet<Pos>) -> bool {
        let under = schem::step(q, Direction::Down);
        if outputs.contains(&under)
            || (self.conductor(under) && self.read(under))
        {
            return true;
        }
        HORIZONTAL.into_iter().any(|d| {
            let c = schem::step(q, d);
            let state = self.get(c);
            match schem::short(state) {
                "repeater" => Direction::facing(state)
                    .is_some_and(|f| schem::step(c, f) == q),
                "comparator" => self.points(q, d),
                _ => self.points(q, d) && self.conductor(c) && self.read(c),
            }
        })
    }

    /// How many things dust at `q` joins: dust beside it, a step up, or a
    /// step down, and components or hard-powered blocks next to it.
    fn dust_links(&self, q: Pos) -> usize {
        let up = schem::step(q, Direction::Up);
        let mut n = 0;
        for d in HORIZONTAL {
            let side = schem::step(q, d);
            let step_up = schem::step(side, Direction::Up);
            let step_down = schem::step(side, Direction::Down);
            n += usize::from(self.dust(side));
            n += usize::from(!self.conductor(up) && self.dust(step_up));
            n += usize::from(!self.conductor(side) && self.dust(step_down));
        }
        for d in Direction::ALL {
            let r = schem::step(q, d);
            let component = matches!(
                schem::short(self.get(r)),
                "redstone_torch"
                    | "redstone_wall_torch"
                    | "repeater"
                    | "comparator"
                    | "lever"
                    | "stone_button"
                    | "redstone_block"
            );
            n += usize::from(component || (self.conductor(r) && self.hard(r)));
        }
        n
    }

    /// The release glass at `p` would need for what it holds, as
    /// `works_from` dates them: dust or a standing torch on it 1.2.1, a
    /// repeater, comparator, lever, or button on it 1.3.1, and anything on
    /// its side or underside 1.14. `None` when it holds nothing.
    fn glass_release(&self, p: Pos) -> Option<&'static str> {
        let above = schem::step(p, Direction::Up);
        held(|q| self.get(q), p)
            .into_iter()
            .filter_map(|q| match schem::short(self.get(q)) {
                // A sign is no part of the circuit, and no release counts it.
                name if name.ends_with("_sign") => None,
                "redstone_wire" | "redstone_torch" if q == above => {
                    Some("1.2.1")
                }
                _ if q == above => Some("1.3.1"),
                _ => Some("1.14"),
            })
            .reduce(later)
    }
}

/// The later of two releases.
fn later(a: &'static str, b: &'static str) -> &'static str {
    if works_from::works_in(a, b) { b } else { a }
}

impl World {
    /// The dust that does nothing, from the ends inwards: dust doing
    /// nothing itself and joining at most one thing.
    fn dead_dust(&self) -> HashSet<Pos> {
        let mut gone = HashSet::new();
        loop {
            let view = View {
                blocks: &self.blocks,
                gone: &gone,
            };
            let dead: Vec<Pos> = self
                .blocks
                .iter()
                .map(|(q, _)| q)
                .filter(|&q| {
                    view.dust(q)
                        && !view.dust_used(q, &self.outputs)
                        && view.dust_links(q) <= 1
                })
                .collect();
            if dead.is_empty() {
                return gone;
            }
            gone.extend(dead);
        }
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

/// Whether `state` is glass of any kind.
pub fn transparent(state: &str) -> bool {
    schem::short(state).ends_with("glass")
}

/// What rests on or hangs from the block at `p`, read through `get`: dust,
/// a torch, a repeater, a comparator, or a sign on top, or a torch, lever,
/// button, or sign on any side. The simulator runs such things floating, but the
/// game drops them.
pub fn held<'a>(get: impl Fn(Pos) -> &'a str, p: Pos) -> Vec<Pos> {
    let mut found = Vec::new();
    let above = schem::step(p, Direction::Up);
    let name = schem::short(get(above));
    if matches!(name, "redstone_wire" | "repeater" | "comparator")
        || (name.ends_with("_sign") && !name.ends_with("_wall_sign"))
    {
        found.push(above);
    }
    for d in Direction::ALL {
        let q = schem::step(p, d);
        let state = get(q);
        let on = match schem::short(state) {
            "lever" | "stone_button" => match schem::property(state, "face") {
                Some("floor") => Some(schem::step(q, Direction::Down)),
                Some("ceiling") => Some(schem::step(q, Direction::Up)),
                _ => Direction::facing(state)
                    .map(|f| schem::step(q, f.opposite())),
            },
            name if name.ends_with("_wall_sign") => {
                Direction::facing(state).map(|f| schem::step(q, f.opposite()))
            }
            _ => schem::torch_block(q, state),
        };
        if on == Some(p) {
            found.push(q);
        }
    }
    found
}

/// Whether dust `state` is saved as a dot, pointing no way.
pub fn is_dot(state: &str) -> bool {
    ["north", "south", "east", "west"]
        .iter()
        .all(|side| schem::property(state, side) == Some("none"))
}

/// How many copies the rules read a build that stacks in, at most.
const COPIES: i64 = 3;

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

/// How many copies of a build stacked `way` the rules read.
fn count(way: StackDirection) -> i64 {
    way.max_copies.map_or(COPIES, |most| most.min(COPIES))
}

/// Where `stack` moves each copy of a build stacked `way` that the rules
/// read.
fn shifts(way: StackDirection) -> Result<Vec<Pos>> {
    let [west, up, north] = [way.west, way.up, way.north].map(i32::try_from);
    let (west, up, north) = (west?, up?, north?);
    let last = i32::try_from(count(way))? - 1;
    Ok((0..=last)
        .map(|i| [(last - i) * west, i * up, (last - i) * north])
        .collect())
}

/// Copies of the build in `data` stacked `way`, as the site stacks them.
pub fn copies(data: &[u8], way: StackDirection) -> Result<Vec<u8>> {
    let copies = usize::try_from(count(way))?;
    Ok(stack::stack(data, &[(way, copies)], stack::Buses::own())?.0)
}

/// The build alone, and stacked each of `ways`.
fn worlds(
    data: &[u8],
    interface: &Interface,
    ways: &[StackDirection],
) -> Result<Vec<World>> {
    let outputs: Vec<Pos> = interface
        .outputs
        .iter()
        .flat_map(|p| p.bits.iter().map(|b| b.pos))
        .collect();
    let mut found = vec![World {
        blocks: Blocks::read(data)?,
        copies: vec![[0, 0, 0]],
        outputs: outputs.iter().copied().collect(),
    }];
    for &way in ways {
        let shifts = shifts(way)?;
        found.push(World {
            blocks: Blocks::read(&copies(data, way)?)?,
            outputs: shifts
                .iter()
                .flat_map(|s| outputs.iter().map(|&p| schem::add(p, *s)))
                .collect(),
            copies: shifts,
        });
    }
    Ok(found)
}

/// What each building block of the build in `data` is for, and its
/// unused dust, read alone and stacked each of `ways`.
pub fn roles(data: &[u8], ways: &[StackDirection]) -> Result<Roles> {
    let file = Blocks::read(data)?;
    let interface = interface::read(&file);
    let ways = ways.to_vec();
    let worlds = worlds(data, &interface, &ways)?;
    let ports: HashSet<Pos> = interface
        .inputs
        .iter()
        .chain(&interface.outputs)
        .flat_map(|p| p.bits.iter().map(|b| b.pos))
        .collect();
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
    // Dust is unused when every world finds it so in every copy.
    let mut dead: HashSet<Pos> = file
        .iter()
        .filter(|(p, state)| {
            schem::short(state) == "redstone_wire"
                && !file.signs.contains_key(p)
        })
        .map(|(p, _)| p)
        .collect();
    for w in &worlds {
        let found = w.dead_dust();
        dead.retain(|&p| places(w, p).iter().all(|q| found.contains(q)));
    }
    let gone: Vec<HashSet<Pos>> = worlds
        .iter()
        .map(|w| dead.iter().flat_map(|&p| places(w, p)).collect())
        .collect();
    let base = works_from::of(&file).release;
    let mut blocks: Vec<Role> = dead
        .iter()
        .map(|&pos| Role::new(pos, file.get(pos), Need::Unused))
        .collect();
    for (pos, state) in file.iter() {
        // A block carrying a sign's text is a label, kept as a port is.
        let port = ports.contains(&pos) || file.signs.contains_key(&pos);
        // Inputs stay concrete; an output may be either.
        if other_kind(state).is_none()
            || (port && !lint::is_output(schem::short(state)))
        {
            continue;
        }
        let glass = transparent(state);
        let mut role = Role::new(pos, state, Need::Either);
        for (w, gone) in worlds.iter().zip(&gone) {
            let view = View {
                blocks: &w.blocks,
                gone,
            };
            for q in places(w, pos) {
                let uses = view.uses(q).into_iter();
                role.uses
                    .extend(uses.map(|u| if glass { u.as_glass() } else { u }));
                role.holds.extend(
                    held(|r| view.get(r), q)
                        .into_iter()
                        .map(|r| schem::short(view.get(r)).to_owned()),
                );
                role.release = role
                    .release
                    .into_iter()
                    .chain(view.glass_release(q))
                    .reduce(later);
            }
        }
        role.release = role.release.filter(|&r| !works_from::works_in(r, base));
        role.uses.sort_unstable();
        role.uses.dedup();
        role.holds.sort_unstable();
        role.holds.dedup();
        let holds = !role.holds.is_empty() || port;
        role.need = match (glass, holds, role.uses.is_empty()) {
            // Air passes power and steps as glass does.
            (true, false, _) | (false, false, true) => Need::Unused,
            (true, true, false) => Need::Transparent,
            (false, _, false) => Need::Solid,
            _ => Need::Either,
        };
        blocks.push(role);
    }
    blocks.sort_by_key(|r| r.pos);
    Ok(Roles { blocks, ways })
}

/// The build in `data` without the blocks `roles` finds unused.
pub fn without_unused(data: &[u8], roles: &Roles) -> Result<Vec<u8>> {
    let gone: HashSet<Pos> = roles
        .blocks
        .iter()
        .filter(|r| r.need == Need::Unused)
        .map(|r| r.pos)
        .collect();
    standardize::without(data, &gone)
}

/// The links that differ between the circuits of the builds in `before`
/// and `after`, alone and stacked each way `roles` read them, as
/// `from -> to` with the strength lost before and after, `-` where there
/// is no link. Links into the blocks `roles` found unused, which `after`
/// may no longer hold, are left out. Empty when the circuit is the same.
pub fn changed_links(
    before: &[u8],
    after: &[u8],
    roles: &Roles,
) -> Result<Vec<String>> {
    let unused: Vec<Pos> = roles
        .blocks
        .iter()
        .filter(|r| r.need == Need::Unused)
        .map(|r| r.pos)
        .collect();
    let mut pairs =
        vec![(before.to_vec(), after.to_vec(), vec![[0, 0, 0]], "")];
    for &way in &roles.ways {
        pairs.push((
            copies(before, way)?,
            copies(after, way)?,
            shifts(way)?,
            " stacked",
        ));
    }
    let xyz = |[x, y, z]: Pos| format!("{x},{y},{z}");
    let show = |d: Option<&u8>| d.map_or("-".to_owned(), u8::to_string);
    let mut found = Vec::new();
    for (was, now, shifts, label) in pairs {
        let gone: HashSet<Pos> = shifts
            .iter()
            .flat_map(|s| unused.iter().map(|&p| schem::add(p, *s)))
            .collect();
        let mut was = links(&Blocks::read(&was)?)?;
        was.retain(|(from, to, _), _| {
            !gone.contains(from) && !gone.contains(to)
        });
        let now = links(&Blocks::read(&now)?)?;
        let mut keys: Vec<&Link> = was.keys().chain(now.keys()).collect();
        keys.sort_unstable();
        keys.dedup();
        for k in keys {
            if was.get(k) != now.get(k) {
                found.push(format!(
                    "{} -> {}{}{label}: {} to {}",
                    xyz(k.0),
                    xyz(k.1),
                    if k.2 { " [side]" } else { "" },
                    show(was.get(k)),
                    show(now.get(k)),
                ));
            }
        }
    }
    Ok(found)
}
