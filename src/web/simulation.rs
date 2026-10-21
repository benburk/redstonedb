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
    app_state::AppState,
    category, details,
    error::AppError,
    import::Upload,
    models::{
        catalogue::{self, Category},
        schematic,
        simulation::{self, Status},
        user::User,
        version,
    },
    simulate::{Report, Test},
};

use super::{
    form::{self, Form},
    pages::{self, Waiting},
};

/// What a build's page already shows of its simulation, as it asks after
/// the rest: `none` while its cases run, `cases` while its timing does.
#[derive(Deserialize)]
pub struct Have {
    have: Option<String>,
}

/// Build `id`'s simulation, for its page to ask after while it runs in the
/// background: its section once there is more than the page `have`s,
/// naming the panels it changes for the page to redraw; 204 until then,
/// starting what is missing if nothing is under way; and nothing in its
/// place once it has no test.
pub async fn poll(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Query(Have { have }): Query<Have>,
) -> Result<Response, AppError> {
    let s = schematic::get(&state.db, &id)
        .await?
        .ok_or(AppError::NotFound)?;
    let report = match simulation::status(&state.db, &s).await? {
        Status::Done(report) => report,
        Status::Pending(Some(report)) if have.as_deref() == Some("none") => {
            report
        }
        Status::Pending(_) => return Ok(StatusCode::NO_CONTENT.into_response()),
        Status::Untested => return Ok(nothing()),
    };
    let poll = format!("/schematics/{id}/simulation");
    Ok((
        [("FX-Redraw", "build-banner build-info")],
        pages::simulation_section(&report, Waiting::Polled(&poll)),
    )
        .into_response())
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
/// its builder made it, as the form's `as_built` says, is tested
/// standardized, as it is kept once recognized, with where it goes up.
pub async fn upload(
    State(state): State<AppState>,
    user_opt: Option<User>,
    Query(Asked { step }): Query<Asked>,
    body: Bytes,
) -> Result<Response, AppError> {
    let (pairs, data) = upload_file(user_opt.as_ref(), &body)?;
    let (data, filing) = if form::first(&pairs, "as_built") == "yes" {
        readied(&state, &pairs, &data).await?
    } else {
        (data, None)
    };
    match step {
        Step::Cases => cases(&state, &pairs, data, None, filing).await,
        Step::Timing => timing(&state, &pairs, data, None).await,
    }
}

/// A file as its builder made it, `data`, as the form's category tests it:
/// standardized when that is a category the site recognizes builds of,
/// as it is kept once recognized, else as built; with a note, for the
/// checks, saying whether it goes up as that category or off the list.
async fn readied(
    state: &AppState,
    pairs: &[(String, String)],
    data: &[u8],
) -> Result<(Vec<u8>, Option<Markup>), AppError> {
    let Ok(upload) = Upload::read(data) else {
        return Ok((data.to_vec(), None));
    };
    let categories = catalogue::categories(&state.db).await?;
    let typed = Form::read(pairs).category;
    let Some(meant) = details::meant(&categories, &typed)
        .filter(|c| category::behaviour(&c.name).evaluated)
    else {
        return Ok((upload.original.data, None));
    };
    let label = meant.label.to_lowercase();
    let off = |off: details::Unrecognized| {
        filing_note(
            false,
            &format!(
                "{}, so it goes up as {}, off the list, with its own blocks \
                 and the details you give. Fix it and upload a new version \
                 to move it up.",
                off.why, off.name
            ),
        )
    };
    // Read as built, it can only say it breaks the conventions.
    let (data, unrecognized) = if let Ok(standard) = upload.standard {
        let build = details::Build::of(&standard);
        let found = details::unrecognized(meant, build).await;
        (standard.data, found)
    } else {
        let build = details::Build {
            data: &upload.original.data,
            conventional: false,
        };
        let found = details::unrecognized(meant, build).await;
        (upload.original.data, found)
    };
    let note = match unrecognized {
        Ok(Some(found)) => Some(off(found)),
        Ok(None) => Some(filing_note(
            true,
            &format!(
                "Recognized as {}, so it goes up standardized, with every \
                 detail measured.",
                pages::article(&label)
            ),
        )),
        Err(_) => None,
    };
    Ok((data, note))
}

/// Where a build goes up, for `form.js` to show in the checks: as its
/// category when `recognized`, else off the list, as `text` says.
fn filing_note(recognized: bool, text: &str) -> Markup {
    let state = if recognized { "pass" } else { "warn" };
    html! { p data-filing=(state) hidden { (text) } }
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
    match step {
        Step::Cases => cases(&state, &pairs, data, Some(&s), None).await,
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
) -> Result<(schematic::Schematic, Vec<u8>), AppError> {
    let s = schematic::get(&state.db, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !user.is_some_and(|user| user.may_edit(&s)) {
        return Err(AppError::Forbidden);
    }
    let data = version::current_file(&state.db, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok((s, data))
}

/// The test the form's category and width call for on `data`, the width a
/// category reads from the file, such as an adder's, standing in for the
/// typed one.
async fn test(
    state: &AppState,
    pairs: &[(String, String)],
    data: &[u8],
) -> Result<Option<Test>, AppError> {
    let form = Form::read(pairs);
    let categories = catalogue::categories(&state.db).await?;
    // An off-list name such as 8-bit adder is tested as an adder, to show
    // what keeps it from being one.
    let category = details::meant(&categories, &form.category).map_or_else(
        || Category::name_of(&categories, &form.category),
        |c| c.name.clone(),
    );
    let derived = simulation::derived_width(&category, true, data)?;
    let width = derived.or_else(|| form.width.trim().parse().ok());
    Ok(Test::of(&category, width))
}

/// The stored result of build `s`, when it is for `test` and its file.
async fn stored_result(
    state: &AppState,
    s: Option<&schematic::Schematic>,
    test: Test,
) -> Result<Option<Report>, AppError> {
    let Some(s) = s else { return Ok(None) };
    Ok(simulation::stored(&state.db, &s.id, &s.sha256, test).await?)
}

/// The details form's simulation panel: the section for the cases, with
/// the timing to follow when they pass; empty for a category and width
/// with no test.
async fn cases(
    state: &AppState,
    pairs: &[(String, String)],
    data: Vec<u8>,
    s: Option<&schematic::Schematic>,
    filing: Option<Markup>,
) -> Result<Response, AppError> {
    let simulate = match s {
        Some(s) => format!("/schematics/{}/simulation", s.id),
        None => "/upload/simulation".to_owned(),
    };
    let panel = |content: &Markup| {
        let content = html! { (filing.clone().unwrap_or_default()) (content) };
        pages::simulation_results(&simulate, &content).into_response()
    };
    let Some(test) = test(state, pairs, &data).await? else {
        return Ok(panel(&html! {}));
    };
    let report = match stored_result(state, s, test).await? {
        Some(report) => report,
        None => match simulation::run(test, data).await {
            Ok(report) => report,
            Err(err) => return Ok(panel(&could_not(&err.to_string()))),
        },
    };
    let timing = timing_url(&simulate);
    Ok(panel(&pages::simulation_section(
        &report,
        Waiting::Posted(&timing),
    )))
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
    s: Option<&schematic::Schematic>,
) -> Result<Response, AppError> {
    let Some(test) = test(state, pairs, &data).await? else {
        return Ok(nothing());
    };
    if let Ok(Some(report)) = stored_result(state, s, test).await
        && let Some(timing) = report.timing()
    {
        return Ok(pages::timing_section(timing).into_response());
    }
    // The build's own test is measured and kept, as publishing would have.
    if let Some(s) = s
        && Test::of(&s.category, s.width) == Some(test)
    {
        let refreshed =
            simulation::refresh_timing(&state.db, &s.id, false).await;
        return Ok(match refreshed {
            Ok(Some(report)) => match report.timing() {
                Some(timing) => pages::timing_section(timing).into_response(),
                None => nothing(),
            },
            Ok(None) => nothing(),
            Err(err) => could_not(&err.to_string()).into_response(),
        });
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let _guard = CancelOnDrop(cancel.clone());
    Ok(match simulation::time(test, data, cancel).await {
        Ok(timing) => pages::timing_section(&timing).into_response(),
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
