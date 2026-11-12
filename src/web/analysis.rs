//! An upload's analysis, or a new version's check, run in the background
//! so choosing a file answers at once and its page shows how far it has
//! got: recognizing a file probes it as each standard category in turn, and
//! checking a version probes it as its build's, which takes seconds for a
//! large build.

use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use uuid::Uuid;

use crate::{
    category::{self, Guessed, Kind},
    import::{self, Upload},
    models::{simulation, user::User},
    recognize::probe::{self, Recognition},
    version::Checked,
    web::app_state::AppState,
};

/// Analyses under way or done, by id, each kept a while for its page to
/// be reloaded.
pub type Analyses = Arc<Mutex<HashMap<Uuid, Analysis>>>;

/// How long a finished analysis stays, for its preview to be reloaded.
const KEPT: Duration = Duration::from_hours(1);

/// The most analyses kept at once; the oldest go first.
const MOST: usize = 200;

/// One upload's analysis, or one version's check: whose it is, how far it
/// has got, and, once it is done, what it found.
pub struct Analysis {
    user: Uuid,
    started: Instant,
    step: Step,
    done: Option<Done>,
}

/// What an upload's analysis found, or why the file could not be read.
pub type Outcome = Arc<Result<Analyzed, String>>;

/// What an analysis ends with: an upload's, or a new version's check.
#[derive(Clone)]
pub enum Done {
    Upload(Outcome),
    Version(Arc<VersionChecked>),
}

/// A file checked as build `schematic`'s next version, or why it cannot be
/// one, with the edit form as it was sent beside it, to show again.
pub struct VersionChecked {
    pub schematic: String,
    pub checked: Result<Checked, String>,
    pub pairs: Vec<(String, String)>,
}

/// How far an analysis has got.
#[derive(Clone)]
pub enum Step {
    /// Finding its inputs and outputs.
    Inspecting,
    /// Probing it as the category `label`, after `tried` others.
    Testing { label: String, tried: usize },
    /// Reading it as built and as standardized, and guessing what it is.
    Preparing,
    /// Checking a file as a build's next version.
    Checking,
}

impl Step {
    /// Its place among the steps the page lists, after uploading the file.
    pub fn place(&self) -> usize {
        match self {
            Step::Inspecting | Step::Checking => 1,
            Step::Testing { .. } => 2,
            Step::Preparing => 3,
        }
    }
}

/// What an analysis found: the file read, what it seems to be, and what
/// the check by function made of it.
pub struct Analyzed {
    /// The file as its author made it, for the colours they gave it.
    pub raw: Vec<u8>,
    /// The details form as sent, for a file chosen in another's place on
    /// a preview, to keep what was filled in.
    pub sent: Option<Vec<(String, String)>>,
    pub upload: Upload,
    pub guess: Option<Guessed>,
    pub recognition: Recognition,
}

/// Keeps a new analysis for `user`, at `step`, the oldest and those long
/// done making room, and returns its id.
fn register(state: &AppState, user: &User, step: Step) -> Uuid {
    let id = Uuid::new_v4();
    {
        let mut all = state.analyses.lock().unwrap();
        all.retain(|_, a| a.started.elapsed() < KEPT);
        while all.len() >= MOST {
            let Some(oldest) =
                all.iter().min_by_key(|(_, a)| a.started).map(|(id, _)| *id)
            else {
                break;
            };
            all.remove(&oldest);
        }
        all.insert(
            id,
            Analysis {
                user: user.id,
                started: Instant::now(),
                step,
                done: None,
            },
        );
    }
    id
}

/// Keeps what analysis `id` ended with, if it is still kept.
fn finish(analyses: &Analyses, id: Uuid, done: Done) {
    if let Some(analysis) = analyses.lock().unwrap().get_mut(&id) {
        analysis.done = Some(done);
    }
}

/// Starts checking a file as build `schematic`'s next version for `user`,
/// by `check`, keeping the edit form as sent, `pairs`, to show again with
/// it, and returns the check's id.
pub fn start_version(
    state: &AppState,
    user: &User,
    schematic: String,
    pairs: Vec<(String, String)>,
    check: impl Future<Output = Result<Checked, String>> + Send + 'static,
) -> Uuid {
    let id = register(state, user, Step::Checking);
    let analyses = Arc::clone(&state.analyses);
    tokio::spawn(async move {
        let checked = check.await;
        let done = VersionChecked {
            schematic,
            checked,
            pairs,
        };
        finish(&analyses, id, Done::Version(Arc::new(done)));
    });
    id
}

/// Starts analyzing `file` for `user`, naming each category it is probed as by
/// its label, keeping `filled`, the details form as sent from a preview
/// whose file it takes the place of, if any, and returns the analysis's id.
pub fn start(
    state: &AppState,
    user: &User,
    file: Vec<u8>,
    filled: Option<Vec<(String, String)>>,
) -> Uuid {
    let id = register(state, user, Step::Inspecting);
    let analyses = Arc::clone(&state.analyses);
    // Probing simulates, so off the async threads.
    tokio::task::spawn_blocking(move || {
        let set = |step: Step| {
            if let Some(analysis) = analyses.lock().unwrap().get_mut(&id) {
                analysis.step = step;
            }
        };
        let mut trying = |kind: Kind, tried| {
            let label = kind.label().to_owned();
            set(Step::Testing { label, tried });
        };
        // A file in blocks the simulator doesn't model, such as wool, is
        // read standardized, as publishing keeps it; one it can't run even
        // so is read as it came, saying why.
        let recognition = probe::recognize_watched(&file, None, &mut trying)
            .or_else(|err| {
                let standard = Upload::read(&file)
                    .ok()
                    .and_then(|upload| upload.standard.ok())
                    .ok_or(err)?;
                probe::recognize_watched(&standard.data, None, &mut trying)
            })
            .unwrap_or_else(|err| Recognition {
                file: file.clone(),
                found: None,
                problems: vec![format!("{err:#}")],
                lines: String::new(),
            });
        set(Step::Preparing);
        let analyzed = Upload::read(&recognition.file)
            .map(|upload| Analyzed {
                raw: file.clone(),
                sent: filled,
                guess: category::guess(&upload.read_as().blocks),
                upload,
                recognition,
            })
            .map_err(|err| format!("Could not read that file: {err:#}."));
        // A build recognized as its category is evaluated now, while its
        // uploader fills in its details, for publishing it to take.
        if let Ok(analyzed) = &analyzed
            && analyzed.recognition.found.is_some()
            && let Ok(standard) = &analyzed.upload.standard
            && let Ok(kept) = import::rewrapped(standard)
        {
            simulation::evaluate_ahead(kept.data, kept.sha256);
        }
        finish(&analyses, id, Done::Upload(Arc::new(analyzed)));
    });
    id
}

/// Analysis `id`, when it is `user`'s: how far it has got, and what it
/// found once it is done.
pub fn get(
    state: &AppState,
    id: Uuid,
    user: &User,
) -> Option<(Step, Option<Done>)> {
    let all = state.analyses.lock().unwrap();
    let analysis = all.get(&id).filter(|a| a.user == user.id)?;
    Some((analysis.step.clone(), analysis.done.clone()))
}

/// How many categories probing tries at most.
pub fn kinds() -> usize {
    Kind::probed().len()
}
