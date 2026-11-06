//! A logic analyzer: a build's circuit traced tick by tick through a change
//! of its inputs, each component's strength at every tick and what set each
//! change off; and many changes swept, for when each output settles, which
//! of its bits glitch, and the build's races.
//!
//! A race is a component that runs a tick on a tick on which an input of
//! its own changes, that input's update running at the same priority: which
//! of the two goes first decides what the component does. The game runs
//! them in the order they were scheduled, which comes of the order it
//! updates blocks in, and so of where the build stands and which way it
//! faces; Redpiler runs them in an order of its own, which may differ each
//! time it compiles the build. A trace finds a race from the strengths it
//! records, whichever way its own run went, by working out what the
//! component comes to with its rivals gone first and with them after.

use std::{
    collections::{BTreeMap, HashMap},
    time::Instant,
};

use anyhow::{Result, bail};
use mchprs_blocks::blocks::ComparatorMode;
use mchprs_redpiler::compile_graph::{LinkType, NodeType};

use super::{
    Value, bench,
    circuit::{self, Carried, Circuit, MAX_TICKS, input_bits, output_bits},
};
use crate::{
    recognize::interface::Interface,
    schem::{Blocks, Pos},
};

/// What a node of a build's circuit is, as far as a trace follows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Repeater {
        delay: u8,
        /// Giving out into another repeater or comparator, which runs its
        /// ticks first of all.
        facing_diode: bool,
    },
    Torch,
    Comparator {
        subtract: bool,
        /// The strength of a container it reads through a block, which
        /// stands in for its back while that is weaker.
        far_input: Option<u8>,
        facing_diode: bool,
    },
    /// A lever or button, which the trace switches.
    Switch {
        button: bool,
    },
    /// A container a comparator reads, at its strength.
    Constant(u8),
    /// A lamp, or anything else Redpiler models that powers nothing.
    Other(&'static str),
}

impl Part {
    /// Its name, such as `comparator`.
    pub fn name(self) -> &'static str {
        match self {
            Part::Repeater { .. } => "repeater",
            Part::Torch => "torch",
            Part::Comparator { .. } => "comparator",
            Part::Switch { button: false } => "lever",
            Part::Switch { button: true } => "button",
            Part::Constant(_) => "container",
            Part::Other(name) => name,
        }
    }

    /// Whether it runs ticks, as a repeater, torch, or comparator does,
    /// changing only as one runs.
    pub fn ticks(self) -> bool {
        matches!(
            self,
            Part::Repeater { .. } | Part::Torch | Part::Comparator { .. }
        )
    }

    /// The ticks it takes to change once its inputs do.
    pub fn delay(self) -> i32 {
        match self {
            Part::Repeater { delay, .. } => i32::from(delay),
            Part::Torch | Part::Comparator { .. } => 1,
            _ => 0,
        }
    }

    /// What it comes to as its tick runs, from `was`, its back powered at
    /// `back` and its sides at `side`: a repeater locked from a side keeps
    /// its state, and one off turns on whatever its back, as the game's
    /// does, the shortest pulse it gives being its delay.
    fn ticked(self, was: u8, back: u8, side: u8) -> u8 {
        match self {
            Part::Repeater { .. } if side > 0 => was,
            Part::Repeater { .. } if was == 0 => 15,
            Part::Repeater { .. } if back == 0 => 0,
            Part::Torch => 15 * u8::from(back == 0),
            Part::Comparator {
                subtract,
                far_input,
                ..
            } => compare(subtract, far_input, back, side),
            _ => was,
        }
    }

    /// Whether an update finds it should change from `was`, so schedules
    /// its tick.
    fn wants(self, was: u8, back: u8, side: u8) -> bool {
        match self {
            Part::Repeater { .. } => side == 0 && (back > 0) != (was > 0),
            Part::Torch => (back == 0) != (was > 0),
            Part::Comparator {
                subtract,
                far_input,
                ..
            } => compare(subtract, far_input, back, side) != was,
            _ => false,
        }
    }

    /// The priority its tick runs at, higher first, turning off or on, as
    /// Redpiler gives it after the game: a repeater facing a repeater or
    /// comparator first of all, then one turning off, then one turning on
    /// or a comparator facing one, then the rest.
    fn priority(self, turning_off: bool) -> u8 {
        match self {
            Part::Repeater {
                facing_diode: true, ..
            } => 3,
            Part::Repeater { .. } if turning_off => 2,
            Part::Repeater { .. }
            | Part::Comparator {
                facing_diode: true, ..
            } => 1,
            _ => 0,
        }
    }
}

