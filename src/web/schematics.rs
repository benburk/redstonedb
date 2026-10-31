//! A stored build's routes: the listing, its page, its file and thumbnail,
//! editing its details, and deleting it. A download is counted unless it is
//! the 3D viewer's `?preview=yes` fetch or a HEAD request.

use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    sync::Arc,
};

use axum::{
    body::Bytes,
    extract::{Multipart, Path, Query, RawQuery, State},
    http::{Method, header},
    response::{IntoResponse, Redirect, Response},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    app_state::AppState,
    category, details,
    error::AppError,
    extractors::{
        client_ip::ClientIp,
        fixi::Fixi,
        session::{Moderator, SignedIn},
    },
    import::{self, Analysis, Prepared},
    leaderboard,
    models::{
        catalogue::{self, Category},
        schematic::{self, Schematic},
        similarity, simulation, tag,
        user::User,
        version::{self, Replacing},
    },
    palette::{self, Palette},
    schem::Blocks,
    search, stack, standardize, thumbnail,
    version::{Checked, check as check_version},
};

use super::{
    form::{self, Form},
    pages::{self, File},
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
    /// The file a search for similar builds was for, and what it found;
    /// see `similar::shown_page`.
    file: String,
    found: Option<String>,
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
                "file" => query.file = value,
                "found" => query.found = Some(value),
                _ => {}
            }
        }
        query
    }
}

/// The home page: every category until something is searched for or
/// chosen, then the builds found.
pub async fn home(
    State(state): State<AppState>,
    RawQuery(raw): RawQuery,
    Fixi(fixi): Fixi,
    user_opt: Option<User>,
) -> Result<Response, AppError> {
    let query = ListQuery::read(raw.as_deref());
    if let Some(found) = &query.found {
        return super::similar::shown_page(
            &state,
            user_opt.as_ref(),
            &query.file,
            found,
        )
        .await;
    }
    if query.q.is_empty() && query.category.is_empty() && query.tags.is_empty()
    {
        return index(&state, fixi, user_opt.as_ref()).await;
    }
    listing(&state, query, fixi, user_opt.as_ref()).await
}

/// Every build, as the home page lists them once a search is cleared.
pub async fn list(
    State(state): State<AppState>,
    RawQuery(raw): RawQuery,
    Fixi(fixi): Fixi,
    user_opt: Option<User>,
) -> Result<Response, AppError> {
    let query = ListQuery::read(raw.as_deref());
    listing(&state, query, fixi, user_opt.as_ref()).await
}

/// How many builds the home page shows as recently added.
const RECENT: usize = 8;

/// The home page with nothing searched for: each category, pictured by its
/// best build, and the builds added last. Through fixi, as when a search
/// is cleared, only the part below the search.
async fn index(
    state: &AppState,
    fixi: bool,
    viewer: Option<&User>,
) -> Result<Response, AppError> {
    let all = schematic::all(&state.db).await?;
    let categories = catalogue::categories(&state.db).await?;
    let palette = palette::key_for(&state.db, viewer).await?;
    let index = pages::Index {
        tiles: &tiles(&categories, &all),
        recent: &recent(&all),
        total: all.len(),
        palette: palette.as_deref(),
    };
    if fixi {
        return Ok(([("FX-Replace-Url", "/")], pages::home_index(&index))
            .into_response());
    }
    Ok(pages::home(viewer, &index).into_response())
}

