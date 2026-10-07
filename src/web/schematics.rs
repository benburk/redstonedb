use axum::{
    Form,
    extract::{Path, State},
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
};

use super::pages;

pub async fn list(
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Response {
    let username = user_opt.map(|user| user.username).unwrap_or_default();
    match schematic::all(&state.db).await {
        Ok(schematics) => {
            pages::schematics(&username, &schematics).into_response()
        }
        Err(err) => internal_error(err).into_response(),
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
) -> Response {
    let found = match schematic::get(&state.db, &id).await {
        Ok(found) => found,
        Err(err) => return internal_error(err).into_response(),
    };
    let Some(s) = found else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match schematic::data(&state.db, &id).await {
        Ok(Some(data)) => (
            [
                (header::CONTENT_TYPE, "application/octet-stream".to_owned()),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{}\"", s.filename()),
                ),
            ],
            data,
        )
            .into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => internal_error(err).into_response(),
    }
}
