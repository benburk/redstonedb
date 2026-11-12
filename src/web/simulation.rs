//! A build's simulation result, run while its details are filled in: the
//! details form asks again whenever its category or width changes, first
//! for the cases, then, once they pass, for the timing, which is slower.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use maud::{Markup, html};
use serde::Deserialize;

use crate::{
    category::{self, Category, Test},
    details,
    import::Upload,
    models::{schematic, simulation, user::User},
    recognize::{fault, probe::Recognition},
    simulate::Report,
    web::{app_state::AppState, error::AppError},
};

use super::{
    form::{self, Form},
    pages,
};

/// Build `id`'s simulation, for its page to ask after while storing it
/// simulates it in the background: 204 until that is done, then the page
/// again, its checks drawn from what was stored.
pub async fn poll(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    schematic::get(&state.db, &id)
        .await?
        .ok_or(AppError::NotFound)?;
    if simulation::completing(&id) {
        return Ok(StatusCode::NO_CONTENT.into_response());
    }
    let page = format!("/schematics/{id}#checks");
    Ok((StatusCode::NO_CONTENT, [("FX-Redirect", page)]).into_response())
}

/// Which part of a simulation the details form asks for: the cases, or,
/// once they pass, the slower timing, as `?step=timing`.
#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Step {
    #[default]
    Cases,
    Timing,
}

/// The address of a simulation's `Step`.
#[derive(Deserialize)]
pub struct Asked {
    #[serde(default)]
    step: Step,
}

/// Where the details form asks for the timing, given where it asks for
/// the cases.
pub fn timing_url(simulate: &str) -> String {
    format!("{simulate}?step=timing")
}

/// The cases or the timing for the file being uploaded, or a stored
/// build's next version, carried in the form's `data`. A file carried as
/// its author made it, as the form's `as_built` says, is tested
/// standardized, as it is kept once recognized, with where it goes up.
pub async fn upload(
    State(state): State<AppState>,
    user_opt: Option<User>,
    Query(Asked { step }): Query<Asked>,
    body: Bytes,
) -> Result<Response, AppError> {
    let (pairs, data) = upload_file(user_opt.as_ref(), &body)?;
    let (data, filing) = if form::first(&pairs, "as_built") == "yes" {
        readied(&pairs, &data).await?
    } else {
        (data, None)
    };
    match step {
        Step::Cases => cases(&state, &pairs, data, None, filing).await,
        Step::Timing => timing(&state, &pairs, data, None).await,
    }
}

/// A file as its author made it, `data`, as the form's category tests it:
/// standardized when that is a category the site recognizes builds of,
/// as it is kept once recognized, else as built; with whether it goes up
/// verified as that category, or unverified, and why.
async fn readied(
    pairs: &[(String, String)],
    data: &[u8],
) -> Result<(Vec<u8>, Option<Filing>), AppError> {
    let Ok(upload) = Upload::read(data) else {
        return Ok((data.to_vec(), None));
    };
    let categories = Category::all();
    let typed = Form::read(pairs).category;
    let Some(meant) = Category::find(&categories, &typed)
        .filter(|c| category::behaviour(&c.name).evaluated)
    else {
        return Ok((upload.original.data, None));
    };
    // Read as built, it can only say it breaks the conventions.
    let (data, unrecognized) = if let Ok(standard) = upload.standard {
        let build = details::Candidate::of(&standard);
        let found = details::unrecognized(meant, build).await;
        (standard.data, found)
    } else {
        let build = details::Candidate {
            data: &upload.original.data,
            conventional: false,
            passes: false,
        };
        let found = details::unrecognized(meant, build).await;
        (upload.original.data, found)
    };
    let filing = match unrecognized {
        Ok(Some(why)) => Some(Filing::Unverified(why)),
        Ok(None) => Some(Filing::Verified),
        Err(_) => None,
    };
    Ok((data, filing))
}

/// Whether a build goes up verified as its category, or unverified, and
/// why.
enum Filing {
    Verified,
    Unverified(String),
}

impl Filing {
    /// The filing, for `form.js` to badge the category with, verified,
    /// failing, or untested, and say why under the form, leaving out
    /// wrong cases when `failed`, as the simulation's section shows them.
    fn note(&self, failed: bool) -> Markup {
        let (state, why) = match self {
            Filing::Verified => ("pass", None),
            Filing::Unverified(why) => {
                (pages::failing_or_untested(&[why]), Some(why.as_str()))
            }
        };
        let reasons: Vec<&str> = why
            .into_iter()
            .filter(|why| !(failed && fault::wrong_outputs(why)))
            .collect();
        html! { template data-filing=(state) { (pages::reason_items(&reasons)) } }
    }
}

