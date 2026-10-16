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
    error::AppError,
    extractors::{
        fixi::Fixi,
        session::{Admin, SignedIn},
    },
    import,
    interface::{self, Interface},
    lint,
    models::schematic::{self, Category, Credit, Measure, Schematic},
    models::user::User,
    models::{similarity, simulation, version},
    mojang,
    schem::Blocks,
    search, simulate, stack, standardize, thumbnail,
    version::{Checked, check as check_version},
};

use super::{
    details,
    pages::{self, Choices, DetailsPage, File, Stored},
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
    let categories = schematic::categories(&state.db).await?;
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
            section: f.section.clone(),
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
    let data = schematic::data(&state.db, &id)
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

/// The builds build `id` is similar to, with how alike.
async fn similar_to(
    db: &SqlitePool,
    id: &str,
) -> anyhow::Result<Vec<(Schematic, u32)>> {
    let mut found = Vec::new();
    for (other, permille) in similarity::similar(db, id).await? {
        if let Some(s) = schematic::get(db, &other).await? {
            found.push((s, permille));
        }
    }
    Ok(found)
}

/// Which part of a build's page fixi asks for, to redraw it.
#[derive(Deserialize)]
pub struct Panel {
    panel: Option<String>,
}

pub async fn detail(
    Path(id): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
    Fixi(fixi): Fixi,
    Query(Panel { panel }): Query<Panel>,
) -> Result<Response, AppError> {
    let s = schematic::get(&state.db, &id)
        .await?
        .ok_or(AppError::NotFound)?;
    let data = schematic::data(&state.db, &id).await?.unwrap_or_default();
    let analysis = analyse(&data, &s.category).map(|mut analysis| {
        analysis.stacking = s
            .stacking
            .iter()
            .filter_map(|&direction| {
                stack::review(
                    &data,
                    direction,
                    stack::Buses::of(&s.category, &s.orientation),
                )
                .ok()
            })
            .collect();
        analysis
    });
    let simulation = simulation::status(&state.db, &s).await?;
    let versions = version::list(&state.db, &id).await?;
    let similar = similar_to(&state.db, &id).await?;
    let flags: Vec<_> = schematic::flags(&state.db)
        .await?
        .into_iter()
        .filter(|flag| flag.category == s.category)
        .collect();
    let report = match &simulation {
        simulation::Status::Done(report)
        | simulation::Status::Pending(Some(report)) => Some(report),
        simulation::Status::Pending(None) | simulation::Status::Untested => {
            None
        }
    };
    Ok(match panel.as_deref().filter(|_| fixi) {
        Some("banner") => {
            let simulating =
                matches!(simulation, simulation::Status::Pending(None));
            pages::build_banner(&s, report, simulating).into_response()
        }
        Some("info") => {
            pages::build_info(&s, &flags, &similar, &analysis, report)
                .into_response()
        }
        _ => pages::schematic_page(
            user_opt.as_ref(),
            &s,
            &flags,
            &similar,
            &analysis,
            &simulation,
            &versions,
        )
        .into_response(),
    })
}

/// The page where a mod, or one of the build's confirmed builders, edits
/// its details and tags.
pub async fn edit(
    Path(id): Path<String>,
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
) -> Result<Response, AppError> {
    let user = editor(&state.db, user, &id).await?;
    show_edit(&state.db, &user, &id, None, "", None, None).await
}

/// `user`, when they may edit build `id`: a mod, or one of its confirmed
/// builders. A 404 for no build, and a 403 for anyone else.
pub(super) async fn editor(
    db: &SqlitePool,
    user: User,
    id: &str,
) -> Result<User, AppError> {
    let s = schematic::get(db, id).await?.ok_or(AppError::NotFound)?;
    if user.may_edit(&s) {
        Ok(user)
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
    let user = editor(&state.db, user, &id).await?;
    let db = &state.db;
    let mut posted = match upload::posted(&mut multipart).await {
        Ok(posted) => posted,
        Err(message) => {
            return show_edit(db, &user, &id, None, "", None, Some(&message))
                .await;
        }
    };
    let form = details::Form::read(&posted.pairs);
    let note: String = posted.get("note").trim().chars().take(500).collect();
    // A new version, when the page carried one, is checked again first.
    let checked = match attached(db, &id, posted.get("data")).await? {
        Ok(checked) => checked,
        Err(message) => {
            return show_edit(
                db,
                &user,
                &id,
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
            Ok(()) => chosen_version(db, &id, &posted).await,
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
        return show_edit(db, &user, &id, Some(&form), "", new_file, refused)
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
        saved(db, &id, &form, new_data, user.is_admin())
            .await?
            .err()
    };
    if let Some(message) = refused {
        return show_edit(
            db,
            &user,
            &id,
            Some(&form),
            &message,
            new_file(),
            None,
        )
        .await;
    }
    if let Some(checked) = &checked {
        let size = &checked.prepared.size;
        let new = version::New {
            data: &checked.prepared.data,
            sha256: &checked.prepared.sha256,
            size: (size.x.into(), size.y.into(), size.z.into()),
            facts: &checked.prepared.facts,
            added_by: Some(user.id),
            note: Some(note.as_str()).filter(|note| !note.is_empty()),
        };
        version::publish(db, &id, &new).await?;
        tracing::info!(id, by = user.username, "new version");
    }
    // A new file, category, or width may call for a test, or another.
    simulation::refresh_soon(db, &id);
    similarity::refresh_soon(db);
    Ok(Redirect::to(&format!("/schematics/{id}")).into_response())
}

/// The file newly chosen on build `id`'s edit page, checked as its next
/// version, or why it could not be read.
async fn chosen_version(
    db: &SqlitePool,
    id: &str,
    posted: &upload::Posted,
) -> Result<Checked, String> {
    let chosen = posted.chosen().await?;
    let s = match schematic::get(db, id).await {
        Ok(Some(s)) => s,
        Ok(None) => return Err("The build is gone.".to_owned()),
        Err(err) => return Err(format!("Could not load the build: {err}.")),
    };
    check_version(db, &s, &chosen.file, chosen.standardize)
        .await
        .map_err(|err| format!("Could not read that file: {err:#}."))
}

/// The new version an edit page carried in `data`, base64, checked against
/// build `id`; none for an empty `data`. The inner `Err` says the file
/// cannot be read, for the page to ask for it again.
async fn attached(
    db: &SqlitePool,
    id: &str,
    data: &str,
) -> Result<Result<Option<Checked>, String>, AppError> {
    if data.is_empty() {
        return Ok(Ok(None));
    }
    let s = schematic::get(db, id).await?.ok_or(AppError::NotFound)?;
    let checked = match STANDARD.decode(data) {
        Ok(data) => check_version(db, &s, &data, None).await,
        Err(_) => Err(anyhow::anyhow!("it did not arrive whole")),
    };
    Ok(checked.map(Some).map_err(|err| {
        format!(
            "Could not read the new version: {err:#}. Choose its file again."
        )
    }))
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

/// Checks and stores an edit, or says why it was refused.
async fn saved(
    db: &SqlitePool,
    id: &str,
    form: &details::Form,
    new_data: Option<&[u8]>,
    may_create_tags: bool,
) -> Result<Result<(), String>, sqlx::Error> {
    let categories = schematic::categories(db).await?;
    let parsed = match form.parse(&categories) {
        Ok(parsed) => parsed,
        Err(message) => return Ok(Err(message)),
    };
    if let Err(message) =
        schematic::check_new_tags(db, &form.tags, may_create_tags).await?
    {
        return Ok(Err(message));
    }
    let category =
        match schematic::check_category(db, &parsed.category, &parsed.design)
            .await?
        {
            Ok(category) => category,
            Err(message) => return Ok(Err(message)),
        };
    // The new version, when there is one, is what the stacking must fit.
    let data = match new_data {
        Some(data) => Some(data.to_vec()),
        None => schematic::data(db, id).await?,
    };
    let conventional = schematic::get(db, id)
        .await?
        .is_some_and(|s| s.conventional);
    // The simulation decides how a recognized build stacks, so its form's
    // stacking counts for nothing and what it found stays.
    let inferred = conventional
        && simulate::INFERS_STACKING.contains(&parsed.category.as_str());
    if let Some(data) = data.filter(|_| !inferred) {
        let buses = stack::Buses::of(&parsed.category, &parsed.orientation);
        let checked = stack::check(&data, &parsed.stacking, buses);
        if let Err(message) = checked {
            return Ok(Err(message));
        }
    }
    let mut details = parsed.as_details();
    let flags =
        schematic::keep_flags(db, &category.name, &parsed.flags).await?;
    // Only a category that names each build keeps a given name.
    if !category.named {
        details.given_name = None;
    }
    // Likewise, only a category ranked by a measure keeps it, as a memory
    // does its capacity.
    if !category.has(Measure::Bytes) {
        details.bytes = None;
    }
    if !category.has(Measure::AtLeast) {
        details.at_least = None;
    }
    // Credited as their players spell them, asked before the transaction
    // opens so it is not held across the lookup.
    let (builders, contributors) = match (
        mojang::respell(&parsed.builders).await,
        mojang::respell(&parsed.contributors).await,
    ) {
        (Ok(builders), Ok(contributors)) => (builders, contributors),
        (Err(message), _) | (_, Err(message)) => return Ok(Err(message)),
    };
    let mut tx = db.begin().await?;
    schematic::set_details(&mut *tx, id, &details).await?;
    if !inferred {
        schematic::set_stacking(&mut tx, id, &parsed.stacking).await?;
    }
    schematic::set_flags(&mut tx, id, &flags).await?;
    let credits = [
        (Credit::Builder, &builders),
        (Credit::Contributor, &contributors),
    ];
    for (credit, names) in credits {
        schematic::set_credits(&mut tx, id, credit, names).await?;
    }
    schematic::set_tags(&mut tx, id, &form.tags).await?;
    tx.commit().await?;
    Ok(Ok(()))
}

/// A new version waiting on the edit page, to save with the details.
pub(super) struct NewFile<'a> {
    pub checked: &'a Checked,
    pub note: &'a str,
}

/// The edit page for build `id`: its details as `sent`, else as stored,
/// with `message` saying why a save was refused, and the file a new
/// version would replace it with, if one is waiting. `choosing` opens the
/// form for choosing one.
pub(super) async fn show_edit(
    db: &SqlitePool,
    user: &User,
    id: &str,
    sent: Option<&details::Form>,
    message: &str,
    new_file: Option<NewFile<'_>>,
    choosing: Option<&str>,
) -> Result<Response, AppError> {
    let s = schematic::get(db, id).await?.ok_or(AppError::NotFound)?;
    let choices = Choices::load(db).await?;
    let tags = schematic::all_tags(db).await?;
    let data = schematic::data(db, id).await?.unwrap_or_default();
    let versions = version::list(db, id).await?;
    // What the page shows is the new file's, when one is waiting.
    let analysis = match &new_file {
        Some(new) => pages::Analysis {
            interface: {
                let mut read = interface::read(&new.checked.prepared.blocks);
                simulate::name_lines(
                    &s.category,
                    &new.checked.prepared.blocks,
                    &mut read,
                );
                read
            },
            violations: if s.conventional {
                lint::check_for(&new.checked.prepared.blocks, &s.category)
            } else {
                Vec::new()
            },
            stacking: Vec::new(),
        },
        None => {
            analyse(&data, &s.category).unwrap_or_else(|_| pages::Analysis {
                interface: Interface {
                    inputs: Vec::new(),
                    outputs: Vec::new(),
                },
                violations: Vec::new(),
                stacking: Vec::new(),
            })
        }
    };
    // An adder's width and orientation come from its file, whatever its
    // category is now, in case it becomes one.
    let derived = if s.conventional {
        import::derived("adder", &analysis.interface)
    } else {
        import::Derived::default()
    };
    let simulation = simulation::current(db, &s).await?;
    let folder = super::schemati::own_folder_quickly(db, user).await?;
    let stored = Stored::of(&s);
    let (fields, chosen_tags) = match sent {
        Some(form) => (form.fields(), form.tags.as_slice()),
        None => (stored.fields(&s), stored.tags.as_slice()),
    };
    let encoded;
    let file = match &new_file {
        Some(new) => {
            encoded = STANDARD.encode(&new.checked.prepared.data);
            File::Version {
                s: &s,
                data: &encoded,
                comparison: new.checked.comparison,
                refusals: &new.checked.refusals,
                version: versions.first().map_or(1, |v| v.version) + 1,
                note: new.note,
            }
        }
        None => File::Stored {
            s: &s,
            simulation: simulation.as_ref(),
        },
    };
    Ok(pages::details_page(
        user,
        &DetailsPage {
            file,
            analysis: &analysis,
            fields: &fields,
            chosen_tags,
            choices: &choices,
            tags: &tags,
            may_create_tags: user.is_admin(),
            derived,
            message,
            choosing,
            schemati: &super::schemati::connection(folder.as_ref()),
        },
    )
    .into_response())
}

/// The build's inputs and outputs, named as `category` makes them, and
/// the conventions it breaks, or why its file could not be read.
fn analyse(data: &[u8], category: &str) -> Result<pages::Analysis, String> {
    let blocks = Blocks::read(data).map_err(|err| err.to_string())?;
    let mut interface = interface::read(&blocks);
    simulate::name_lines(category, &blocks, &mut interface);
    Ok(pages::Analysis {
        interface,
        violations: lint::check(&blocks),
        stacking: Vec::new(),
    })
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
    let data = schematic::data(db, id).await?.ok_or(AppError::NotFound)?;
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
        let buses = stack::Buses::of(&s.category, &s.orientation);
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
                let buses = stack::Buses::of(&s.category, &s.orientation);
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
