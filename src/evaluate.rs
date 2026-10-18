//! The one way the site reads a build: what it is and how it performs,
//! worked out from its file alone, trusting nothing stored. The test that
//! recognizes it names its category; that test's cases say whether it
//! works; its copies, stacked, say how it stacks; and its timing gives its
//! latency, whether its outputs change together, and its throughput.

use std::sync::atomic::AtomicBool;

use anyhow::Result;

use crate::{
    import::{self, Prepared},
    interface,
    models::schematic::{Encoding, Orientation, StackDirection},
    simulate::{self, Report, Stacking, Test, throughput},
};

/// Everything worked out from a build's file.
pub struct Evaluation {
    /// The file as the site keeps it.
    pub prepared: Prepared,
    /// What its test recognizes it as; `None` when no test fits it.
    pub category: Option<&'static str>,
    pub width: Option<i64>,
    pub orientation: Option<Orientation>,
    pub encoding: Option<Encoding>,
    /// The flags its file shows, such as an adder's `carry_in`.
    pub flags: Vec<String>,
    /// Its test's result, with its timing and how its copies stack when
    /// every case came out right.
    pub report: Option<Report>,
    pub stacking: Vec<StackDirection>,
    pub latency: Option<i64>,
    pub synchronized: Option<bool>,
    /// The fewest ticks from one input to the next that keep every result
    /// right.
    pub throughput: Option<i64>,
}

impl Evaluation {
    /// Whether its test ran and every case came out right.
    pub fn passes(&self) -> bool {
        matches!(&self.report, Some(Report::Ran(run)) if run.wrong == 0)
    }
}

/// Reads the build in `file`.
pub fn evaluate(file: &[u8]) -> Result<Evaluation> {
    let prepared = import::prepare(file, None, true, true, false)?;
    let blocks = &prepared.blocks;
    let category = simulate::guess(blocks).map(|guess| guess.category);
    let interface = interface::read(blocks);
    let read = category
        .map(|c| import::derived(c, &interface))
        .unwrap_or_default();
    let mut found = Evaluation {
        category,
        width: read.width,
        orientation: read.orientation,
        encoding: read.encoding,
        flags: category
            .and_then(|c| import::derived_flags(c, &interface))
            .unwrap_or_default(),
        report: None,
        stacking: Vec::new(),
        latency: None,
        synchronized: None,
        throughput: None,
        prepared,
    };
    let Some(test) = category.and_then(|c| Test::of(c, found.width)) else {
        return Ok(found);
    };
    let mut report = test.run(&found.prepared.blocks);
    if let Report::Ran(run) = &mut report
        && run.wrong == 0
    {
        run.stacking = test.stacking(&found.prepared.data);
        if let Some(Stacking::Stacks { step, .. }) = &run.stacking {
            found.stacking = vec![step.direction()];
            if let Some(orientation) = step.orientation() {
                found.orientation = Some(orientation);
            }
        }
        let timing =
            test.time(&found.prepared.blocks, &AtomicBool::new(false))?;
        found.latency = timing.latency().map(|t| t.max(0).into());
        found.synchronized = Some(timing.synced);
        found.throughput =
            throughput(&found.prepared.blocks, &timing)?.map(i64::from);
        run.timing = Some(timing);
    }
    found.report = Some(report);
    Ok(found)
}