/// The cases or the timing for stored build `id` under the category and
/// width on its edit page, which may not be saved yet.
pub async fn stored(
    Path(id): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
    Query(Asked { step }): Query<Asked>,
    body: Bytes,
) -> Result<Response, AppError> {
    let (s, data) = stored_file(&state, user_opt.as_ref(), &id).await?;
    let pairs = form::pairs(&body);
    // An unverified build says why it stays so, whatever its cases show.
    let filing = s.unverified.clone().map(Filing::Unverified);
    match step {
        Step::Cases => cases(&state, &pairs, data, Some(&s), filing).await,
        Step::Timing => timing(&state, &pairs, data, Some(&s)).await,
    }
}

type Pairs = Vec<(String, String)>;

/// The form's fields and the file it carries, for someone logged in. The
/// form asks in the background, so anyone else is refused rather than sent
/// off to log in, which would leave the form they were filling in.
fn upload_file(
    user: Option<&User>,
    body: &[u8],
) -> Result<(Pairs, Vec<u8>), AppError> {
    if user.is_none() {
        return Err(AppError::Forbidden);
    }
    let pairs = form::pairs(body);
    let data = Some(form::first(&pairs, "data"))
        .filter(|data| !data.is_empty())
        .and_then(|data| STANDARD.decode(data).ok())
        .ok_or_else(|| {
            AppError::BadRequest("The form carries no file.".to_owned())
        })?;
    Ok((pairs, data))
}