/// What a comparator gives out, its back at `back` and its sides at
/// `side`.
fn compare(subtract: bool, far_input: Option<u8>, back: u8, side: u8) -> u8 {
    let back = match far_input {
        Some(far) if back < 15 => far,
        _ => back,
    };
    match back.checked_sub(side) {
        Some(difference) if subtract => difference,
        Some(_) => back,
        None => 0,
    }
}

/// A node of a build's circuit and the nodes that power it.
#[derive(Debug, Clone)]
pub struct Node {
    pub at: Pos,
    pub part: Part,
    /// The nodes powering its back, each with the strength lost on the
    /// way, through dust and blocks.
    pub back: Vec<(usize, u8)>,
    /// The nodes powering its sides: a repeater's lock, or a comparator's
    /// side input.
    pub side: Vec<(usize, u8)>,
}

/// A build's circuit as Redpiler compiles it: its components, levers, and
/// containers, by place, each with the nodes powering it.
#[derive(Debug, Clone)]
pub struct Graph {
    pub nodes: Vec<Node>,
}

impl Graph {
    /// The circuit of `blocks`.
    pub fn of(blocks: &Blocks) -> Result<Graph> {
        let compiled = circuit::compile_graph(blocks)?;
        let mut found = Vec::new();
        for i in compiled.node_indices() {
            let node = &compiled[i];
            let part = match node.ty {
                NodeType::Repeater {
                    delay,
                    facing_diode,
                } => Part::Repeater {
                    delay,
                    facing_diode,
                },
                NodeType::Torch => Part::Torch,
                NodeType::Comparator {
                    mode,
                    far_input,
                    facing_diode,
                } => Part::Comparator {
                    subtract: matches!(mode, ComparatorMode::Subtract),
                    far_input,
                    facing_diode,
                },
                NodeType::Lever => Part::Switch { button: false },
                NodeType::Button => Part::Switch { button: true },
                NodeType::Constant => {
                    Part::Constant(node.state.output_strength)
                }
                // Dust is followed through, as the links between the parts
                // it joins.
                NodeType::Wire => continue,
                NodeType::Lamp => Part::Other("lamp"),
                NodeType::PressurePlate => Part::Other("pressure plate"),
                NodeType::Trapdoor => Part::Other("trapdoor"),
                NodeType::NoteBlock { .. } => Part::Other("note block"),
            };
            if let Some(at) = circuit::node_place(node) {
                found.push((at, part, i));
            }
        }
        found.sort_by_key(|&(at, ..)| at);
        let index: HashMap<_, usize> = found
            .iter()
            .enumerate()
            .map(|(n, &(.., i))| (i, n))
            .collect();
        let mut links: BTreeMap<(usize, usize, bool), u8> = BTreeMap::new();
        for edge in compiled.all_edges() {
            let ends = (index.get(&edge.source()), index.get(&edge.target()));
            let (Some(&from), Some(&to)) = ends else {
                continue;
            };
            let link = edge.weight();
            let side = link.ty == LinkType::Side;
            let loss = links.entry((from, to, side)).or_insert(link.ss);
            *loss = (*loss).min(link.ss);
        }
        let mut nodes: Vec<Node> = found
            .into_iter()
            .map(|(at, part, _)| Node {
                at,
                part,
                back: Vec::new(),
                side: Vec::new(),
            })
            .collect();
        for ((from, to, side), loss) in links {
            let node = &mut nodes[to];
            if side { &mut node.side } else { &mut node.back }
                .push((from, loss));
        }
        Ok(Graph { nodes })
    }

    /// The node at `at`.
    pub fn find(&self, at: Pos) -> Option<usize> {
        self.nodes.binary_search_by_key(&at, |node| node.at).ok()
    }

