//! The logic analyzer: a change of a build's inputs traced tick by tick,
//! what set each change off, and the races it finds, whichever way the
//! simulator's own run of them went.

mod common;

use app::{
    category::Kind,
    import,
    schem::{Blocks, Pos},
    simulate::trace::{self, Tracer},
};

/// Fixture adder `name` set up to be traced, its lines named as its
/// timing names them: A, B, and carry in.
fn tracer(name: &str) -> Tracer {
    let data = import::with_roles(&common::fixture(name), "adder").unwrap();
    let blocks = Blocks::read(&data).unwrap();
    let lines = Kind::Adder.test().unwrap().lines(&blocks).unwrap();
    Tracer::new(&blocks, lines).unwrap()
}

fn at(tracer: &Tracer, node: usize) -> Pos {
    tracer.graph.nodes[node].at
}

/// This 10-bit carry-cancel adder's line NAND(A0, B0) is held up at rest by
/// a torch inverting B0, and once B0 turns on by a comparator that turns
/// on only as that torch's power leaves the line, a tick later. So as B0
/// turns on, the torch reading the line, its generate, runs its tick on
/// the tick the comparator turns on: first, it lights for a tick and bit
/// 1 glitches; after, it stays out. The race is found from either run, on
/// each fresh compile, and the outputs go as the race did.
#[test]
fn a_hand_off_with_a_gap_races() {
    for _ in 0..4 {
        let mut tracer = tracer("cca-10-bit-3-carry-lost");
        let run = tracer.run(&[0, 0, 0], &[0, 1, 0], &[0, 0, 0]).unwrap();
        let races = run.races(&tracer.graph);
        let race = &races[0];
        assert_eq!((race.tick, at(&tracer, race.node)), (2, [9, 3, 4]));
        let rivals: Vec<Pos> =
            race.rivals.iter().map(|&r| at(&tracer, r)).collect();
        assert_eq!(rivals, [[4, 2, 3]]);
        let outcomes: Vec<(usize, u8)> = race
            .outcomes
            .iter()
            .map(|(ahead, outcome)| (ahead.len(), *outcome))
            .collect();
        assert_eq!(outcomes, [(0, 15), (1, 0)]);
        let steps = run.output_steps(0);
        if race.ran == 15 {
            assert_eq!(steps, [(3, 3), (4, 1)]);
            assert_eq!(run.bit_changes(0, 1), [3, 4]);
        } else {
            assert_eq!(steps, [(3, 1)]);
        }
    }
}

/// What set a change off is the change of what powers it, its delay
/// before: the comparator taking over the line turns on on tick 2, as the
/// torch inverting B0 goes out on tick 1, which B0's lever set off.
#[test]
fn a_change_is_set_off_by_what_powers_it() {
    let mut tracer = tracer("cca-10-bit-3-carry-lost");
    let run = tracer.run(&[0, 0, 0], &[0, 1, 0], &[0, 0, 0]).unwrap();
    let comparator = tracer.graph.find([4, 2, 3]).unwrap();
    let change = run
        .changes()
        .into_iter()
        .find(|change| change.node == comparator)
        .unwrap();
    assert_eq!((change.tick, change.was, change.now), (2, 0, 13));
    let causes = run.causes(&tracer.graph, &change);
    let torch: Vec<(i32, Pos)> = causes
        .iter()
        .map(|c| (c.tick, at(&tracer, c.node)))
        .collect();
    assert_eq!(torch, [(1, [6, 1, 2])]);
    let lever = run.causes(&tracer.graph, &causes[0]);
    assert_eq!(lever.len(), 1);
    assert_eq!(lever[0].tick, 0);
}

/// An input switching later than the rest switches on its own tick.
#[test]
fn an_input_switches_on_its_tick() {
    let mut tracer = tracer("cca-10-bit-3-carry-lost");
    let run = tracer.run(&[0, 0, 0], &[1, 0, 0], &[2, 0, 0]).unwrap();
    // Sum bit 0 shows A0 three ticks after it switches.
    assert_eq!(run.output_steps(0), [(5, 1)]);
}

/// A sweep finds the race first in more changes than any other, and an
/// adder whose every line is driven one way has none.
#[test]
fn a_sweep_finds_the_races() {
    let mut racing = tracer("cca-10-bit-3-carry-lost");
    let cases = trace::cases(&racing.input_widths(), 200);
    let sweep = trace::sweep(&mut racing, &cases, None).unwrap();
    assert_eq!(sweep.cases, cases.len());
    assert_eq!(at(&racing, sweep.races[0].race.node), [9, 3, 4]);
    assert!(sweep.outputs[0].settled.keys().all(|&tick| tick <= 4));

    let mut clean = tracer("adder-8-lsb-top");
    let cases = trace::cases(&clean.input_widths(), 200);
    let sweep = trace::sweep(&mut clean, &cases, None).unwrap();
    assert!(sweep.races.is_empty());
    assert!(sweep.outputs[0].glitches.is_empty());
}