/// Build `id` and its file, for someone who may edit it.
async fn stored_file(
    state: &AppState,
    user: Option<&User>,
    id: &str,
) -> Result<(schematic::Build, Vec<u8>), AppError> {
    let s = schematic::get(&state.db, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !user.is_some_and(|user| user.may_edit(&s)) {
        return Err(AppError::Forbidden);
    }
    let data = simulation::current_file(&state.db, &s).await?;
    Ok((s, data))
}

/// The test the form's category calls for on `data`, at the width it
/// reads from the file: only a blessed category's, see
/// `category::Facts::listed_as`, as only such a build goes on the list.
fn test(
    pairs: &[(String, String)],
    data: &[u8],
) -> Result<Option<Test>, AppError> {
    let form = Form::read(pairs);
    let categories = Category::all();
    let Some(category) = Category::find(&categories, &form.category)
        .map(|c| c.name.as_str())
        .filter(|c| crate::category::blessed(c))
    else {
        return Ok(None);
    };
    let width = simulation::derived_width(category, true, data)?;
    Ok(Test::of(category, width))
}

/// The stored result of build `s`, when it is for `test` and its file.
async fn stored_result(
    state: &AppState,
    s: Option<&schematic::Build>,
    test: Test,
) -> Result<Option<Report>, AppError> {
    let Some(s) = s else { return Ok(None) };
    Ok(simulation::stored(&state.db, &s.id, &s.file.sha256, test).await?)
}

/// The details form's checks: the same stages a build's page shows, its
/// file read, its lines found, its cases run under the form's category and
/// width, and its timing measured once the page has loaded.
async fn cases(
    state: &AppState,
    pairs: &[(String, String)],
    data: Vec<u8>,
    s: Option<&schematic::Build>,
    filing: Option<Filing>,
) -> Result<Response, AppError> {
    let simulate = match s {
        Some(s) => format!("/schematics/{}/simulation", s.id),
        None => "/upload/simulation".to_owned(),
    };
    let panel = |content: &Markup, failed: bool| {
        let note = filing.as_ref().map(|filing| filing.note(failed));
        let content = html! { (note.unwrap_or_default()) (content) };
        pages::simulation_results(&simulate, &content).into_response()
    };
    let category = meant_category(pairs);
    let recognition = recognized(&data, &category).await;
    let lined = lined(&data, recognition.as_ref());
    let report = match test(pairs, &lined)? {
        None => None,
        Some(test) => match stored_result(state, s, test).await? {
            Some(report) => Some(report),
            None => match simulation::run(test, lined.clone()).await {
                Ok(report) => Some(report),
                Err(err) => {
                    return Ok(panel(&could_not(&err.to_string()), false));
                }
            },
        },
    };
    let original = Some(form::first(pairs, "original"))
        .filter(|o| !o.is_empty())
        .and_then(|o| STANDARD.decode(o).ok());
    let passes = report.as_ref().is_some_and(Report::passed);
    let file = read_file(&data, &category, recognition, original, passes).await;
    let why = match &filing {
        Some(Filing::Unverified(why)) => Some(why.as_str()),
        _ => None,
    };
    let subject = if let Some(s) = s {
        pages::Subject::of(s)
    } else {
        let size = crate::schem::Blocks::read(&data)
            .map_or([0; 3], |blocks| blocks.size);
        // Its buses as its recognized lines lay them, which the conventions
        // may not.
        let unaligned =
            crate::schem::Blocks::read(&lined).is_ok_and(|blocks| {
                blocks.roles.is_some()
                    && crate::interface::Kept::of(&crate::interface::read(
                        &blocks,
                    ))
                    .unaligned()
            });
        pages::Subject {
            size: (size[0].into(), size[1].into(), size[2].into()),
            category: &category,
            // Unranked is said only of one that works.
            unranked: (unaligned && passes)
                .then(|| "unaligned buses".to_owned()),
            unaligned,
            conventional: true,
            unverified: why,
        }
    };
    let trace = s.map(|s| format!("/schematics/{}/trace", s.id));
    let timing = timing_url(&simulate);
    let known = pages::Known {
        s: subject,
        file: &file,
        report: report.as_ref(),
        trace: trace.as_deref(),
        timing: Some(&timing),
    };
    Ok(panel(
        &pages::form_checks(&known),
        report.as_ref().is_some_and(Report::failed),
    ))
}

/// The category the form names, as the site lists it; empty for none.
fn meant_category(pairs: &[(String, String)]) -> String {
    let categories = Category::all();
    Category::find(&categories, &Form::read(pairs).category)
        .map_or_else(String::new, |c| c.name.clone())
}

/// What probing `data` as `category` finds, off the async threads: for a
/// category the site recognizes builds of, the build recognized, or the
/// fault to show with the file carrying the closest try's lines, as a
/// failing adder's split operands; `None` for any other category. Probing
/// doesn't read hex inputs, so a hex adder's faults are its test's.
async fn recognized(
    data: &[u8],
    category: &str,
) -> Option<Result<Recognition, String>> {
    if !category::blessed(category) || category == "hex_adder" {
        return None;
    }
    let (data, meant) = (data.to_vec(), category.to_owned());
    let found = tokio::task::spawn_blocking(move || {
        crate::recognize::probe::recognize_as(&data, Some(&meant))
    })
    .await;
    Some(match found {
        Ok(Ok(recognition)) => Ok(recognition),
        Ok(Err(err)) => Err(err.to_string()),
        Err(err) => Err(err.to_string()),
    })
}

/// `data` carrying the lines `recognition` found, so its test reads them
/// as a stored build's does, rather than guessing from their shapes; as it
/// is when it found none.
fn lined(
    data: &[u8],
    recognition: Option<&Result<Recognition, String>>,
) -> Vec<u8> {
    match recognition {
        Some(Ok(r)) if crate::models::simulation::carries_roles(&r.file) => {
            r.file.clone()
        }
        _ => data.to_vec(),
    }
}

/// The file `data` as the checks read it, as a build of `category`: its
/// lines, its blocks, and, for a category the site recognizes, what keeps
/// it from being one, as a stored build's page re-reads it; and what
/// uploading it takes out or swaps, from `original`, the file as its
/// author made it, when the form carries it, its unused blocks only when
/// it `passes`.
async fn read_file(
    data: &[u8],
    category: &str,
    recognition: Option<Result<Recognition, String>>,
    original: Option<Vec<u8>>,
    passes: bool,
) -> Result<pages::FileRead, String> {
    let blocks =
        crate::schem::Blocks::read(data).map_err(|err| err.to_string())?;
    let problems = match &recognition {
        Some(Ok(r)) => r.problems.clone(),
        Some(Err(err)) => vec![err.clone()],
        None => Vec::new(),
    };
    let found = match recognition {
        Some(Ok(r)) if r.found.is_some() => Some(r.file),
        _ => None,
    };
    let kept = tokio::task::spawn_blocking({
        let data = data.to_vec();
        move || taken_out(&data, original.as_deref(), found.as_deref(), passes)
    })
    .await
    .map_err(|err| err.to_string())?;
    Ok(pages::FileRead {
        analysis: crate::import::Analysis::of(&blocks, category),
        block_counts: crate::schem::block::block_counts(&blocks),
        unrecognized: problems,
        strength_inputs: crate::interface::takes_digits(&blocks),
        stacking: Vec::new(),
        kept,
    })
}

/// What uploading `data` takes out or swaps, each a sentence: the blocks
/// `original`, as its author made it, had swapped for barrels and its
/// trapdoors taken out; and, as the site keeps it once recognized,
/// `found`, its signs taken out, and its unused blocks when it `passes`.
fn taken_out(
    data: &[u8],
    original: Option<&[u8]>,
    found: Option<&[u8]>,
    passes: bool,
) -> Vec<String> {
    use crate::schem::Blocks;
    let count = |file: &[u8], is: &dyn Fn(&str) -> bool| {
        Blocks::read(file).map_or(0, |blocks| {
            blocks
                .iter()
                .filter(|(_, state)| is(crate::schem::short(state)))
                .count()
        })
    };
    let plural = |n: usize, one: &str, many: &str| {
        if n == 1 {
            format!("1 {one}")
        } else {
            format!("{n} {many}")
        }
    };
    let mut said = Vec::new();
    if let Some(original) = original {
        let trapdoors = count(original, &|name| name.ends_with("_trapdoor"));
        if trapdoors > 0 {
            said.push(format!(
                "{} taken out, which only showed an output flipping.",
                plural(trapdoors, "trapdoor", "trapdoors")
            ));
        }
        if let Ok(blocks) = Blocks::read(original) {
            let swapped =
                crate::ingest::standardize::swappable(&blocks).stand_ins;
            if !swapped.is_empty() {
                let one = matches!(swapped.as_slice(), [(_, 1)]);
                said.push(format!(
                    "Swapped {} for {} giving the same signal strength.",
                    pages::counted(&swapped),
                    if one { "a barrel" } else { "barrels" }
                ));
            }
        }
    }
    let Some(found) = found else {
        return said;
    };
    let sign = |name: &str| name.ends_with("_sign");
    let signs = count(data, &sign).saturating_sub(count(found, &sign));
    if signs > 0 {
        said.push(format!(
            "{} taken out: the site names its lines itself.",
            plural(signs, "sign", "signs")
        ));
    }
    if passes {
        let buses = crate::stack::Buses::own();
        if let Ok(Some(pruned)) =
            crate::recognize::roles::prune(found, &[], buses)
        {
            said.push(format!(
                "{} no part of its circuit uses taken out.",
                plural(pruned.gone.len(), "block", "blocks")
            ));
        }
    }
    said
}

/// What takes the timing's place when there is none to show: nothing.
fn nothing() -> Response {
    html! { template {} }.into_response()
}

/// The timing section, once the cases pass.
async fn timing(
    state: &AppState,
    pairs: &[(String, String)],
    data: Vec<u8>,
    s: Option<&schematic::Build>,
) -> Result<Response, AppError> {
    // Timed on the lines recognition finds, as its cases were.
    let category = meant_category(pairs);
    let data = lined(&data, recognized(&data, &category).await.as_ref());
    let Some(test) = test(pairs, &data)? else {
        return Ok(nothing());
    };
    if let Ok(Some(report)) = stored_result(state, s, test).await
        && let Some(timing) = report.timing()
    {
        return Ok(pages::timing_section(timing, None).into_response());
    }
    // Measured for the form alone: saving the edit stores it.
    let cancel = Arc::new(AtomicBool::new(false));
    let _guard = CancelOnDrop(cancel.clone());
    Ok(match simulation::time(test, data, cancel).await {
        Ok(timing) => pages::timing_section(&timing, None).into_response(),
        Err(err) => could_not(&err.to_string()).into_response(),
    })
}

fn could_not(why: &str) -> Markup {
    html! {
        p class="mt-6 text-muted" { "Could not simulate it: " (why) }
    }
}

/// Sets its flag when dropped, as when the request it serves is abandoned
/// because the form asked again, so the timing it started stops.
struct CancelOnDrop(Arc<AtomicBool>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}
