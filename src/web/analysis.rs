//! An upload's analysis, run in the background so choosing a file answers
//! at once and its page shows how far it has got: recognizing a file
//! probes it as each blessed category in turn, which takes seconds for a
//! large build.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use uuid::Uuid;

use crate::{
    app_state::AppState,
    import::Upload,
    models::user::User,
    probe::{self, Recognition},
    simulate::{self, Guess},
};

/// Analyses under way or done, by id, each kept a while for its page to
/// be reloaded.
pub type Analyses = Arc<Mutex<HashMap<Uuid, Analysis>>>;

/// How long a finished analysis stays, for its preview to be reloaded.
const KEPT: Duration = Duration::from_hours(1);

/// The most analyses kept at once; the oldest go first.
const MOST: usize = 200;

/// One upload's analysis: whose it is, how far it has got, and, once it
/// is done, what it found or why the file could not be read.
pub struct Analysis {
    user: Uuid,
    started: Instant,
    step: Step,
    done: Option<Outcome>,
}

/// What an analysis found, or why the file could not be read.
pub type Outcome = Arc<Result<Analyzed, String>>;

/// How far an analysis has got.
#[derive(Clone)]
pub enum Step {
    /// Finding its inputs and outputs.
    Inspecting,
    /// Probing it as the category `label`, after `tried` others.
    Testing { label: String, tried: usize },
    /// Reading it as built and as standardized, and guessing what it is.
    Preparing,
}

impl Step {
    /// Its place among the steps the page lists, after uploading the file.
    pub fn place(&self) -> usize {
        match self {
            Step::Inspecting => 1,
            Step::Testing { .. } => 2,
            Step::Preparing => 3,
        }
    }
}

/// What an analysis found: the file read, what it seems to be, and what
/// the check by function made of it.
pub struct Analyzed {
    pub upload: Upload,
    pub guess: Option<Guess>,
    pub recognition: Recognition,
}

/// Starts analyzing `file` for `user`, naming each category it is probed
/// as by its label in `labels`, pairs of name and label, and returns the analysis's id.
pub fn start(
    state: &AppState,
    user: &User,
    file: Vec<u8>,
    labels: Vec<(String, String)>,
) -> Uuid {
    let labels: HashMap<String, String> = labels.into_iter().collect();
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
                step: Step::Inspecting,
                done: None,
            },
        );
    }
    let analyses = Arc::clone(&state.analyses);
    // Probing simulates, so off the async threads.
    tokio::task::spawn_blocking(move || {
        let set = |step: Step| {
            if let Some(analysis) = analyses.lock().unwrap().get_mut(&id) {
                analysis.step = step;
            }
        };
        let mut trying = |kind: &str, tried| {
            let label = labels
                .get(kind)
                .cloned()
                .unwrap_or_else(|| kind.replace('_', " "));
            set(Step::Testing { label, tried });
        };
        // A file the simulator can't run is read as it came, saying why.
        let recognition = probe::recognize_watched(&file, None, &mut trying)
            .unwrap_or_else(|err| Recognition {
                file: file.clone(),
                found: None,
                problems: vec![format!("{err:#}")],
            });
        set(Step::Preparing);
        let analyzed = Upload::read(&recognition.file)
            .map(|upload| Analyzed {
                guess: simulate::guess(&upload.read_as().blocks),
                upload,
                recognition,
            })
            .map_err(|err| format!("Could not read that file: {err:#}."));
        if let Some(analysis) = analyses.lock().unwrap().get_mut(&id) {
            analysis.done = Some(Arc::new(analyzed));
        }
    });
    id
}

/// Analysis `id`, when it is `user`'s: how far it has got, and what it
/// found once it is done.
pub fn get(
    state: &AppState,
    id: Uuid,
    user: &User,
) -> Option<(Step, Option<Outcome>)> {
    let all = state.analyses.lock().unwrap();
    let analysis = all.get(&id).filter(|a| a.user == user.id)?;
    Some((analysis.step.clone(), analysis.done.clone()))
}

/// How many categories probing tries at most.
pub const KINDS: usize = probe::KINDS.len();
