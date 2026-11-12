//! Where each spec on a build's page comes from: measured by the
//! simulation, read from the file, or typed by its uploader, so the page
//! can mark what the site checked.

use crate::{
    build::Build,
    category,
    ingest::read,
    interface::Interface,
    simulate::{Report, Timing},
};

/// Where a spec on a build's page comes from, so a reader can tell what the
/// site checked from what its uploader typed.
#[derive(Clone, Copy)]
pub(crate) enum Source {
    /// Measured or confirmed by the simulation.
    Simulated,
    /// Read from the file.
    File,
    /// As typed, unchecked.
    Given,
    /// Typed, and the simulation did not bear it out, as it says.
    Unconfirmed(&'static str),
}

/// Where each spec that can be checked on build `s`'s page comes from.
pub(crate) struct Sources {
    pub(crate) category: Source,
    pub(crate) width: Source,
    pub(crate) orientation: Source,
    /// Latency and whether it is synchronized, measured together.
    pub(crate) timing: Source,
    /// A monostable's pulse and whether it is inverted, measured with its
    /// timing.
    pub(crate) pulse: Source,
    pub(crate) stacking: Source,
    pub(crate) throughput: Source,
    /// Whether it holds state.
    pub(crate) logic: Source,
    /// Whether every spec was measured or read from the file, as for a
    /// recognized build of a category whose every detail `evaluate` works
    /// out, once it passes: the page then says so once, not spec by spec.
    pub(crate) verified: bool,
}

impl Sources {
    /// Build `s`'s sources, with its file's `interface` if it could be
    /// read and its simulation's `report` if it has one.
    pub(crate) fn of(
        s: &Build,
        interface: Option<&Interface>,
        report: Option<&Report>,
    ) -> Self {
        let passed = report.is_some_and(Report::passed);
        // A conventional build's file sets these over what was typed.
        let derived = interface
            .filter(|_| s.file.conventional)
            .map(|interface| read::derived(&s.category, interface))
            .unwrap_or_default();
        let checked = |from_file: bool, simulated: bool| {
            if from_file {
                Source::File
            } else if simulated {
                Source::Simulated
            } else {
                Source::Given
            }
        };
        let run = match report {
            Some(Report::Ran(run)) => Some(run),
            _ => None,
        };
        let stacking = if stacking_found(s)
            && run.is_some_and(|run| run.stacking.is_some())
        {
            Source::Simulated
        } else {
            Source::Given
        };
        let category_source = if report.is_some_and(Report::failed) {
            Source::Unconfirmed("failing")
        } else if !s.verified() {
            // It is untested as its category until recognized.
            Source::Unconfirmed("untested")
        } else {
            checked(false, passed)
        };
        if passed
            && matches!(category_source, Source::Simulated)
            && s.evaluated()
        {
            return Sources::every(Source::Simulated);
        }
        Sources {
            category: category_source,
            width: checked(
                derived.width == s.specs.width && s.specs.width.is_some(),
                passed,
            ),
            orientation: checked(
                derived.orientation == Some(s.specs.orientation),
                false,
            ),
            timing: checked(
                false,
                report
                    .and_then(Report::timing)
                    .and_then(Timing::latency)
                    .is_some(),
            ),
            pulse: checked(
                false,
                report
                    .and_then(Report::timing)
                    .is_some_and(|timing| timing.pulse.is_some()),
            ),
            stacking,
            throughput: Source::Given,
            logic: Source::Given,
            verified: false,
        }
    }

    /// Whether the site checked any spec on the page.
    pub(crate) fn some_checked(&self) -> bool {
        [
            self.category,
            self.width,
            self.orientation,
            self.timing,
            self.pulse,
            self.stacking,
        ]
        .iter()
        .any(|source| matches!(source, Source::Simulated | Source::File))
    }

    /// Every spec from `source`, all verified.
    fn every(source: Source) -> Self {
        Sources {
            category: source,
            width: source,
            orientation: source,
            timing: source,
            pulse: source,
            stacking: source,
            throughput: source,
            logic: source,
            verified: true,
        }
    }
}

/// Whether the simulation decides how build `s` stacks, rather than its
/// author giving it: a conventional build of a category whose stacking
/// it infers.
pub(crate) fn stacking_found(s: &Build) -> bool {
    s.file.conventional
        && category::behaviour(&s.category).stacking
            == category::Stacking::Inferred
}
