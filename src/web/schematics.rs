use std::collections::HashMap;

use axum::{
    Form,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use maud::Markup;
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    app_state::AppState,
    error::internal_error,
    interface, lint,
    models::schematic::{self, Schematic, Tag},
    models::user::User,
    schem::Blocks,
    search, stack, thumbnail,
};

use super::pages;

#[derive(Deserialize)]
pub struct ListQuery {
    view: Option<String>,
    /// A search such as `register vertical alpha -torchless`.
    q: Option<String>,
}

/// The schematics matching `?q=`, as cards unless `?view=list` asks for
/// the table.
pub async fn list(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
    user_opt: Option<User>,
) -> Response {
    let username = user_opt.map(|user| user.username).unwrap_or_default();
    let view = match query.view.as_deref() {
        Some("list") => pages::View::List,
        _ => pages::View::Cards,
    };
    let query_text = query.q.unwrap_or_default();
    let terms = search::parse(&query_text);
    match schematic::all(&state.db).await {
        Ok(all) => {
            let total = all.len();
            let shown = search::matching(&terms, all);
            pages::schematics(
                &username,
                &pages::Listing {
                    shown: &shown,
                    total,
                    view,
                    query: &query_text,
                    terms: &terms,
                },
            )
            .into_response()
        }
        Err(err) => internal_error(err).into_response(),
    }
}

/// An isometric picture of the build. Cards link to it with the file's hash
/// in the query, so a changed file gets a new URL and this one can be
/// cached for good.
pub async fn thumbnail(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Response {
    let data = match schematic::data(&state.db, &id).await {
        Ok(Some(data)) => data,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(err) => return internal_error(err).into_response(),
    };
    match Blocks::read(&data).and_then(|blocks| thumbnail::render(&blocks)) {
        Ok(png) => (
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
            ],
            png,
        )
            .into_response(),
        Err(err) => internal_error(&*err).into_response(),
    }
}

pub async fn detail(
    Path(id): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Response {
    let s = match schematic::get(&state.db, &id).await {
        Ok(Some(s)) => s,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(err) => return internal_error(err).into_response(),
    };
    let analysis = match schematic::data(&state.db, &id).await {
        Ok(data) => analyse(data.as_deref().unwrap_or_default()),
        Err(err) => return internal_error(err).into_response(),
    };
    let Some(user) = user_opt else {
        return pages::schematic("", &s, None, &analysis).into_response();
    };
    match available_tags(&state.db, &s).await {
        Ok(available) => {
            pages::schematic(&user.username, &s, Some(&available), &analysis)
                .into_response()
        }
        Err(err) => internal_error(err).into_response(),
    }
}

/// The build's inputs and outputs and the conventions it breaks, or why
/// its file could not be read.
fn analyse(data: &[u8]) -> Result<pages::Analysis, String> {
    let blocks = Blocks::read(data).map_err(|err| err.to_string())?;
    Ok(pages::Analysis {
        interface: interface::read(&blocks),
        violations: lint::check(&blocks),
    })
}

#[derive(Deserialize)]
pub struct AddTag {
    tag: Option<String>,
}

/// Attaches a tag. A fixi request gets the redrawn info panel; a plain form
/// post is sent back to the page.
pub async fn add_tag(
    Path(id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    user_opt: Option<User>,
    Form(form): Form<AddTag>,
) -> Response {
    if user_opt.is_none() {
        return Redirect::to("/sessions/new").into_response();
    }
    let name = form.tag.unwrap_or_default();
    let result = change_tag(&state.db, &id, &name, true).await;
    if !headers.contains_key("fx-request") {
        return Redirect::to(&format!("/schematics/{id}")).into_response();
    }
    respond(result)
}

pub async fn remove_tag(
    Path((id, name)): Path<(String, String)>,
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Response {
    if user_opt.is_none() {
        return Redirect::to("/sessions/new").into_response();
    }
    respond(change_tag(&state.db, &id, &name, false).await)
}

fn respond(result: Result<Option<Markup>, sqlx::Error>) -> Response {
    match result {
        Ok(Some(info)) => info.into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => internal_error(err).into_response(),
    }
}

/// Attaches or detaches the tag `name`, then redraws the schematic's info
/// panel, naming the tag in it if it does not exist. `None` means there is
/// no such schematic.
async fn change_tag(
    db: &SqlitePool,
    id: &str,
    name: &str,
    attach: bool,
) -> Result<Option<Markup>, sqlx::Error> {
    if schematic::get(db, id).await?.is_none() {
        return Ok(None);
    }
    let message = match schematic::tag_id(db, name).await? {
        _ if name.is_empty() => String::new(),
        Some(tag) if attach => {
            schematic::attach(db, id, &tag).await?;
            String::new()
        }
        Some(tag) => {
            schematic::detach(db, id, &tag).await?;
            String::new()
        }
        None => format!("There is no tag named {name}."),
    };
    let Some(s) = schematic::get(db, id).await? else {
        return Ok(None);
    };
    let available = available_tags(db, &s).await?;
    Ok(Some(pages::schematic_info(&s, Some(&available), &message)))
}

/// The tags `s` does not carry yet.
async fn available_tags(
    db: &SqlitePool,
    s: &Schematic,
) -> Result<Vec<Tag>, sqlx::Error> {
    Ok(schematic::all_tags(db)
        .await?
        .into_iter()
        .filter(|tag| s.tags.iter().all(|own| own.name != tag.name))
        .collect())
}

/// The stored `.schem` file, named by the naming scheme. The viewer fetches
/// the same URL.
pub async fn download(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let found = match schematic::get(&state.db, &id).await {
        Ok(found) => found,
        Err(err) => return internal_error(err).into_response(),
    };
    let Some(s) = found else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let data = match schematic::data(&state.db, &id).await {
        Ok(Some(data)) => data,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(err) => return internal_error(err).into_response(),
    };
    // The query names copies per direction, such as `?west=7&north=3`.
    let (data, s) = if query.is_empty() {
        (data, s)
    } else {
        let stacked = query
            .iter()
            .map(|(name, count)| Ok((name.clone(), count.parse()?)))
            .collect::<anyhow::Result<HashMap<String, usize>>>()
            .and_then(|wanted| stack::plan(&s.stacking, &wanted))
            .and_then(|plan| {
                let (data, size) = stack::stack(&data, &plan)?;
                Ok((data, s.stacked(&plan, size)))
            });
        match stacked {
            Ok(stacked) => stacked,
            Err(err) => {
                return (StatusCode::BAD_REQUEST, err.to_string())
                    .into_response();
            }
        }
    };
    (
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", s.filename()),
            ),
        ],
        data,
    )
        .into_response()
}
