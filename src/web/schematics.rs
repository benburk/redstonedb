use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    http::{Method, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    app_state::AppState,
    error::internal_error,
    interface, lint,
    models::schematic::{self, Category, Credit, Schematic},
    models::user::User,
    schem::Blocks,
    search, stack, standardize, thumbnail,
};

use super::{
    details,
    pages::{self, Choices},
};

#[derive(Deserialize)]
pub struct ListQuery {
    view: Option<String>,
    /// A search such as `register vertical alpha -torchless`.
    q: Option<String>,
    /// Only builds of this category, such as `adder`.
    category: Option<String>,
    /// `volume` or `latency`; the default order otherwise.
    sort: Option<String>,
}

/// The schematics matching `?q=`, as cards unless `?view=list` asks for
/// the table.
pub async fn list(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
    user_opt: Option<User>,
) -> Response {
    let view = match query.view.as_deref() {
        Some("list") => pages::View::List,
        _ => pages::View::Cards,
    };
    let query_text = query.q.unwrap_or_default();
    let category = query.category.unwrap_or_default();
    let terms = search::parse(&query_text);
    let loaded = async {
        Ok::<_, sqlx::Error>((
            schematic::all(&state.db).await?,
            schematic::categories(&state.db).await?,
        ))
    };
    let (all, categories) = match loaded.await {
        Ok(loaded) => loaded,
        Err(err) => return internal_error(err).into_response(),
    };
    let total = all.len();
    let found = search::matching(&terms, all);
    let facets = facets(&categories, &found, &category);
    let matched = found.len();
    let mut shown: Vec<Schematic> = if category.is_empty() {
        found
    } else {
        found
            .into_iter()
            .filter(|s| in_category(s, &category))
            .collect()
    };
    // Newest first, and the other orders are stable, so ties keep it.
    shown.sort_by(|a, b| b.added_at.cmp(&a.added_at));
    let sort = pages::Sort::parse(query.sort.as_deref().unwrap_or_default());
    match sort {
        pages::Sort::Newest => {}
        pages::Sort::Volume => shown.sort_by_key(Schematic::volume),
        pages::Sort::Latency => {
            shown.sort_by_key(|s| (s.latency.is_none(), s.latency));
        }
    }
    pages::schematics(
        user_opt.as_ref(),
        &pages::Listing {
            shown: &shown,
            total,
            matched,
            view,
            sort,
            query: &query_text,
            terms: &terms,
            category: &category,
            facets: &facets,
        },
    )
    .into_response()
}

/// The sidebar's name for every category off the standard list.
const OTHER: &str = "other";

/// Whether `s` belongs under the sidebar's `category`: its own category, or
/// `other` for any category off the standard list.
fn in_category(s: &Schematic, category: &str) -> bool {
    if category == OTHER {
        s.category_label.is_none()
    } else {
        s.category == category
    }
}

/// How many of `found` each category has, for the sidebar: the standard
/// categories alphabetically, then every other category together under
/// Other.
fn facets(
    categories: &[Category],
    found: &[Schematic],
    chosen: &str,
) -> Vec<pages::Facet> {
    let count =
        |name: &str| found.iter().filter(|s| in_category(s, name)).count();
    let mut facets: Vec<pages::Facet> = categories
        .iter()
        .map(|f| pages::Facet {
            name: f.name.clone(),
            label: f.plural.clone(),
            count: count(&f.name),
        })
        .collect();
    facets.push(pages::Facet {
        name: OTHER.to_owned(),
        label: "Other".to_owned(),
        count: count(OTHER),
    });
    // Every standard category shows, even with no builds yet; Other only
    // when it has some or is chosen.
    facets.retain(|f| f.name != OTHER || f.count > 0 || f.name == chosen);
    facets
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
    pages::schematic(user_opt.as_ref(), &s, &analysis).into_response()
}

