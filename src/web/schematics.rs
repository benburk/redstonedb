use std::{cmp::Ordering, collections::HashMap};

use axum::{
    extract::{Path, Query, RawQuery, State},
    http::{Method, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
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

/// What the home page's address asks for. Separate parameters narrow
/// together; within one, commas mean any of them, as in
/// `?category=adder,subtractor`. `tag` may repeat, each needing a match:
/// `?tag=alpha&tag=torchless` is both, and `?tag=alpha,torchless` either.
#[derive(Default)]
struct ListQuery {
    view: String,
    /// A search such as `register vertical alpha -torchless`.
    q: String,
    category: String,
    /// Fields such as `-downloads,volume`; see [`order`].
    sort: String,
    tags: Vec<String>,
}

impl ListQuery {
    fn read(raw: Option<&str>) -> Self {
        let mut query = ListQuery::default();
        let pairs = form_urlencoded::parse(raw.unwrap_or_default().as_bytes());
        for (key, value) in pairs {
            let value = value.trim().to_owned();
            match key.as_ref() {
                "view" => query.view = value,
                "q" => query.q = value,
                "category" => query.category = value,
                "sort" => query.sort = value,
                "tag" if !value.is_empty() => query.tags.push(value),
                _ => {}
            }
        }
        query
    }
}

pub async fn list(
    State(state): State<AppState>,
    RawQuery(raw): RawQuery,
    user_opt: Option<User>,
) -> Response {
    let query = ListQuery::read(raw.as_deref());
    let view = match query.view.as_str() {
        "list" => pages::View::List,
        _ => pages::View::Cards,
    };
    let terms = search::parse(&query.q);
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
    let found: Vec<Schematic> = search::matching(&terms, all)
        .into_iter()
        .filter(|s| has_tags(s, &query.tags))
        .collect();
    let facets = facets(&categories, &found, &query.category);
    let matched = found.len();
    let mut shown: Vec<Schematic> = if query.category.is_empty() {
        found
    } else {
        found
            .into_iter()
            .filter(|s| in_category(s, &query.category))
            .collect()
    };
    order(&mut shown, &query.sort);
    pages::schematics(
        user_opt.as_ref(),
        &pages::Listing {
            shown: &shown,
            total,
            matched,
            view,
            sort: &query.sort,
            query: &query.q,
            terms: &terms,
            category: &query.category,
            tags: &query.tags,
            facets: &facets,
        },
    )
    .into_response()
}

/// Whether `s` carries a tag from every group, a group being one `tag`
/// value with its alternatives separated by commas. A tag matches by name
/// or token.
fn has_tags(s: &Schematic, groups: &[String]) -> bool {
    groups.iter().all(|group| {
        group.split(',').map(schematic::tag_name).any(|wanted| {
            s.tags.iter().any(|tag| {
                tag.name == wanted || tag.token.as_deref() == Some(&*wanted)
            })
        })
    })
}

/// Orders builds by `sort`: fields separated by commas, each with `-` for
/// descending, such as `-downloads,volume`. The fields are `added_at`,
/// `volume`, `latency`, and `downloads`; an unknown latency goes last
/// either way. Ties, and an empty sort, fall back to newest first.
fn order(shown: &mut [Schematic], sort: &str) {
    shown.sort_by(|a, b| {
        for key in sort.split(',').map(str::trim).filter(|k| !k.is_empty()) {
            let (field, descending) = key
                .strip_prefix('-')
                .map_or((key, false), |field| (field, true));
            let ordering = match field {
                "added_at" => a.added_at.cmp(&b.added_at),
                "volume" => a.volume().cmp(&b.volume()),
                "downloads" => a.downloads.cmp(&b.downloads),
                "latency" => match (a.latency, b.latency) {
                    (Some(x), Some(y)) => x.cmp(&y),
                    (None, Some(_)) => return Ordering::Greater,
                    (Some(_), None) => return Ordering::Less,
                    (None, None) => Ordering::Equal,
                },
                _ => Ordering::Equal,
            };
            let ordering = if descending {
                ordering.reverse()
            } else {
                ordering
            };
            if ordering != Ordering::Equal {
                return ordering;
            }
        }
        b.added_at.cmp(&a.added_at)
    });
}

/// The sidebar's name for every category off the standard list.
const OTHER: &str = "other";

/// Whether `s` belongs under the sidebar's `category`: its own category, or
/// `other` for any category off the standard list. Commas give several.
fn in_category(s: &Schematic, category: &str) -> bool {
    category.split(',').any(|category| {
        if category == OTHER {
            s.category_label.is_none()
        } else {
            s.category == category
        }
    })
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
        Ok(None) => return super::not_found(user_opt.as_ref()),
        Err(err) => return internal_error(err).into_response(),
    };
    let analysis = match schematic::data(&state.db, &id).await {
        Ok(data) => analyse(data.as_deref().unwrap_or_default()),
        Err(err) => return internal_error(err).into_response(),
    };
    pages::schematic(user_opt.as_ref(), &s, &analysis).into_response()
}

/// The page where a mod, or one of the build's confirmed builders, edits
/// its details and tags.
pub async fn edit(
    Path(id): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Response {
    let user = match editor(&state.db, user_opt, &id).await {
        Ok(user) => user,
        Err(response) => return response,
    };
    show_edit(&state.db, &user, &id, None, "").await
}

/// The account editing build `id`, or the answer when it may not: a login
/// for no account, a 404 for no build, and a 403 for an account that is
/// neither a mod nor one of its confirmed builders.
async fn editor(
    db: &SqlitePool,
    user_opt: Option<User>,
    id: &str,
) -> Result<User, Response> {
    let Some(user) = user_opt else {
        return Err(Redirect::to("/sessions/new").into_response());
    };
    match schematic::get(db, id).await {
        Ok(Some(s)) if user.may_edit(&s) => Ok(user),
        Ok(Some(_)) => Err(StatusCode::FORBIDDEN.into_response()),
        Ok(None) => Err(super::not_found(Some(&user))),
        Err(err) => Err(internal_error(err).into_response()),
    }
}

/// Saves an edit of a build's details and tags, by a mod or one of its
/// builders, then shows its page; a refusal shows the edit page again with
/// the values sent.
pub async fn save(
    Path(id): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
    body: axum::body::Bytes,
) -> Response {
    let user = match editor(&state.db, user_opt, &id).await {
        Ok(user) => user,
        Err(response) => return response,
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
    let flags =
        schematic::keep_flags(db, &category.name, &parsed.flags).await?;
    // Only a category that names each build keeps a given name.
    if !category.named {
        details.given_name = None;
    }
    let mut tx = db.begin().await?;
    schematic::set_details(&mut *tx, id, &details).await?;
    schematic::set_stacking(&mut tx, id, &parsed.stacking).await?;
    schematic::set_flags(&mut tx, id, &flags).await?;
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
        Ok((None, ..)) => super::not_found(Some(user)),
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