    /// The nodes powering node `node`, its back's and then its sides'.
    pub fn sources(&self, node: usize) -> impl Iterator<Item = usize> + '_ {
        let node = &self.nodes[node];
        node.back.iter().chain(&node.side).map(|&(from, _)| from)
    }

    /// The strengths reaching node `node`'s back and sides, each node
    /// giving out what `strength` says.
    fn inputs(&self, node: usize, strength: impl Fn(usize) -> u8) -> (u8, u8) {
        let most = |links: &[(usize, u8)]| {
            links
                .iter()
                .map(|&(from, loss)| strength(from).saturating_sub(loss))
                .max()
                .unwrap_or(0)
        };
        let node = &self.nodes[node];
        (most(&node.back), most(&node.side))
    }

    /// What each node gives out in `circuit` as it stands.
    fn strengths(&self, circuit: &Circuit) -> Vec<u8> {
        self.nodes
            .iter()
            .map(|node| match node.part {
                Part::Constant(strength) => strength,
                Part::Other(_) => 0,
                _ => circuit.strength(node.at),
            })
            .collect()
    }
}

/// A change of a build's inputs traced tick by tick. Tick 0 is the instant
/// the first input switches, before any tick runs; each later one is as it
/// stands once that tick has run and any input switching on it has.
#[derive(Debug, Clone)]
pub struct Trace {
    /// Each input's value before the change, and after.
    pub from: Vec<Value>,
    pub to: Vec<Value>,
    /// The tick each input switches on.
    pub at: Vec<i32>,
    /// What each node gave out before the change, settled from `from`.
    pub before: Vec<u8>,
    /// What each node gives out at each tick.
    pub strengths: Vec<Vec<u8>>,
    /// Each output's value before the change.
    pub outputs_before: Vec<Value>,
    /// Each output's value at each tick, each bit as it arrives at its
    /// block, as the site times it: the ticks the repeater or comparator on
    /// its block takes to carry it out left out.
    pub outputs: Vec<Vec<Value>>,
}

/// One node changing what it gives out, on one tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Change {
    pub tick: i32,
    pub node: usize,
    pub was: u8,
    pub now: u8,
}

/// A race: a node running a tick that its rivals' updates, running at its
/// own priority on the same tick, may go before or after.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Race {
    pub tick: i32,
    pub node: usize,
    /// Its inputs that change on the tick, their updates at its priority.
    pub rivals: Vec<usize>,
    /// What it comes to with each set of its rivals gone first; for a
    /// pulse, whether it is set off to turn on.
    pub outcomes: Vec<(Vec<usize>, u8)>,
    /// Whether the race is not of its tick but of its being set off: a
    /// repeater that one order powers for an instant, so it turns on its
    /// delay later and gives a pulse, and another never does.
    pub pulse: bool,
    /// What it came to in this run; for a pulse, whether it turned on its
    /// delay later.
    pub ran: u8,
}

impl Trace {
    /// How many ticks it holds, from 0.
    pub fn ticks(&self) -> i32 {
        i32::try_from(self.strengths.len()).expect("a trace is short")
    }

    /// What node `node` gave out at `tick`, or before the change at -1.
    pub fn strength(&self, node: usize, tick: i32) -> u8 {
        match usize::try_from(tick) {
            Ok(tick) => self.strengths[tick][node],
            Err(_) => self.before[node],
        }
    }

    /// Whether node `node` changed on `tick`.
    pub fn changed(&self, node: usize, tick: i32) -> bool {
        self.strength(node, tick) != self.strength(node, tick - 1)
    }

    /// Output `port`'s value at `tick`, or before the change at -1.
    pub fn output(&self, port: usize, tick: i32) -> Value {
        match usize::try_from(tick) {
            Ok(tick) => self.outputs[tick][port],
            Err(_) => self.outputs_before[port],
        }
    }

    /// Each tick output `port` changed on, with its value from then.
    pub fn output_steps(&self, port: usize) -> Vec<(i32, Value)> {
        (0..self.ticks())
            .filter(|&tick| {
                self.output(port, tick) != self.output(port, tick - 1)
            })
            .map(|tick| (tick, self.output(port, tick)))
            .collect()
    }

    /// Each tick bit `bit` of output `port` changed on.
    pub fn bit_changes(&self, port: usize, bit: usize) -> Vec<i32> {
        let at = |tick| self.output(port, tick) >> bit & 1;
        (0..self.ticks()).filter(|&t| at(t) != at(t - 1)).collect()
    }