/// The page where a mod edits a build's details and tags.
pub async fn edit(
    Path(id): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Response {
    let Some(user) = user_opt else {
        return Redirect::to("/sessions/new").into_response();
    };
    if !user.can_edit() {
        return StatusCode::FORBIDDEN.into_response();
    }
    show_edit(&state.db, &user, &id, None, "").await
}

/// Saves a mod's edit of a build's details and tags, then shows its page;
/// a refusal shows the edit page again with the values sent.
pub async fn save(
    Path(id): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
    body: axum::body::Bytes,
) -> Response {
    let Some(user) = user_opt.filter(User::can_edit) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    let form = details::Form::read(&details::pairs(&body));
    let refused = match saved(&state.db, &id, &form).await {
        Ok(Ok(())) => {
            return Redirect::to(&format!("/schematics/{id}")).into_response();
        }
        Ok(Err(message)) => message,
        Err(err) => return internal_error(err).into_response(),
    };
    show_edit(&state.db, &user, &id, Some(&form), &refused).await
}

/// Deletes a build for good, with its tags, credits, and stacking; admins
/// only.
pub async fn delete(
    Path(id): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Response {
    let Some(admin) = user_opt.filter(User::is_admin) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    match schematic::delete(&state.db, &id).await {
        Ok(()) => {
            tracing::info!(id, by = admin.username, "deleted a build");
            Redirect::to("/").into_response()
        }
        Err(err) => internal_error(err).into_response(),
    }
}

/// Checks and stores an edit, or says why it was refused.
async fn saved(
    db: &SqlitePool,
    id: &str,
    form: &details::Form,
) -> Result<Result<(), String>, sqlx::Error> {
    let parsed = match form.parse() {
        Ok(parsed) => parsed,
        Err(message) => return Ok(Err(message)),
    };
    let category =
        match schematic::check_category(db, &parsed.category, &parsed.design)
            .await?
        {
            Ok(category) => category,
            Err(message) => return Ok(Err(message)),
        };
    if let Some(data) = schematic::data(db, id).await?
        && let Err(message) = stack::check(&data, &parsed.stacking)
    {
        return Ok(Err(message));
    }
    let mut details = parsed.as_details();
    // Carry lines mean nothing for a category without them.
    if !category.carries {
        details.carry_in = None;
        details.carry_out = None;
    }
    // Only a category that names each build keeps a given name.
    if !category.named {
        details.given_name = None;
    }
    let mut tx = db.begin().await?;
    schematic::set_details(&mut *tx, id, &details).await?;
    schematic::set_stacking(&mut tx, id, &parsed.stacking).await?;
    let credits = [
        (Credit::Builder, &parsed.builders),
        (Credit::Contributor, &parsed.contributors),
    ];
    for (credit, names) in credits {
        schematic::set_credits(&mut tx, id, credit, names).await?;
    }
    schematic::set_tags(&mut tx, id, &form.tags).await?;
    tx.commit().await?;
    Ok(Ok(()))
}

async fn show_edit(
    db: &SqlitePool,
    user: &User,
    id: &str,
    sent: Option<&details::Form>,
    message: &str,
) -> Response {
    let loaded = async {
        Ok::<_, sqlx::Error>((
            schematic::get(db, id).await?,
            Choices::load(db).await?,
            schematic::all_tags(db).await?,
        ))
    };
    match loaded.await {
        Ok((Some(s), choices, tags)) => pages::edit_schematic(
            user,
            &pages::EditSchematic {
                s: &s,
                sent,
                choices: &choices,
                tags: &tags,
                message,
            },
        )
        .into_response(),
        Ok((None, ..)) => StatusCode::NOT_FOUND.into_response(),
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

/// The stored `.schem` file, named by the naming scheme. The viewer fetches
/// the same URL.
pub async fn download(
    Path(id): Path<String>,
    State(state): State<AppState>,
    method: Method,
    Query(mut query): Query<HashMap<String, String>>,
) -> Response {
    // `markers=yes` adds WorldEdit corner markers; the rest of the query
    // names copies per direction, such as `?west=7&north=3`.
    let markers = query.remove("markers").is_some_and(|v| v == "yes");
    // The 3D viewer fetches the file with `preview=yes`; that, and a HEAD
    // request, is no download.
    let counted = query.remove("preview").is_none() && method == Method::GET;
    // One copy each way is the build itself.
    query.retain(|_, count| count != "1");
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
    if counted && let Err(err) = schematic::count_download(&state.db, &id).await
    {
        return internal_error(err).into_response();
    }
    let data = if markers {
        match standardize::with_corner_markers(&data) {
            Ok(data) => data,
            Err(err) => return internal_error(&*err).into_response(),
        }
    } else {
        data
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
