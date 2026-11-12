//! A change of a build's inputs traced tick by tick, and the races it
//! finds, whichever way the simulator's own run of them went.

mod common;

use app::{
    category::Kind,
    import,
    recognize::interface::Interface,
    schem::{Blocks, Pos},
    simulate::trace::Tracer,
};

/// Fixture adder `name` set up to be traced by `set_up`, its lines named
/// as its timing names them: A, B, and carry in.
fn traced(
    name: &str,
    set_up: fn(&Blocks, Interface) -> anyhow::Result<Tracer>,
) -> Tracer {
    let data = import::with_roles(&common::fixture(name), "adder").unwrap();
    let blocks = Blocks::read(&data).unwrap();
    let lines = Kind::Adder.test().unwrap().lines(&blocks).unwrap();
    set_up(&blocks, lines).unwrap()
}

/// Fixture adder `name` compiled by Redpiler.
fn tracer(name: &str) -> Tracer {
    traced(name, Tracer::new)
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

/// An input switching later than the rest switches on its own tick.
#[test]
fn an_input_switches_on_its_tick() {
    let mut tracer = tracer("cca-10-bit-3-carry-lost");
    let run = tracer.run(&[0, 0, 0], &[1, 0, 0], &[2, 0, 0]).unwrap();
    // Sum bit 0 shows A0 three ticks after it switches.
    assert_eq!(run.output_steps(0), [(5, 1)]);
}

/// Block by block, as the game orders a tick's ticks, the comparator
/// taking over the line hears it fall as soon as the dust beside it does,
/// long before the generate torch hears it reach nothing, so it is
/// scheduled first and goes first: the torch never lights, and the sum is 1
/// on tick 3. The race is still found, the tie being in the circuit, but
/// it goes one way only, as in game; only Redpiler's own order glitches.
#[test]
fn block_by_block_the_hand_off_goes_first() {
    let mut world = traced("cca-10-bit-3-carry-lost", Tracer::in_world);
    let run = world.run(&[0, 0, 0], &[0, 1, 0], &[0, 0, 0]).unwrap();
    assert_eq!(run.output_steps(0), [(3, 1)]);
    let races = run.races(&world.graph);
    let race = &races[0];
    assert_eq!(
        (race.tick, at(&world, race.node), race.ran),
        (2, [9, 3, 4], 0)
    );
}
