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
    category::{self, Category},
    details,
    import::Upload,
    models::{schematic, simulation, user::User},
    recognize::fault,
    simulate::{Report, Test},
    web::{app_state::AppState, error::AppError},
};

use super::{
    form::{self, Form},
    pages::{self, Waiting},
};

/// Build `id`'s simulation, for its page to ask after while storing it
/// simulates it in the background: 204 until that is done, then its
/// section, naming the panels it changes for the page to redraw, or
/// nothing in its place when none is stored to show.
pub async fn poll(
    Path(id): Path<String>,
    State(state): State<AppState>,
    viewer: Option<User>,
) -> Result<Response, AppError> {
    let s = schematic::get_visible(&state.db, &id, viewer.as_ref())
        .await?
        .ok_or(AppError::NotFound)?;
    if simulation::completing(&id) {
        return Ok(StatusCode::NO_CONTENT.into_response());
    }
    // Off the list, a build the simulator can't run says why under its
    // reasons already.
    let report = simulation::current(&state.db, &s).await?.filter(|report| {
        simulation::stands_in(&s).is_none()
            || !matches!(report, Report::Skipped { .. })
    });
    let redraw = [("FX-Redraw", "build-banner build-info")];
    Ok(match report {
        Some(report) => {
            let trace = format!("/schematics/{id}/trace");
            (
                redraw,
                pages::simulation_section(
                    &report,
                    Waiting::Stored,
                    Some(&trace),
                ),
            )
                .into_response()
        }
        None => (redraw, nothing()).into_response(),
    })
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
    let Some(meant) = details::meant(&categories, &typed)
        .filter(|c| category::behaviour(&c.name).evaluated)
    else {
        return Ok((upload.original.data, None));
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
            passes: false,
        };
        let found = details::unrecognized(meant, build).await;
        (upload.original.data, found)
    };
    let filing = match unrecognized {
        Ok(Some(found)) => Some(Filing::Unverified(found.why)),
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
) -> Result<(schematic::Schematic, Vec<u8>), AppError> {
    let s = schematic::get_visible(&state.db, id, user)
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
    // An off-list name such as 8-bit adder is tested as an adder, to show
    // what keeps it from being one.
    let Some(category) = details::meant(&categories, &form.category)
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
    let Some(test) = test(pairs, &data)? else {
        return Ok(panel(&html! {}, false));
    };
    let report = match stored_result(state, s, test).await? {
        Some(report) => report,
        None => match simulation::run(test, data).await {
            Ok(report) => report,
            Err(err) => {
                return Ok(panel(&could_not(&err.to_string()), false));
            }
        },
    };
    let timing = timing_url(&simulate);
    Ok(panel(
        &pages::simulation_section(&report, Waiting::Posted(&timing), None),
        report.failed(),
    ))
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