    /// Every change of every node, by tick, in the order of the graph.
    pub fn changes(&self) -> Vec<Change> {
        let nodes = self.before.len();
        (0..self.ticks())
            .flat_map(|tick| (0..nodes).map(move |node| (tick, node)))
            .filter(|&(tick, node)| self.changed(node, tick))
            .map(|(tick, node)| Change {
                tick,
                node,
                was: self.strength(node, tick - 1),
                now: self.strength(node, tick),
            })
            .collect()
    }

    /// The changes that set off `change`: those of its node's inputs on the
    /// latest tick, its own less its delay or earlier, on which any
    /// changed. None for a switch's, which the trace makes.
    pub fn causes(&self, graph: &Graph, change: &Change) -> Vec<Change> {
        let node = &graph.nodes[change.node];
        if !node.part.ticks() {
            return Vec::new();
        }
        let mut sources: Vec<usize> = graph
            .sources(change.node)
            .filter(|&s| s != change.node)
            .collect();
        sources.sort_unstable();
        sources.dedup();
        (0..=change.tick - node.part.delay())
            .rev()
            .map(|tick| {
                sources
                    .iter()
                    .filter(|&&s| self.changed(s, tick))
                    .map(|&s| Change {
                        tick,
                        node: s,
                        was: self.strength(s, tick - 1),
                        now: self.strength(s, tick),
                    })
                    .collect::<Vec<_>>()
            })
            .find(|found| !found.is_empty())
            .unwrap_or_default()
    }

    /// Whether node `node` runs a tick on `tick`: it changed then, or it
    /// wanted to as the tick its delay before ended and has not changed
    /// since.
    fn due(&self, graph: &Graph, node: usize, tick: i32) -> bool {
        if self.changed(node, tick) {
            return true;
        }
        let start = tick - graph.nodes[node].part.delay();
        if start < 0 || (start + 1..tick).any(|t| self.changed(node, t)) {
            return false;
        }
        let (back, side) = graph.inputs(node, |s| self.strength(s, start));
        graph.nodes[node]
            .part
            .wants(self.strength(node, start), back, side)
    }

    /// The priority of the tick in which node `node` changed on `tick`.
    fn priority_of(&self, graph: &Graph, node: usize, tick: i32) -> u8 {
        graph.nodes[node]
            .part
            .priority(self.strength(node, tick) == 0)
    }

    /// Every race in the trace, by tick; see [`Race`].
    pub fn races(&self, graph: &Graph) -> Vec<Race> {
        let mut races = Vec::new();
        for tick in 1..self.ticks() {
            for (node, part) in graph.nodes.iter().map(|n| n.part).enumerate() {
                if !part.ticks() {
                    continue;
                }
                // Its inputs that changed as this tick ran. A switch
                // switches between ticks, so none of theirs.
                let mut moved: Vec<usize> = graph
                    .sources(node)
                    .filter(|&s| {
                        s != node
                            && !matches!(
                                graph.nodes[s].part,
                                Part::Switch { .. }
                            )
                            && self.changed(s, tick)
                    })
                    .collect();
                moved.sort_unstable();
                moved.dedup();
                if moved.is_empty() {
                    continue;
                }
                let found = if self.due(graph, node, tick) {
                    self.tick_race(graph, node, tick, &moved)
                } else {
                    self.pulse_race(graph, node, tick, &moved)
                };
                races.extend(found);
            }
        }
        races
    }

