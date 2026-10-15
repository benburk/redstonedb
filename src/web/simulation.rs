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
    error::internal_error,
    models::{
        schematic,
        simulation::{self, Status},
        user::User,
    },
    simulate::{Report, Test},
};

use super::{
    details,
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
) -> Response {
    let s = match schematic::get(&state.db, &id).await {
        Ok(Some(s)) => s,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(err) => return internal_error(err).into_response(),
    };
    let report = match simulation::status(&state.db, &s).await {
        Ok(Status::Done(report)) => report,
        Ok(Status::Pending(Some(report)))
            if have.as_deref() == Some("none") =>
        {
            report
        }
        Ok(Status::Pending(_)) => {
            return StatusCode::NO_CONTENT.into_response();
        }
        Ok(Status::Untested) => return nothing(),
        Err(err) => return internal_error(&*err).into_response(),
    };
    let poll = format!("/schematics/{id}/simulation");
    (
        [("FX-Redraw", "build-banner build-info")],
        pages::simulation_section(&report, Waiting::Polled(&poll)),
    )
        .into_response()
}

/// The cases, for the file being uploaded, carried in the form's `data`.
pub async fn upload(
    State(state): State<AppState>,
    user_opt: Option<User>,
    body: Bytes,
) -> Response {
    match upload_file(user_opt.as_ref(), &body) {
        Ok((pairs, data)) => cases(&state, &pairs, data, None).await,
        Err(status) => status.into_response(),
    }
}

/// The timing, for the file being uploaded.
pub async fn upload_timing(
    State(state): State<AppState>,
    user_opt: Option<User>,
    body: Bytes,
) -> Response {
    match upload_file(user_opt.as_ref(), &body) {
        Ok((pairs, data)) => timing(&state, &pairs, data, None).await,
        Err(status) => status.into_response(),
    }
}

/// The cases for stored build `id` under the category and width on its
/// edit page, which may not be saved yet.
pub async fn stored(
    Path(id): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
    body: Bytes,
) -> Response {
    match stored_file(&state, user_opt.as_ref(), &id).await {
        Ok((s, data)) => {
            cases(&state, &details::pairs(&body), data, Some(&s)).await
        }
        Err(response) => response,
    }
}

/// The timing for stored build `id`, as `stored`.
pub async fn stored_timing(
    Path(id): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
    body: Bytes,
) -> Response {
    match stored_file(&state, user_opt.as_ref(), &id).await {
        Ok((s, data)) => {
            timing(&state, &details::pairs(&body), data, Some(&s)).await
        }
        Err(response) => response,
    }
}

type Pairs = Vec<(String, String)>;

/// The form's fields and the file it carries, for someone logged in.
fn upload_file(
    user: Option<&User>,
    body: &[u8],
) -> Result<(Pairs, Vec<u8>), StatusCode> {
    if user.is_none() {
        return Err(StatusCode::FORBIDDEN);
    }
    let pairs = details::pairs(body);
    let data = pairs
        .iter()
        .find(|(key, _)| key == "data")
        .and_then(|(_, value)| STANDARD.decode(value).ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    Ok((pairs, data))
}

/// Build `id` and its file, for someone who may edit it.
async fn stored_file(
    state: &AppState,
    user: Option<&User>,
    id: &str,
) -> Result<(schematic::Schematic, Vec<u8>), Response> {
    let s = match schematic::get(&state.db, id).await {
        Ok(Some(s)) => s,
        Ok(None) => return Err(StatusCode::NOT_FOUND.into_response()),
        Err(err) => return Err(internal_error(err).into_response()),
    };
    if !user.is_some_and(|user| user.may_edit(&s)) {
        return Err(StatusCode::FORBIDDEN.into_response());
    }
    match schematic::data(&state.db, id).await {
        Ok(Some(data)) => Ok((s, data)),
        Ok(None) => Err(StatusCode::NOT_FOUND.into_response()),
        Err(err) => Err(internal_error(err).into_response()),
    }
}

/// The test the form's category and width call for on `data`, the width a
/// category reads from the file, such as an adder's, standing in for the
/// typed one.
async fn test(
    state: &AppState,
    pairs: &[(String, String)],
    data: &[u8],
) -> Result<Option<Test>, Response> {
    let form = details::Form::read(pairs);
    let categories = schematic::categories(&state.db)
        .await
        .map_err(|err| internal_error(err).into_response())?;
    let Ok((category, _)) = form.category(&categories) else {
        return Ok(None);
    };
    let conventional = !pairs
        .iter()
        .any(|(key, value)| key == "unconventional" && value == "yes");
    let derived = simulation::derived_width(&category, conventional, data)
        .map_err(|err| internal_error(&*err).into_response())?;
    let width = derived.or_else(|| form.width.trim().parse().ok());
    Ok(Test::of(&category, width))
}

/// The stored result of build `s`, when it is for `test` and its file.
async fn stored_result(
    state: &AppState,
    s: Option<&schematic::Schematic>,
    test: Test,
) -> Result<Option<Report>, Response> {
    let Some(s) = s else { return Ok(None) };
    let stored = simulation::get(&state.db, &s.id)
        .await
        .map_err(|err| internal_error(&*err).into_response())?;
    Ok(stored
        .filter(|stored| {
            stored.test == test.name() && stored.sha256 == s.sha256
        })
        .map(|stored| stored.report))
}

/// The details form's simulation panel: the section for the cases, with
/// the timing to follow when they pass; empty for a category and width
/// with no test.
async fn cases(
    state: &AppState,
    pairs: &[(String, String)],
    data: Vec<u8>,
    s: Option<&schematic::Schematic>,
) -> Response {
    let simulate = match s {
        Some(s) => format!("/schematics/{}/simulation", s.id),
        None => "/upload/simulation".to_owned(),
    };
    let panel = |content: &Markup| {
        pages::simulation_results(&simulate, content).into_response()
    };
    let test = match test(state, pairs, &data).await {
        Ok(Some(test)) => test,
        Ok(None) => return panel(&html! {}),
        Err(response) => return response,
    };
    let report = match stored_result(state, s, test).await {
        Ok(Some(report)) => report,
        Ok(None) => match simulation::run(test, data).await {
            Ok(report) => report,
            Err(err) => return panel(&could_not(&err.to_string())),
        },
        Err(response) => return response,
    };
    let timing = format!("{simulate}/timing");
    panel(&pages::simulation_section(
        &report,
        Waiting::Posted(&timing),
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
) -> Response {
    let test = match test(state, pairs, &data).await {
        Ok(Some(test)) => test,
        Ok(None) => return nothing(),
        Err(response) => return response,
    };
    if let Ok(Some(report)) = stored_result(state, s, test).await
        && let Some(timing) = report.timing()
    {
        return pages::timing_section(timing).into_response();
    }
    // The build's own test is measured and kept, as publishing would have.
    if let Some(s) = s
        && Test::of(&s.category, s.width) == Some(test)
    {
        return match simulation::refresh_timing(&state.db, &s.id, false).await {
            Ok(Some(report)) => match report.timing() {
                Some(timing) => pages::timing_section(timing).into_response(),
                None => nothing(),
            },
            Ok(None) => nothing(),
            Err(err) => could_not(&err.to_string()).into_response(),
        };
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let _guard = CancelOnDrop(cancel.clone());
    match simulation::time(test, data, cancel).await {
        Ok(timing) => pages::timing_section(&timing).into_response(),
        Err(err) => could_not(&err.to_string()).into_response(),
    }
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
