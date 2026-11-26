//! The time the site gives a build: run out, recognizing it says so and
//! testing it is skipped, rather than either failing or calling it wrong.

use std::time::Duration;

use app::{
    category::Test,
    interface::problem::Rule,
    recognize::probe,
    schem::Blocks,
    simulate::{Report, budget},
};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("tests/fixtures/{name}.schem")).unwrap()
}

#[test]
fn a_build_out_of_time_is_recognized_as_none_saying_so() {
    let data = fixture("cca-8-repeater-out");
    let recognition =
        budget::within(Duration::ZERO, || probe::recognize(&data)).unwrap();
    assert!(recognition.found.is_none());
    assert!(
        recognition
            .problems
            .iter()
            .any(|problem| problem.rule == Rule::TookTooLong),
        "{:?}",
        recognition.problems
    );
    // Given the time, it is an adder.
    let recognition = probe::recognize(&data).unwrap();
    assert!(recognition.found.is_some());
}

#[test]
fn a_test_out_of_time_is_skipped_not_wrong() {
    let data = fixture("cca-8-repeater-out");
    let file = probe::recognize(&data).unwrap().file;
    let blocks = Blocks::read(&file).unwrap();
    let report = budget::within(Duration::ZERO, || Test::Adder.run(&blocks));
    let Report::Skipped { reason } = report else {
        panic!("{report:?}");
    };
    assert!(reason.contains("Took too long to simulate"), "{reason}");
    assert!(Test::Adder.run(&blocks).passed());
}
