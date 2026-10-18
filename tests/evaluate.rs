//! Reading a build from its file alone.

use app::{evaluate::evaluate, standardize};

/// A fixture as the site stores it.
fn stored(name: &str) -> Vec<u8> {
    let raw = std::fs::read(format!("tests/fixtures/{name}.schem"))
        .expect("fixture is readable");
    let tidied = standardize::tidy(&raw).expect("fixture tidies");
    standardize::pink_status(&tidied).expect("fixture recolours")
}

#[test]
fn a_ripple_adder_slice_is_read_whole_from_its_file() {
    let e = evaluate(&stored("rca-slice-capped")).unwrap();
    assert_eq!(e.category, Some("adder"));
    assert!(e.passes());
    assert_eq!(e.width, Some(1));
    assert_eq!(e.flags, ["carry_in", "carry_out"]);
    let latency = e.latency.unwrap();
    let throughput = e.throughput.unwrap();
    assert!((1..=2 * latency.max(1)).contains(&throughput));
}

#[test]
fn a_build_failing_its_test_has_no_timing() {
    let e = evaluate(&stored("cca-8-dust-out")).unwrap();
    assert_eq!(e.category, Some("adder"));
    assert!(!e.passes());
    assert_eq!((e.latency, e.throughput), (None, None));
}