/// Each category with a build, most builds first and Other last, pictured
/// by the first on its leaderboard as it opens, the quickest of its most
/// common width, or by its newest build when none is timed.
fn tiles<'a>(
    categories: &[Category],
    all: &'a [Schematic],
) -> Vec<pages::Tile<'a>> {
    let newest = |builds: &[&'a Schematic]| {
        builds
            .iter()
            .copied()
            .max_by(|a, b| a.added_at.cmp(&b.added_at))
    };
    let mut tiles: Vec<pages::Tile> = categories
        .iter()
        .filter_map(|category| {
            let builds: Vec<&Schematic> =
                all.iter().filter(|s| s.category == category.name).collect();
            let newest = newest(&builds)?;
            let board = leaderboard::board(
                &builds,
                &leaderboard::Wanted::default(),
                &category.measures,
            );
            let leader =
                leaderboard::fastest(&board.builds).first().map(|&(_, s)| s);
            Some(pages::Tile {
                category: category.name.clone(),
                label: category.plural.clone(),
                count: builds.len(),
                best: leader.unwrap_or(newest),
                section: category.section,
            })
        })
        .collect();
    tiles.sort_by(|a, b| b.count.cmp(&a.count).then(a.label.cmp(&b.label)));
    let other: Vec<&Schematic> =
        all.iter().filter(|s| in_category(s, OTHER)).collect();
    if let Some(newest) = newest(&other) {
        tiles.push(pages::Tile {
            category: OTHER.to_owned(),
            label: "Other".to_owned(),
            count: other.len(),
            best: newest,
            section: None,
        });
    }
    tiles
}

/// The builds added last, newest first, one of each name so an import of
/// many alike takes one place, each with how many builds share its name.
fn recent(all: &[Schematic]) -> Vec<(&Schematic, usize)> {
    let mut named: HashMap<String, usize> = HashMap::new();
    for s in all {
        *named.entry(s.name()).or_default() += 1;
    }
    let mut newest: Vec<&Schematic> = all.iter().collect();
    newest.sort_by(|a, b| b.added_at.cmp(&a.added_at));
    let mut seen = HashSet::new();
    newest
        .into_iter()
        .filter(|s| seen.insert(s.name()))
        .take(RECENT)
        .map(|s| (s, named[&s.name()]))
        .collect()
}