    /// The race of node `node`'s own tick on `tick`, its inputs `moved`
    /// changing then: whether what it comes to depends on which of its
    /// rivals, those at its priority, go before it. Those at a higher
    /// priority always do, and those at a lower one never.
    fn tick_race(
        &self,
        graph: &Graph,
        node: usize,
        tick: i32,
        moved: &[usize],
    ) -> Option<Race> {
        let part = graph.nodes[node].part;
        let was = self.strength(node, tick - 1);
        let mine = part.priority(was > 0);
        let prior = |s: usize| self.priority_of(graph, s, tick);
        let first: Vec<usize> =
            moved.iter().copied().filter(|&s| prior(s) > mine).collect();
        let rivals: Vec<usize> = moved
            .iter()
            .copied()
            .filter(|&s| prior(s) == mine)
            .collect();
        if rivals.is_empty() || rivals.len() > 8 {
            return None;
        }
        let outcomes: Vec<(Vec<usize>, u8)> = (0..1u32 << rivals.len())
            .map(|mask| {
                let ahead: Vec<usize> = rivals
                    .iter()
                    .enumerate()
                    .filter(|&(i, _)| mask >> i & 1 == 1)
                    .map(|(_, &s)| s)
                    .collect();
                let (back, side) = graph.inputs(node, |s| {
                    let gone = first.contains(&s) || ahead.contains(&s);
                    self.strength(s, if gone { tick } else { tick - 1 })
                });
                (ahead, part.ticked(was, back, side))
            })
            .collect();
        let differs = outcomes.iter().any(|(_, o)| *o != outcomes[0].1);
        differs.then(|| Race {
            tick,
            node,
            rivals,
            outcomes,
            pulse: false,
            ran: self.strength(node, tick),
        })
    }

    /// The race of whether repeater `node`, off and not due on `tick`, is
    /// set off then: its inputs `moved` change on the tick, each group of
    /// a priority after those above it, in any order within it, and one
    /// order powers it for an instant, which schedules it to turn on, where
    /// another never does. When every order ends powering it, it turns on
    /// whatever the order, so it is no race.
    fn pulse_race(
        &self,
        graph: &Graph,
        node: usize,
        tick: i32,
        moved: &[usize],
    ) -> Option<Race> {
        let part = graph.nodes[node].part;
        if !matches!(part, Part::Repeater { .. })
            || self.strength(node, tick - 1) != 0
        {
            return None;
        }
        let powered = |gone: &[usize]| {
            let (back, side) = graph.inputs(node, |s| {
                self.strength(
                    s,
                    if gone.contains(&s) { tick } else { tick - 1 },
                )
            });
            back > 0 && side == 0
        };
        if powered(moved) {
            return None;
        }
        let mut groups: BTreeMap<u8, Vec<usize>> = BTreeMap::new();
        for &s in moved {
            groups
                .entry(self.priority_of(graph, s, tick))
                .or_default()
                .push(s);
        }
        if groups.values().any(|group| group.len() > 6) {
            return None;
        }
        let mut gone: Vec<usize> = Vec::new();
        let mut some_order_powers = false;
        let mut every_order_powers = false;
        let mut sets_off = Vec::new();
        for group in groups.values().rev() {
            // Each set of the group gone first is where some order stands
            // partway through it.
            for mask in 1..1u32 << group.len() {
                let ahead: Vec<usize> = group
                    .iter()
                    .enumerate()
                    .filter(|&(i, _)| mask >> i & 1 == 1)
                    .map(|(_, &s)| s)
                    .collect();
                let state: Vec<usize> =
                    gone.iter().chain(&ahead).copied().collect();
                if powered(&state) {
                    some_order_powers = true;
                    if sets_off.is_empty() {
                        sets_off = ahead;
                    }
                }
            }
            if !clean_order(group, &gone, &powered) {
                every_order_powers = true;
            }
            gone.extend(group);
        }
        if !some_order_powers || every_order_powers {
            return None;
        }
        let rivals: Vec<usize> = moved.to_vec();
        let later = tick + part.delay();
        let turned_on = later < self.ticks() && self.strength(node, later) > 0;
        Some(Race {
            tick,
            node,
            rivals,
            outcomes: vec![(Vec::new(), 0), (sets_off, 15)],
            pulse: true,
            ran: 15 * u8::from(turned_on),
        })
    }
}

/// Whether `group` can go in some order, after `gone`, without `powered`
/// holding partway through.
fn clean_order(
    group: &[usize],
    gone: &[usize],
    powered: &impl Fn(&[usize]) -> bool,
) -> bool {
    if group.is_empty() {
        return true;
    }
    group.iter().enumerate().any(|(i, &s)| {
        let mut now: Vec<usize> = gone.to_vec();
        now.push(s);
        let rest: Vec<usize> = group
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != i)
            .map(|(_, &r)| r)
            .collect();
        // Once the whole group has gone the state is the same whatever the
        // order, so only partway matters.
        (rest.is_empty() || !powered(&now)) && clean_order(&rest, &now, powered)
    })
}

