//! The home page and the listing: what an address asks for, each category
//! pictured by its best build, the builds added last, and the builds a
//! search finds, ordered, with how many each category has.

use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
};

use axum::{
    extract::{RawQuery, State},
    response::{IntoResponse, Response},
};

use crate::{
    leaderboard,
    models::{
        catalogue::{self, Category},
        schematic::{self, Schematic},
        tag,
        user::User,
    },
    search,
    web::{app_state::AppState, error::AppError, extractors::fixi::Fixi},
};

use super::super::pages;

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
    /// Whether builds not verified as their category are listed too.
    unverified: bool,
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
                // Asking for the unverified by their tag shows them.
                "tag"
                    if [tag::FAILS_VERIFICATION, tag::NON_STANDARD]
                        .contains(&tag::name(&value).as_str()) =>
                {
                    query.unverified = true;
                    query.tags.push(value);
                }
                "tag" if !value.is_empty() => query.tags.push(value),
                "unverified" => query.unverified = value == "show",
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
        return super::super::similar::shown_page(
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
    let all = schematic::visible(&state.db, viewer).await?;
    let categories = catalogue::categories(&state.db).await?;
    let viewing = super::super::viewing(&state.db, viewer).await?;
    // A category's tile counts and pictures its verified builds.
    let verified: Vec<Schematic> =
        all.iter().filter(|s| s.verified()).cloned().collect();
    let index = pages::Index {
        tiles: &tiles(&categories, &verified),
        recent: &recent(&all),
        total: all.len(),
        viewing: &viewing,
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
    let all = schematic::visible(&state.db, viewer).await?;
    let categories = catalogue::categories(&state.db).await?;
    let total = all.len();
    let mut found: Vec<Schematic> = search::matching(&terms, all)
        .into_iter()
        .filter(|s| has_tags(s, &query.tags))
        .collect();
    // Builds not verified as their category list only when asked for; the
    // listing says how many there are either way.
    let unverified = found
        .iter()
        .filter(|s| {
            !s.verified()
                && (query.category.is_empty()
                    || in_category(s, &query.category))
        })
        .count();
    if !query.unverified {
        found.retain(Schematic::verified);
    }
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
    let viewing = super::super::viewing(&state.db, viewer).await?;
    let listing = pages::Listing {
        shown: &shown,
        total,
        view,
        sort: &query.sort,
        query: &query.q,
        terms: &terms,
        category: &query.category,
        tags: &query.tags,
        unverified: pages::Unverified {
            shown: query.unverified,
            count: unverified,
        },
        facets: &facets,
        viewing: &viewing,
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
/// value with its alternatives separated by commas, its design and flags
/// among its tags. A tag matches by name or token.
fn has_tags(s: &Schematic, groups: &[String]) -> bool {
    let tags = s.every_tag();
    groups.iter().all(|group| {
        group
            .split(',')
            .map(tag::name)
            .any(|wanted| tags.iter().any(|tag| tag.is_named(&wanted)))
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