/// The builds a search, category, and tags find, with the menu of
/// categories. Through fixi, as a search is typed, only the part below the
/// search, with the search's address.
async fn listing(
    state: &AppState,
    query: ListQuery,
    fixi: bool,
    viewer: Option<&User>,
) -> Result<Response, AppError> {
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
    let mut shown: Vec<Schematic> = if query.category.is_empty() {
        found
    } else {
        found
            .into_iter()
            .filter(|s| in_category(s, &query.category))
            .collect()
    };
    order(&mut shown, &query.sort);
    let palette = palette::key_for(&state.db, viewer).await?;
    let listing = pages::Listing {
        shown: &shown,
        total,
        view,
        sort: &query.sort,
        query: &query.q,
        terms: &terms,
        category: &query.category,
        tags: &query.tags,
        facets: &facets,
        palette: palette.as_deref(),
    };
    // A search as it is typed redraws only the results, and puts the
    // search in the address, so Back and reloading keep it.
    if fixi {
        return Ok((
            [("FX-Replace-Url", listing.href())],
            pages::home_results(&listing),
        )
            .into_response());
    }
    Ok(pages::schematics(viewer, &listing).into_response())
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

/// The menu's name for every category off the standard list.
const OTHER: &str = "other";

/// Whether `s` belongs under the menu's `category`: its own category, or
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

/// How many of `found` each category has, for the menu: the standard
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
/// cached for good. Each file is drawn once, off the async workers, as
/// drawing one takes tens of milliseconds.
pub async fn thumbnail(
    Path(id): Path<String>,
    State(state): State<AppState>,
    viewer: Option<User>,
) -> Result<Response, AppError> {
    let data = version::current_file(&state.db, &id)
        .await?
        .ok_or(AppError::NotFound)?;
    // A recognized build is drawn in the viewer's palette, if they chose
    // one, and kept apart from others' pictures of it.
    let palette = match &viewer {
        Some(user) => palette::of(&state.db, user.id).await?,
        None => None,
    };
    let painted = match &palette {
        Some(palette) => palette::needs(&state.db, &id)
            .await?
            .map(|needs| (palette, needs)),
        None => None,
    };
    let (data, key) = match &painted {
        Some((palette, needs)) => (
            palette::apply(&data, needs, palette)?,
            format!("{}-{}", import::sha256(&data), palette.key()),
        ),
        None => (data.clone(), import::sha256(&data)),
    };
    let sha256 = key;
    let cached = state.thumbnails.lock().unwrap().get(&sha256);
    // From memory, else as drawn before and kept beside the database, else
    // drawn now, fetching the game's files it needs that aren't kept.
    let kept = std::path::Path::new(thumbnail::DRAWN_DIR)
        .join(format!("{}-{sha256}.webp", thumbnail::RENDERER));
    let png = if let Some(png) = cached {
        png
    } else if let Ok(webp) = tokio::fs::read(&kept).await {
        let webp = Bytes::from(webp);
        state
            .thumbnails
            .lock()
            .unwrap()
            .insert(sha256, webp.clone());
        webp
    } else {
        let blocks = Blocks::read(&data)?;
        let renderer = Arc::clone(&state.renderer);
        let webp = tokio::task::spawn_blocking(move || {
            thumbnail::render_with(&blocks, &renderer)
        })
        .await
        .map_err(anyhow::Error::from)??;
        let _ = tokio::fs::create_dir_all(thumbnail::DRAWN_DIR).await;
        let _ = tokio::fs::write(&kept, &webp).await;
        let webp = Bytes::from(webp);
        state
            .thumbnails
            .lock()
            .unwrap()
            .insert(sha256, webp.clone());
        webp
    };
    // The address names the palette, but who may see it is the viewer's.
    let cache = if painted.is_some() {
        "private, max-age=31536000, immutable"
    } else {
        "public, max-age=31536000, immutable"
    };
    Ok((
        [
            (header::CONTENT_TYPE, "image/webp"),
            (header::CACHE_CONTROL, cache),
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

/// What build `s`'s page shows of its file, `data`, with `blocks`: as its
/// category reads it, or for one off the list while it is not recognized
/// as the category it stands in for, standardized and read as that
/// category, as recognizing it would.
fn read_for_page(s: &Schematic, data: &[u8], blocks: &Blocks) -> Analysis {
    if let Some(meant) = simulation::meant_by(&s.category)
        && let Ok(standard) =
            import::prepare(data, true, true, s.keeps_paste, true)
    {
        return Analysis::of(&standard.blocks, meant);
    }
    Analysis::of(blocks, &s.category)
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
    let unrecognized = why_not_recognized(&s, &data).await;
    let file = Blocks::read(&data)
        .map(|blocks| pages::FileRead {
            unrecognized,
            analysis: read_for_page(&s, &data, &blocks),
            block_counts: crate::standardize::block_counts(&blocks),
            strength_inputs: !crate::interface::dust_inputs(&blocks).is_empty(),
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

/// For build `s`, filed off the list as an adder or a register, what keeps
/// it from being recognized as one by the check by function on its file
/// `data`: one line for each fault. Empty for any other build.
async fn why_not_recognized(s: &Schematic, data: &[u8]) -> Vec<String> {
    let Some(meant) = crate::models::simulation::meant_by(&s.category)
        .filter(|meant| crate::category::BLESSED.contains(meant))
        .map(str::to_owned)
    else {
        return Vec::new();
    };
    let data = data.to_vec();
    // Probing simulates, so off the async threads.
    let recognized = tokio::task::spawn_blocking(move || {
        crate::probe::recognize_as(&data, Some(&meant))
    })
    .await;
    match recognized {
        Ok(Ok(recognition)) => recognition.problems,
        Ok(Err(err)) => vec![err.to_string()],
        Err(err) => vec![err.to_string()],
    }
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
    ClientIp(ip): ClientIp,
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
    let (kept, refused) =
        save_details(db, &user, &s, &form, checked.as_ref()).await?;
    let new_prepared = kept.as_ref().or(checked.as_ref().map(|c| &c.prepared));
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
    if let Some(prepared) = new_prepared {
        let how = Replacing::NextVersion {
            added_by: Some(user.id),
            added_from: ip.as_deref(),
            note: Some(note.as_str()).filter(|note| !note.is_empty()),
        };
        version::replace(db, &id, prepared, how).await?;
        tracing::info!(id, by = user.username, ip, "new version");
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
    // A listed build that follows the conventions is kept standardized;
    // one off the list as built, until
    // a version it is recognized by moves it up.
    let listed = Category::find(
        &catalogue::categories(db)
            .await
            .map_err(|err| err.to_string())?,
        &s.category,
    )
    .is_some();
    let standardize = s.conventional && listed;
    check_version(db, s, &chosen.file, standardize)
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
        Ok(data) => check_version(db, s, &data, false).await,
        Err(_) => Err(anyhow::anyhow!("it did not arrive whole")),
    };
    checked.map(Some).map_err(|err| {
        format!(
            "Could not read the new version: {err:#}. Choose its file again."
        )
    })
}

/// Deletes a build for good, with its tags, credits, and stacking; mods
/// and admins only.
pub async fn delete(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Moderator(by): Moderator,
) -> Result<Response, AppError> {
    schematic::delete(&state.db, &id).await?;
    tracing::info!(id, by = by.username, "deleted a build");
    Ok(Redirect::to("/").into_response())
}

/// Checks and stores `user`'s edit of build `s`, with the new version
/// `checked` when one waits: the file it is kept as, when that differs
/// from the one checked, and why the edit was refused, if it was. A build
/// its version moves up goes under the category the version passes.
async fn save_details(
    db: &SqlitePool,
    user: &User,
    s: &Schematic,
    form: &Form,
    checked: Option<&Checked>,
) -> Result<(Option<Prepared>, Option<String>), AppError> {
    if checked.is_some_and(|c| !c.refusals.is_empty()) {
        let why = "The new version can't replace the build, as the checks say.";
        return Ok((None, Some(why.to_owned())));
    }
    let kept = match checked {
        Some(checked) => {
            version_kept(db, s, &form.category, &checked.prepared).await?
        }
        None => None,
    };
    let (kept, moved_to) = kept.map_or((None, None), |(p, to)| (Some(p), to));
    let moved;
    let saving = match moved_to {
        Some(category) => {
            moved = Form {
                category,
                ..form.clone()
            };
            &moved
        }
        None => form,
    };
    let new = kept.as_ref().or(checked.map(|c| &c.prepared));
    let refused = saved(db, user, s, saving, new).await?.err();
    Ok((kept, refused))
}

/// The file a new version, `prepared`, is kept as, and the category build
/// `s` goes up under with it, when either differs from what was given:
/// a build off the list whose new version is recognized as the listed
/// category its name means, as an 8-bit adder's as an adder, moves up,
/// its file standardized and turned to lie the standard way, as an
/// upload's is.
/// Only a build whose category, as `category` gives it, is unchanged
/// moves up.
async fn version_kept(
    db: &SqlitePool,
    s: &Schematic,
    category: &str,
    prepared: &Prepared,
) -> anyhow::Result<Option<(Prepared, Option<String>)>> {
    let categories = catalogue::categories(db).await?;
    let evaluated = |c: &&Category| category::behaviour(&c.name).evaluated;
    if Category::name_of(&categories, category) == s.category
        && let Some((moved, to)) =
            import::moved_up(&categories, &s.category, prepared, s.keeps_paste)
                .await?
    {
        return Ok(Some((moved, Some(to.name.clone()))));
    }
    match Category::find(&categories, category).filter(evaluated) {
        Some(_) if prepared.conventional => {
            Ok(Some((import::rewrapped(prepared)?, None)))
        }
        _ => Ok(None),
    }
}

/// Checks and stores `user`'s edit of build `s`, or says why it was
/// refused. A new version's file, `new`, is what the stacking must fit,
/// when there is one.
async fn saved(
    db: &SqlitePool,
    user: &User,
    s: &Schematic,
    form: &Form,
    new: Option<&Prepared>,
) -> Result<Result<(), String>, AppError> {
    let given = match form.parse() {
        Ok(given) => given,
        Err(message) => return Ok(Err(message)),
    };
    let stored;
    let build = if let Some(new) = new {
        details::Build::of(new)
    } else {
        stored = version::current_file(db, &s.id).await?.unwrap_or_default();
        // Its file, kept under its category, passed its test already,
        // which a save of its details needn't run again.
        let same = Category::name_of(
            &catalogue::categories(db).await?,
            &form.category,
        ) == s.category;
        details::Build {
            data: &stored,
            conventional: s.conventional,
            passes: same
                && simulation::current(db, s)
                    .await?
                    .is_some_and(|report| report.passed()),
        }
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
    let versions = version::list(db, &s.id).await?;
    let simulation = simulation::current(db, s).await?;
    let stored = Form::of(s);
    // What the page shows is the new file's, when one is waiting.
    let (encoded, stored_blocks);
    let (file, blocks) = if let Some(new) = &new_file {
        encoded = STANDARD.encode(&new.checked.prepared.data);
        let file = File::Version {
            s,
            data: &encoded,
            comparison: new.checked.comparison,
            refusals: &new.checked.refusals,
            version: versions.first().map_or(1, |v| v.version) + 1,
            note: new.note,
        };
        (file, Some(&new.checked.prepared.blocks))
    } else {
        let data = version::current_file(db, &s.id).await?.unwrap_or_default();
        stored_blocks = Blocks::read(&data).ok();
        let file = File::Stored {
            s,
            simulation: simulation.as_ref(),
        };
        (file, stored_blocks.as_ref())
    };
    let shown = form::Shown {
        file,
        blocks,
        message,
        choosing,
    };
    form::page(db, user, sent.unwrap_or(&stored), shown).await
}

/// What a download of a build's file asks for: from the download dialog,
/// the stacking guide's links, and the 3D viewer.
#[derive(Deserialize)]
pub(super) struct Download {
    /// Set when the 3D viewer fetches the file, which is no download.
    preview: Option<String>,
    /// The direction, by name, to put glass at two opposite corners of
    /// the box `//rstack` stacks, for `WorldEdit` to select it by.
    markers: Option<String>,
    /// One of the files the in-game stacking procedure pastes, for the
    /// `direction` it stacks.
    part: Option<Part>,
    direction: Option<String>,
    /// Copies each way the build stacks.
    west: Option<usize>,
    up: Option<usize>,
    north: Option<usize>,
    #[serde(rename = "west-up")]
    west_up: Option<usize>,
}

/// A file the in-game stacking procedure pastes.
#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Part {
    /// The build without the parts only its first copy keeps.
    Without,
    /// Those parts alone.
    First,
}

impl Download {
    /// The copies asked for each way, by direction name, leaving out one
    /// copy, which is the build itself.
    fn copies(&self) -> HashMap<String, usize> {
        [
            ("west", self.west),
            ("up", self.up),
            ("north", self.north),
            ("west-up", self.west_up),
        ]
        .into_iter()
        .filter_map(|(name, copies)| Some((name.to_owned(), copies?)))
        .filter(|&(_, copies)| copies != 1)
        .collect()
    }
}

/// The stored `.schem` file, named by the naming scheme. The viewer fetches
/// the same URL.
pub async fn download(
    Path(id): Path<String>,
    State(state): State<AppState>,
    viewer: Option<User>,
    method: Method,
    Query(asked): Query<Download>,
) -> Result<Response, AppError> {
    // The 3D viewer's fetch, and a HEAD request, is no download.
    let preview = asked.preview.is_some();
    let counted = !preview && method == Method::GET;
    let palette = match &viewer {
        Some(user) => palette::of(&state.db, user.id).await?,
        None => None,
    };
    let (filename, data) =
        export(&state.db, &id, &asked, counted, palette.as_ref()).await?;
    // Only readable files are stored, so the viewer's copy falls back to
    // the file as it is only if that changes.
    let data = if preview {
        standardize::for_viewer(&data).unwrap_or(data)
    } else {
        data
    };
    Ok(super::attachment(&filename, data))
}

/// Build `id`'s file as `asked`, with its name, counted as a download if
/// `counted`, in `palette` if it is a recognized build's.
pub(super) async fn export(
    db: &SqlitePool,
    id: &str,
    asked: &Download,
    counted: bool,
    palette: Option<&Palette>,
) -> Result<(String, Vec<u8>), AppError> {
    let s = schematic::get(db, id).await?.ok_or(AppError::NotFound)?;
    let data = version::current_file(db, id)
        .await?
        .ok_or(AppError::NotFound)?;
    // Its blocks before any stacking, by position; its colours after, as
    // stacking finds buses by them.
    let palette = match palette {
        Some(palette) => palette::needs(db, id).await?.map(|n| (palette, n)),
        None => None,
    };
    let data = match &palette {
        Some((palette, needs)) => palette::blocks_in(&data, needs, palette)?,
        None => data,
    };
    let (filename, data) = shaped(db, id, s, data, asked, counted).await?;
    let data = match &palette {
        Some((palette, _)) => palette::colour(&data, palette)?,
        None => data,
    };
    Ok((filename, data))
}

/// Build `s`'s file `data` as `asked`: a part, copies stacked, or as it is,
/// with stacking markers if asked; counted as a download if `counted`.
async fn shaped(
    db: &SqlitePool,
    id: &str,
    s: Schematic,
    data: Vec<u8>,
    asked: &Download,
    counted: bool,
) -> Result<(String, Vec<u8>), AppError> {
    if let Some(part) = asked.part {
        let direction = s
            .stacking
            .iter()
            .find(|d| asked.direction.as_deref() == Some(d.name().as_str()))
            .ok_or_else(|| AppError::BadRequest("no such part".to_owned()))?;
        let first = part == Part::First;
        let data = stack::part(&data, *direction, first)?;
        let suffix = if first {
            "_first_copy_parts"
        } else {
            "_without_first_copy_parts"
        };
        let filename =
            s.filename().replace(".schem", &format!("{suffix}.schem"));
        return Ok((filename, data));
    }
    let copies = asked.copies();
    let markers = match asked.markers.as_deref() {
        None | Some("") => None,
        Some(name) => Some(
            s.stacking
                .iter()
                .copied()
                .find(|d| d.name() == name)
                .ok_or_else(|| {
                    AppError::BadRequest("no such direction".to_owned())
                })?,
        ),
    };
    if markers.is_some() && !copies.is_empty() {
        return Err(AppError::BadRequest(
            "Stacking markers go on a single copy.".to_owned(),
        ));
    }
    let (data, s) = if copies.is_empty() {
        (data, s)
    } else {
        let stacked = stack::plan(&s.stacking, &copies).and_then(|plan| {
            let buses = stack::Buses::of(&s.category, s.orientation);
            let (data, size) = stack::stack(&data, &plan, buses)?;
            Ok((data, s.stacked(&plan, size)))
        });
        stacked.map_err(|err| AppError::BadRequest(err.to_string()))?
    };
    if counted {
        schematic::count_download(db, id).await?;
    }
    let data = match markers {
        Some(direction) => {
            stack::with_markers(&data, direction)?.ok_or_else(|| {
                AppError::BadRequest(
                "Every pair of opposite corners of its box holds a block, so \
                 there's nowhere to put the glass."
                    .to_owned(),
            )
            })?
        }
        None => data,
    };
    Ok((s.filename(), data))
}
