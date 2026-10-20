//! The one way the site reads a build: what it is and how it performs,
//! worked out from its file alone, trusting nothing stored. The test that
//! recognizes it names its category; that test's cases say whether it
//! works; its copies, stacked, say how it stacks; its timing gives its
//! latency, whether its outputs change together, and its throughput; and
//! for an adder, how its carries are made gives its design.

use std::sync::atomic::AtomicBool;

use anyhow::Result;

use crate::{
    import::{self, Prepared},
    interface,
    models::{
        schematic::{Encoding, Logic, Orientation, StackDirection},
        simulation::XNOR_FLAG,
    },
    simulate::{
        self, Design, Report, Stacking, Test, adder_design, throughput,
    },
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
    /// right, no fewer than torch burnout allows.
    pub throughput: Option<i64>,
    /// How an adder makes its carries.
    pub design: Option<Design>,
    /// Whether it holds state: whether what its test leaves on its outputs
    /// differs from a fresh copy given the same inputs.
    pub logic: Option<Logic>,
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
        design: None,
        logic: None,
        prepared,
    };
    let Some(test) = category.and_then(|c| Test::of(c, found.width)) else {
        return Ok(found);
    };
    let mut report = test.run(&found.prepared.blocks);
    if let Report::Ran(run) = &mut report
        && run.wrong == 0
    {
        let never = category.is_some_and(|c| {
            crate::category::behaviour(c).stacking
                == crate::category::Stacking::Never
        });
        run.stacking = if never {
            None
        } else {
            test.stacking(&found.prepared.data)
        };
        // Copies that work run the way its bits do, which a 1-bit build's
        // lone input cannot show; a gate's work only up to so many.
        let stepped = match &run.stacking {
            Some(Stacking::Stacks { step, .. }) => Some((step, None)),
            Some(Stacking::Wider { step, most, .. }) => Some((step, *most)),
            _ => None,
        };
        if let Some((step, most)) = stepped {
            found.stacking = vec![StackDirection {
                max_copies: most.and_then(|n| i64::try_from(n).ok()),
                ..step.direction()
            }];
            if let Some(orientation) = step.orientation() {
                found.orientation = Some(orientation);
            }
        }
        // A gate's cases say whether it is an XNOR.
        if test == Test::XorXnor && run.pattern == simulate::XNOR {
            found.flags.push(XNOR_FLAG.to_owned());
        }
        let timing =
            test.time(&found.prepared.blocks, &AtomicBool::new(false))?;
        found.latency = timing.latency().map(|t| t.max(0).into());
        found.synchronized = Some(timing.synced);
        // A torch burns out switched too often, which the simulator does
        // not model: the game takes a new input no sooner than every third
        // tick through torches, or every second without.
        let burnout = if found.prepared.facts.torches > 0 {
            3
        } else {
            2
        };
        found.throughput = throughput(&found.prepared.blocks, &timing)?
            .map(|ticks| i64::from(ticks).max(burnout));
        run.timing = Some(timing);
        found.logic = test.remembers(&found.prepared.blocks)?.map(|held| {
            if held {
                Logic::Stateful
            } else {
                Logic::Stateless
            }
        });
        if category == Some("adder") {
            found.design = adder_design(&found.prepared.blocks)?;
        }
    }
    found.report = Some(report);
    Ok(found)
}