/// A build set up to be traced: its circuit, its lines, and the simulator
/// running it.
pub struct Tracer {
    pub graph: Graph,
    pub interface: Interface,
    circuit: Circuit,
    carried: Carried,
}

impl Tracer {
    /// Sets up `blocks`, whose lines are `interface`.
    pub fn new(blocks: &Blocks, interface: Interface) -> Result<Tracer> {
        Self::with(blocks, interface, Circuit::new)
    }

    /// Sets up `blocks` to run in the world itself, block by block, its
    /// races going the way the game's block updates take them; see
    /// [`Circuit::in_world`].
    pub fn in_world(blocks: &Blocks, interface: Interface) -> Result<Tracer> {
        Self::with(blocks, interface, Circuit::in_world)
    }

    fn with(
        blocks: &Blocks,
        interface: Interface,
        set_up: fn(&Blocks, &Interface) -> Result<Circuit>,
    ) -> Result<Tracer> {
        let circuit = set_up(blocks, &interface)?;
        let carried = circuit.carried();
        Ok(Tracer {
            graph: Graph::of(blocks)?,
            interface,
            circuit,
            carried,
        })
    }

    /// The width of each input, in bits.
    pub fn input_widths(&self) -> Vec<usize> {
        self.interface.inputs.iter().map(input_bits).collect()
    }

    /// The width of each output, in bits.
    pub fn output_widths(&self) -> Vec<usize> {
        self.interface
            .outputs
            .iter()
            .map(|port| output_bits(&self.interface, port))
            .collect()
    }

    /// Traces the inputs changing from `from` to `to`, each switching on
    /// its tick of `at`, until nothing is left to change.
    pub fn run(
        &mut self,
        from: &[Value],
        to: &[Value],
        at: &[i32],
    ) -> Result<Trace> {
        for (port, &value) in from.iter().enumerate() {
            self.circuit.set(port, value);
        }
        self.circuit.settle()?;
        let before = self.graph.strengths(&self.circuit);
        let outputs_before = self.read();
        let last = at.iter().copied().max().unwrap_or(0);
        let mut strengths = Vec::new();
        let mut reads = Vec::new();
        for tick in 0.. {
            if tick > 0 {
                self.circuit.tick();
            }
            for (port, &value) in to.iter().enumerate() {
                if at[port] == tick {
                    self.circuit.set(port, value);
                }
            }
            self.circuit.show();
            strengths.push(self.graph.strengths(&self.circuit));
            reads.push(self.read());
            if tick >= last && !self.circuit.pending() {
                break;
            }
            if tick == i32::try_from(MAX_TICKS)? {
                bail!("It was still changing {MAX_TICKS} ticks after.");
            }
        }
        Ok(Trace {
            from: from.to_vec(),
            to: to.to_vec(),
            at: at.to_vec(),
            before,
            strengths,
            outputs_before,
            outputs: self.arrived(&reads),
        })
    }

    /// Each output's value as read at each tick, from the component on each
    /// bit's block, moved back to when it arrived at the block.
    fn arrived(&self, reads: &[Vec<Value>]) -> Vec<Vec<Value>> {
        let widths = self.output_widths();
        (0..reads.len())
            .map(|tick| {
                widths
                    .iter()
                    .enumerate()
                    .map(|(port, &width)| {
                        (0..width).fold(0, |value, bit| {
                            let carried = self.carried.bit(port, bit);
                            let read = (tick
                                + usize::try_from(carried).unwrap_or(0))
                            .min(reads.len() - 1);
                            value | reads[read][port] & 1 << bit
                        })
                    })
                    .collect()
            })
            .collect()
    }

    fn read(&self) -> Vec<Value> {
        (0..self.interface.outputs.len())
            .map(|port| self.circuit.get(port))
            .collect()
    }
}

/// One change of a build's inputs to sweep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    pub from: Vec<Value>,
    pub to: Vec<Value>,
}

/// Many changes of a build's inputs traced, every input switching at once.
#[derive(Debug, Clone, Default)]
pub struct Sweep {
    /// How many changes it ran.
    pub cases: usize,
    /// For each output, how it settled.
    pub outputs: Vec<Settling>,
    /// Each race coming first in a change, by its node and rivals, with how
    /// many changes it came up in and the first, most often first.
    pub races: Vec<Seen>,
}

