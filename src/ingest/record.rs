//! A record of what recognition makes of a set of files, one line of JSON
//! each in a fixed order, so two records, taken before and after a change
//! to recognizing, testing, or laying builds the standard way, differ only
//! where that change does. The fixtures' record is kept with the tests,
//! which hold recognition to it; `rsdb record` takes one of the stored
//! builds too.

use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use anyhow::Result;
use serde::Serialize;

use crate::{
    category::Test, ingest::prepare::Upload, interface, recognize::probe,
    schem::Blocks, simulate::Report,
};

/// One file to record: its name in the record, its bytes, and for a
/// stored build, its category and width, which its own test runs as.
pub struct Subject {
    pub name: String,
    pub data: Vec<u8>,
    pub stored: Option<(String, Option<i64>)>,
}

/// What recognition made of one file: as an upload is recognized, laid
/// the standard way, and read; and for a stored build, its own test on its
/// stored file.
#[derive(Serialize)]
pub struct Line {
    name: String,
    /// What probing found it to be, or `None`.
    found: Option<String>,
    /// What keeps it from being recognized, if anything.
    problems: Vec<String>,
    /// How its standard file was laid: mirrored, then quarter turns; or
    /// why there is no standard file.
    laid: Result<(bool, u8), String>,
    /// Each input and output of its standard file: its name, role, and
    /// bits from the lowest, by position.
    lines: Vec<(String, Option<String>, Vec<[i32; 3]>)>,
    /// A stored build's test on its stored file: right or wrong cases, or
    /// why it didn't run.
    stored_test: Option<String>,
}

/// Every .schem file in `dir`, named by its file name.
pub fn fixtures(dir: &Path) -> Result<Vec<Subject>> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "schem") {
            let name = path.file_name().unwrap_or_default();
            found.push(Subject {
                name: name.to_string_lossy().into_owned(),
                data: fs::read(&path)?,
                stored: None,
            });
        }
    }
    Ok(found)
}

/// Each subject's line as JSON, in order of name, worked out on every core.
pub fn record(subjects: &[Subject]) -> Vec<String> {
    let next = AtomicUsize::new(0);
    let lines = Mutex::new(BTreeMap::new());
    let workers = std::thread::available_parallelism().map_or(4, usize::from);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                while let Some(subject) =
                    subjects.get(next.fetch_add(1, Ordering::Relaxed))
                {
                    let json = serde_json::to_string(&line(subject))
                        .unwrap_or_default();
                    let mut lines = lines.lock().unwrap();
                    lines.insert(subject.name.clone(), json);
                }
            });
        }
    });
    lines.into_inner().unwrap().into_values().collect()
}

/// What recognition makes of `subject`.
pub fn line(subject: &Subject) -> Line {
    let mut line = Line {
        name: subject.name.clone(),
        found: None,
        problems: Vec::new(),
        laid: Err(String::new()),
        lines: Vec::new(),
        stored_test: subject.stored.as_ref().map(|(category, width)| {
            stored_test(&subject.data, category, *width)
        }),
    };
    let recognition = match probe::recognize(&subject.data) {
        Ok(recognition) => recognition,
        Err(err) => {
            line.problems.push(format!("{err:#}"));
            return line;
        }
    };
    line.found = recognition.found;
    line.problems = recognition
        .problems
        .into_iter()
        .map(|p| p.message)
        .collect();
    let standard = match Upload::read(&recognition.file) {
        Ok(upload) => upload.standard,
        Err(err) => Err(format!("{err:#}")),
    };
    match standard {
        Ok(standard) => {
            line.laid = Ok((standard.turn.mirrored, standard.turn.quarters));
            let read = interface::read(&standard.blocks);
            line.lines = read
                .inputs
                .iter()
                .chain(&read.outputs)
                .map(|port| {
                    (
                        port.name.clone(),
                        port.role.map(crate::schem::lines::Role::key),
                        port.bits.iter().map(|bit| bit.pos).collect(),
                    )
                })
                .collect();
        }
        Err(why) => line.laid = Err(why),
    }
    line
}

/// A stored build's own test on its stored file, in short.
fn stored_test(data: &[u8], category: &str, width: Option<i64>) -> String {
    let Some(test) = Test::of(category, width) else {
        return "no test".to_owned();
    };
    let blocks = match Blocks::read(data) {
        Ok(blocks) => blocks,
        Err(err) => return format!("unreadable: {err:#}"),
    };
    match test.run(&blocks) {
        Report::Ran(run) => {
            format!("{} of {} wrong: {}", run.wrong, run.cases, run.pattern)
        }
        other => format!("{other:?}"),
    }
}
