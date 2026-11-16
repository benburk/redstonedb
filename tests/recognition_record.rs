//! Recognition held to its record: what each fixture is recognized as, how
//! it is laid the standard way, and where each of its lines' bits lies, as
//! `tests/recognition.jsonl` keeps them. A change meant to alter any of it
//! records them again with `just record`, the diff showing what changed.

use std::path::Path;

use app::ingest::record;

#[test]
fn every_fixture_is_recognized_as_recorded() {
    let subjects = record::fixtures(Path::new("tests/fixtures")).unwrap();
    let lines = record::record(&subjects);
    let kept = std::fs::read_to_string("tests/recognition.jsonl")
        .expect("tests/recognition.jsonl is missing: run `just record`");
    let kept: Vec<&str> = kept.lines().collect();
    let short = |line: &str| line.chars().take(200).collect::<String>();
    let now: Vec<String> = lines
        .iter()
        .filter(|line| !kept.contains(&line.as_str()))
        .map(|line| format!("+ {}", short(line)))
        .collect();
    let was: Vec<String> = kept
        .iter()
        .filter(|line| !lines.iter().any(|now| now == *line))
        .map(|line| format!("- {}", short(line)))
        .collect();
    assert!(
        now.is_empty() && was.is_empty(),
        "Fixtures are recognized differently from tests/recognition.jsonl; \
         if that is meant, run `just record` and check the diff:\n{}\n{}",
        was.join("\n"),
        now.join("\n")
    );
}