/// How one output settled over a sweep.
#[derive(Debug, Clone, Default)]
pub struct Settling {
    /// How many of the changes it changed in it settled by each tick in.
    pub settled: BTreeMap<i32, usize>,
    /// Its slowest change, and the tick it settled by.
    pub slowest: Option<(i32, Case)>,
    /// Each bit that glitched, changing more than once in a change, with
    /// how many changes it did in and the first.
    pub glitches: BTreeMap<usize, (usize, Case)>,
}

/// A race as a sweep came on it.
#[derive(Debug, Clone)]
pub struct Seen {
    /// The first time it came up.
    pub race: Race,
    pub case: Case,
    /// How many changes it came up in.
    pub count: usize,
}

/// The changes to sweep for a build whose inputs are `widths` bits wide:
/// each input alone from all off to each of its values, or to its test
/// patterns when it is wider than 10 bits, and `random` changes from one
/// set of values to another at random, the same each time.
pub fn cases(widths: &[usize], random: usize) -> Vec<Case> {
    let zero = vec![0; widths.len()];
    let mut cases = Vec::new();
    for (port, &width) in widths.iter().enumerate() {
        let values: Vec<Value> = if width <= 10 {
            (1..1 << width).collect()
        } else {
            bench::patterns(width)
                .into_iter()
                .filter(|&v| v != 0)
                .collect()
        };
        for value in values {
            let mut to = zero.clone();
            to[port] = value;
            cases.push(Case {
                from: zero.clone(),
                to,
            });
        }
    }
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = |width: usize| {
        let mut draw = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let value = Value::from(draw()) << 64 | Value::from(draw());
        value & bench::mask(width)
    };
    for _ in 0..random {
        let from = widths.iter().map(|&w| next(w)).collect();
        let to = widths.iter().map(|&w| next(w)).collect();
        cases.push(Case { from, to });
    }
    cases
}

/// Traces each of `cases` through `tracer`, every input switching at tick
/// 0, until `until` passes, and gathers how each output settles and the
/// races that come first in each change: a later one, of a node a race
/// before set changing, is likely its doing.
pub fn sweep(
    tracer: &mut Tracer,
    cases: &[Case],
    until: Option<Instant>,
) -> Result<Sweep> {
    let widths = tracer.output_widths();
    let mut sweep = Sweep {
        cases: 0,
        outputs: vec![Settling::default(); widths.len()],
        races: Vec::new(),
    };
    let mut seen: BTreeMap<(usize, Vec<usize>, bool), usize> = BTreeMap::new();
    for case in cases {
        if until.is_some_and(|until| Instant::now() > until) {
            break;
        }
        sweep.cases += 1;
        let at = vec![0; case.to.len()];
        let trace = tracer.run(&case.from, &case.to, &at)?;
        for (port, &width) in widths.iter().enumerate() {
            let settling = &mut sweep.outputs[port];
            let Some(&(settled, _)) = trace.output_steps(port).last() else {
                continue;
            };
            *settling.settled.entry(settled).or_default() += 1;
            if settling.slowest.as_ref().is_none_or(|(s, _)| settled > *s) {
                settling.slowest = Some((settled, case.clone()));
            }
            for bit in 0..width {
                if trace.bit_changes(port, bit).len() > 1 {
                    settling
                        .glitches
                        .entry(bit)
                        .or_insert_with(|| (0, case.clone()))
                        .0 += 1;
                }
            }
        }
        let races = trace.races(&tracer.graph);
        let first = races.first().map_or(0, |race| race.tick);
        let mut counted = Vec::new();
        for race in races.into_iter().take_while(|race| race.tick == first) {
            let key = (race.node, race.rivals.clone(), race.pulse);
            if counted.contains(&key) {
                continue;
            }
            counted.push(key.clone());
            if let Some(&i) = seen.get(&key) {
                sweep.races[i].count += 1;
            } else {
                seen.insert(key, sweep.races.len());
                sweep.races.push(Seen {
                    race,
                    case: case.clone(),
                    count: 1,
                });
            }
        }
    }
    sweep
        .races
        .sort_by_key(|seen| std::cmp::Reverse(seen.count));
    Ok(sweep)
}
