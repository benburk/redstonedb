use std::{cmp::Ordering, collections::HashMap};

use axum::{
    extract::{Multipart, Path, Query, RawQuery, State},
    http::{Method, header},
    response::{IntoResponse, Redirect, Response},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    app_state::AppState,
    details,
    error::AppError,
    extractors::{
        fixi::Fixi,
        session::{Admin, SignedIn},
    },
    import::{self, Analysis},
    models::{
        catalogue::{self, Category},
        schematic::{self, Schematic},
        similarity, simulation, tag,
        user::User,
        version::{self, Replacing},
    },
    schem::Blocks,
    search, stack, standardize, thumbnail,
    version::{Checked, check as check_version},
};

use super::{
    form::{self, Form},
    pages::{self, DetailsPage, File},
    upload,
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
) -> Result<Response, AppError> {
    let query = ListQuery::read(raw.as_deref());
    let view = match query.view.as_str() {
        "list" => pages::View::List,
        _ => pages::View::Cards,
    };
    let terms = search::parse(&query.q);
    let all = schematic::all(&state.db).await?;
    let categories = catalogue::categories(&state.db).await?;
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
    Ok(pages::schematics(
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
    .into_response())
}

/// Whether `s` carries a tag from every group, a group being one `tag`
/// value with its alternatives separated by commas. A tag matches by name
/// or token.
fn has_tags(s: &Schematic, groups: &[String]) -> bool {
    groups.iter().all(|group| {
        group.split(',').map(tag::name).any(|wanted| {
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
            section: f.section,
        })
        .collect();
    facets.push(pages::Facet {
        name: OTHER.to_owned(),
        label: "Other".to_owned(),
        count: count(OTHER),
        section: None,
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
) -> Result<Response, AppError> {
    let data = version::current_file(&state.db, &id)
        .await?
        .ok_or(AppError::NotFound)?;
    let png = thumbnail::render(&Blocks::read(&data)?)?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        png,
    )
        .into_response())
}

/// The builds build `id` is similar to, with how alike, the most alike
/// first.
async fn similar_to(
    db: &SqlitePool,
    id: &str,
) -> anyhow::Result<Vec<(Schematic, u32)>> {
    let similar = similarity::similar(db, id).await?;
    Ok(super::similar::builds(db, &similar).await?)
}

/// Which part of a build's page fixi asks for, to redraw it.
#[derive(Deserialize)]
pub struct Panel {
    panel: Option<String>,
}

/// A build's page, or the one part of it fixi asks for, loading only what
/// that part shows.
pub async fn detail(
    Path(id): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
    Fixi(fixi): Fixi,
    Query(Panel { panel }): Query<Panel>,
) -> Result<Response, AppError> {
    let db = &state.db;
    let s = schematic::get(db, &id).await?.ok_or(AppError::NotFound)?;
    let simulation = simulation::status(db, &s).await?;
    let report = match &simulation {
        simulation::Status::Done(report)
        | simulation::Status::Pending(Some(report)) => Some(report),
        simulation::Status::Pending(None) | simulation::Status::Untested => {
            None
        }
    };
    let panel = panel.as_deref().filter(|_| fixi);
    if panel == Some("banner") {
        let simulating =
            matches!(simulation, simulation::Status::Pending(None));
        return Ok(pages::build_banner(&s, report, simulating).into_response());
    }
    let data = version::current_file(db, &id).await?.unwrap_or_default();
    let file = Blocks::read(&data)
        .map(|blocks| pages::FileRead {
            analysis: Analysis::of(&blocks, &s.category, s.conventional),
            stacking: s
                .stacking
                .iter()
                .filter_map(|&direction| {
                    stack::review(
                        &data,
                        direction,
                        stack::Buses::of(&s.category, s.orientation),
                    )
                    .ok()
                })
                .collect(),
        })
        .map_err(|err| err.to_string());
    let similar = similar_to(db, &id).await?;
    let flags: Vec<_> = catalogue::flags(db)
        .await?
        .into_iter()
        .filter(|flag| flag.category == s.category)
        .collect();
    if panel == Some("info") {
        return Ok(pages::build_info(&s, &flags, &similar, &file, report)
            .into_response());
    }
    let versions = version::list(db, &id).await?;
    Ok(pages::schematic_page(
        user_opt.as_ref(),
        &s,
        &flags,
        &similar,
        &file,
        &simulation,
        &versions,
    )
    .into_response())
}

/// The page where a mod, or one of the build's confirmed builders, edits
/// its details and tags.
pub async fn edit(
    Path(id): Path<String>,
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
) -> Result<Response, AppError> {
    let s = editable(&state.db, &user, &id).await?;
    show_edit(&state.db, &user, &s, None, "", None, None).await
}

/// Build `id`, when `user` may edit it: a mod, one of its confirmed
/// builders, or its uploader. A 404 for no build, and a 403 for anyone
/// else.
pub(super) async fn editable(
    db: &SqlitePool,
    user: &User,
    id: &str,
) -> Result<Schematic, AppError> {
    let s = schematic::get(db, id).await?.ok_or(AppError::NotFound)?;
    if user.may_edit(&s) {
        Ok(s)
    } else {
        Err(AppError::Forbidden)
    }
}

/// Saves an edit of a build's details and tags, by a mod or one of its
/// builders, then shows its page; a refusal shows the edit page again with
/// the values sent.
pub async fn save(
    Path(id): Path<String>,
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    mut multipart: Multipart,
) -> Result<Response, AppError> {
    let db = &state.db;
    let s = editable(db, &user, &id).await?;
    let mut posted = match upload::posted(&mut multipart).await {
        Ok(posted) => posted,
        Err(message) => {
            return show_edit(db, &user, &s, None, "", None, Some(&message))
                .await;
        }
    };
    let form = Form::read(&posted.pairs);
    let note: String = posted.get("note").trim().chars().take(500).collect();
    // A new version, when the page carried one, is checked again first.
    let checked = match attached(db, &s, posted.get("data")).await {
        Ok(checked) => checked,
        Err(message) => {
            return show_edit(
                db,
                &user,
                &s,
                Some(&form),
                "",
                None,
                Some(&message),
            )
            .await;
        }
    };
    if posted.choosing() {
        let chosen = match posted.fetch_own(db, &user).await {
            Ok(()) => chosen_version(db, &s, &posted).await,
            Err(message) => Err(message),
        };
        let (checked, refused) = match &chosen {
            Ok(chosen) => (Some(chosen), None),
            Err(message) => (checked.as_ref(), Some(message.as_str())),
        };
        let new_file = checked.map(|checked| NewFile {
            checked,
            note: &note,
        });
        return show_edit(db, &user, &s, Some(&form), "", new_file, refused)
            .await;
    }
    let new_file = || {
        checked.as_ref().map(|checked| NewFile {
            checked,
            note: &note,
        })
    };
    let refused = if checked.as_ref().is_some_and(|c| !c.refusals.is_empty()) {
        Some(
            "The new version can't replace the build, as the checks say."
                .to_owned(),
        )
    } else {
        let new_data = checked.as_ref().map(|c| c.prepared.data.as_slice());
        saved(db, &user, &s, &form, new_data).await?.err()
    };
    if let Some(message) = refused {
        return show_edit(
            db,
            &user,
            &s,
            Some(&form),
            &message,
            new_file(),
            None,
        )
        .await;
    }
    if let Some(checked) = &checked {
        let how = Replacing::NextVersion {
            added_by: Some(user.id),
            note: Some(note.as_str()).filter(|note| !note.is_empty()),
        };
        version::replace(db, &id, &checked.prepared, how).await?;
        tracing::info!(id, by = user.username, "new version");
    }
    // A new file, category, or width may call for a test, or another.
    simulation::refresh_soon(db, &id);
    similarity::refresh_soon(db);
    Ok(Redirect::to(&format!("/schematics/{id}")).into_response())
}

/// The file newly chosen on build `s`'s edit page, checked as its next
/// version, or why it could not be read.
async fn chosen_version(
    db: &SqlitePool,
    s: &Schematic,
    posted: &upload::Posted,
) -> Result<Checked, String> {
    let chosen = posted.chosen().await?;
    check_version(db, s, &chosen.file, chosen.standardize)
        .await
        .map_err(|err| format!("Could not read that file: {err:#}."))
}

/// The new version an edit page carried in `data`, base64, checked against
/// build `s`; none for an empty `data`. The `Err` says the file cannot be
/// read, for the page to ask for it again.
async fn attached(
    db: &SqlitePool,
    s: &Schematic,
    data: &str,
) -> Result<Option<Checked>, String> {
    if data.is_empty() {
        return Ok(None);
    }
    let checked = match STANDARD.decode(data) {
        Ok(data) => check_version(db, s, &data, None).await,
        Err(_) => Err(anyhow::anyhow!("it did not arrive whole")),
    };
    checked.map(Some).map_err(|err| {
        format!(
            "Could not read the new version: {err:#}. Choose its file again."
        )
    })
}

/// Deletes a build for good, with its tags, credits, and stacking; admins
/// only.
pub async fn delete(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Admin(admin): Admin,
) -> Result<Response, AppError> {
    schematic::delete(&state.db, &id).await?;
    tracing::info!(id, by = admin.username, "deleted a build");
    Ok(Redirect::to("/").into_response())
}

/// Checks and stores `user`'s edit of build `s`, or says why it was
/// refused. A new version's file, `new_data`, is what the stacking must
/// fit, when there is one.
async fn saved(
    db: &SqlitePool,
    user: &User,
    s: &Schematic,
    form: &Form,
    new_data: Option<&[u8]>,
) -> Result<Result<(), String>, AppError> {
    let categories = catalogue::categories(db).await?;
    let given = match form.parse(&categories) {
        Ok(given) => given,
        Err(message) => return Ok(Err(message)),
    };
    let stored;
    let data = if let Some(data) = new_data {
        data
    } else {
        stored = version::current_file(db, &s.id).await?.unwrap_or_default();
        &stored
    };
    let build = details::Build {
        data,
        conventional: s.conventional,
    };
    match details::check(db, given, build, user.is_admin()).await? {
        Ok(ready) => Ok(Ok(schematic::set_details(db, &s.id, &ready).await?)),
        Err(message) => Ok(Err(message)),
    }
}

/// A new version waiting on the edit page, to save with the details.
pub(super) struct NewFile<'a> {
    pub checked: &'a Checked,
    pub note: &'a str,
}

/// The edit page for build `s`: its details as `sent`, else as stored,
/// with `message` saying why a save was refused, and the file a new
/// version would replace it with, if one is waiting. `choosing` opens the
/// form for choosing one.
pub(super) async fn show_edit(
    db: &SqlitePool,
    user: &User,
    s: &Schematic,
    sent: Option<&Form>,
    message: &str,
    new_file: Option<NewFile<'_>>,
    choosing: Option<&str>,
) -> Result<Response, AppError> {
    let choices = form::choices(db).await?;
    let versions = version::list(db, &s.id).await?;
    // What the page shows is the new file's, when one is waiting; a file
    // that cannot be read shows no inputs and outputs.
    let analysis = if let Some(new) = &new_file {
        let blocks = &new.checked.prepared.blocks;
        Analysis::of(blocks, &s.category, s.conventional)
    } else {
        let data = version::current_file(db, &s.id).await?.unwrap_or_default();
        Blocks::read(&data).map_or_else(
            |_| Analysis::default(),
            |blocks| Analysis::of(&blocks, &s.category, s.conventional),
        )
    };
    // An adder's width and orientation come from its file, whatever its
    // category is now, in case it becomes one.
    let derived = if s.conventional {
        import::derived("adder", &analysis.interface)
    } else {
        import::Derived::default()
    };
    let simulation = simulation::current(db, s).await?;
    let folder = super::schemati::own_folder_quickly(db, user).await?;
    let stored = Form::of(s);
    let form = sent.unwrap_or(&stored);
    let encoded;
    let file = match &new_file {
        Some(new) => {
            encoded = STANDARD.encode(&new.checked.prepared.data);
            File::Version {
                s,
                data: &encoded,
                comparison: new.checked.comparison,
                refusals: &new.checked.refusals,
                version: versions.first().map_or(1, |v| v.version) + 1,
                note: new.note,
            }
        }
        None => File::Stored {
            s,
            simulation: simulation.as_ref(),
        },
    };
    Ok(pages::details_page(
        user,
        &DetailsPage {
            file,
            analysis: &analysis,
            form,
            choices: &choices,
            derived,
            message,
            choosing,
            schemati: &super::schemati::connection(folder.as_ref()),
        },
    )
    .into_response())
}

/// The stored `.schem` file, named by the naming scheme. The viewer fetches
/// the same URL.
pub async fn download(
    Path(id): Path<String>,
    State(state): State<AppState>,
    method: Method,
    Query(mut query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    // The 3D viewer fetches the file with `preview=yes`; that, and a HEAD
    // request, is no download.
    let preview = query.remove("preview").is_some();
    let counted = !preview && method == Method::GET;
    let (filename, data) = export(&state.db, &id, query, counted).await?;
    // Only readable files are stored, so the viewer's copy falls back to
    // the file as it is only if that changes.
    let data = if preview {
        standardize::for_viewer(&data).unwrap_or(data)
    } else {
        data
    };
    Ok(super::attachment(&filename, data))
}

/// Build `id`'s file as the download dialog's `query` asks, with its name,
/// counted as a download if `counted`.
pub(super) async fn export(
    db: &SqlitePool,
    id: &str,
    mut query: HashMap<String, String>,
    counted: bool,
) -> Result<(String, Vec<u8>), AppError> {
    // `markers=yes` adds WorldEdit corner markers; the rest of the query
    // names copies per direction, such as `?west=7&north=3`.
    let markers = query.remove("markers").is_some_and(|v| v == "yes");
    // `part=without` or `part=first`, with a `direction` it stacks, is one
    // of the files the in-game stacking procedure pastes: the build without
    // the parts only its first copy keeps, or those parts.
    let part = query.remove("part");
    let part_direction = query.remove("direction");
    // One copy each way is the build itself.
    query.retain(|_, count| count != "1");
    let s = schematic::get(db, id).await?.ok_or(AppError::NotFound)?;
    let data = version::current_file(db, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if let Some(part) = part {
        let first = part == "first";
        let direction = s
            .stacking
            .iter()
            .find(|d| part_direction.as_deref() == Some(d.name().as_str()));
        let (Some(&direction), "first" | "without") =
            (direction, part.as_str())
        else {
            return Err(AppError::BadRequest("no such part".to_owned()));
        };
        let buses = stack::Buses::of(&s.category, s.orientation);
        let data = stack::part(&data, direction, first, buses)?;
        let suffix = if first {
            "_first_copy_parts"
        } else {
            "_without_first_copy_parts"
        };
        let filename =
            s.filename().replace(".schem", &format!("{suffix}.schem"));
        return Ok((filename, data));
    }
    let (data, s) = if query.is_empty() {
        (data, s)
    } else {
        let stacked = query
            .iter()
            .map(|(name, count)| Ok((name.clone(), count.parse()?)))
            .collect::<anyhow::Result<HashMap<String, usize>>>()
            .and_then(|wanted| stack::plan(&s.stacking, &wanted))
            .and_then(|plan| {
                let buses = stack::Buses::of(&s.category, s.orientation);
                let (data, size) = stack::stack(&data, &plan, buses)?;
                Ok((data, s.stacked(&plan, size)))
            });
        stacked.map_err(|err| AppError::BadRequest(err.to_string()))?
    };
    if counted {
        schematic::count_download(db, id).await?;
    }
    let data = if markers {
        standardize::with_corner_markers(&data)?
    } else {
        data
    };
    Ok((s.filename(), data))
}
