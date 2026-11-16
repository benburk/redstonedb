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
use sqlx::SqlitePool;

use crate::{
    account::User,
    build::{Build, tag},
    category::Category,
    leaderboard,
    screen::{self, Field},
    search,
    store::{builds, users},
    web::{
        app_state::AppState,
        browse::{
            home_view::{self, Index, Tile},
            list_view::{self, Listing},
            query::{HomeQuery, Sort, SortField},
            similar,
        },
        error::AppError,
        extractors::fixi::Fixi,
        ui::cards::Viewing,
    },
};

/// The file a search for similar builds was for, and what it found, when
/// the address `raw` carries them; see `similar::shown_page`.
fn similar_found(raw: &str) -> Option<(String, String)> {
    let given: Vec<(String, String)> = form_urlencoded::parse(raw.as_bytes())
        .into_owned()
        .collect();
    // The last of a repeated key counts.
    let value = |key: &str| {
        given
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, value)| value.trim().to_owned())
    };
    Some((value("file").unwrap_or_default(), value("found")?))
}

/// The home page: every category until something is searched for or
/// chosen, then the builds found.
pub async fn home(
    State(state): State<AppState>,
    RawQuery(raw): RawQuery,
    Fixi(fixi): Fixi,
    user_opt: Option<User>,
) -> Result<Response, AppError> {
    let raw = raw.unwrap_or_default();
    if let Some((file, found)) = similar_found(&raw) {
        return similar::shown_page(&state, user_opt.as_ref(), &file, &found)
            .await;
    }
    let query = HomeQuery::parse(&raw);
    if !query.narrows() {
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
    let query = HomeQuery::parse(raw.as_deref().unwrap_or_default());
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
    let all = builds::visible(&state.db, viewer).await?;
    let categories = Category::all();
    let viewing = viewing(&state.db, viewer).await?;
    // A category's tile counts its verified builds, unaligned too, as its
    // listing shows them, and pictures the first on its board, where none
    // unaligned rank.
    let verified: Vec<Build> =
        all.iter().filter(|s| s.verified()).cloned().collect();
    let index = Index {
        tiles: &tiles(&categories, &verified),
        recent: &recent(&all),
        total: all.len(),
        viewing: &viewing,
    };
    if fixi {
        return Ok(([("FX-Replace-Url", "/")], home_view::home_index(&index))
            .into_response());
    }
    Ok(home_view::home(viewer, &index).into_response())
}

/// Every category, most builds first and Other last, when it has any,
/// each pictured by the first on its leaderboard as it opens, the quickest
/// of its most common width, or by its newest build when none is timed; a
/// category with none yet is still shown, unpictured, so people see the
/// site takes it.
fn tiles<'a>(categories: &[Category], all: &'a [Build]) -> Vec<Tile<'a>> {
    let newest = |builds: &[&'a Build]| {
        builds
            .iter()
            .copied()
            .max_by(|a, b| a.entry.added_at.cmp(&b.entry.added_at))
    };
    let mut tiles: Vec<Tile> = categories
        .iter()
        .map(|category| {
            let builds: Vec<&Build> =
                all.iter().filter(|s| s.category == category.name).collect();
            let board = leaderboard::board(
                &builds,
                &leaderboard::Wanted::default(),
                &category.measures,
            );
            let leader = leaderboard::fastest(&board.builds, board.ruler)
                .first()
                .map(|&(_, s)| s);
            Tile {
                category: category.name.clone(),
                label: category.plural.clone(),
                count: builds.len(),
                best: leader.or_else(|| newest(&builds)),
                section: category.section,
            }
        })
        .collect();
    tiles.sort_by(|a, b| b.count.cmp(&a.count).then(a.label.cmp(&b.label)));
    let other: Vec<&Build> =
        all.iter().filter(|s| in_category(s, OTHER)).collect();
    if let Some(newest) = newest(&other) {
        tiles.push(Tile {
            category: OTHER.to_owned(),
            label: "Other".to_owned(),
            count: other.len(),
            best: Some(newest),
            section: None,
        });
    }
    tiles
}

/// The builds added last, newest first, one of each name so an import of
/// many alike takes one place, each with how many builds share its name.
fn recent(all: &[Build]) -> Vec<(&Build, usize)> {
    let mut named: HashMap<String, usize> = HashMap::new();
    for s in all {
        *named.entry(s.name()).or_default() += 1;
    }
    let mut newest: Vec<&Build> = all.iter().collect();
    newest.sort_by(|a, b| b.entry.added_at.cmp(&a.entry.added_at));
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
    query: HomeQuery,
    fixi: bool,
    viewer: Option<&User>,
) -> Result<Response, AppError> {
    let terms = search::parse(&query.q);
    let all = builds::visible(&state.db, viewer).await?;
    let total = all.len();
    // A build places among every build like it, whatever is searched for.
    let places = screen::places(&all);
    let found: Vec<Build> = search::matching(&terms, all)
        .into_iter()
        .filter(|s| has_tags(s, &query.tags))
        .collect();
    // Each field's choices count the builds the search and tags find that
    // the other fields take. Only working builds list until the rest are
    // asked for, and unaligned builds list as any working build does,
    // though they rank on no board.
    let rows = screen::rows(&found.iter().collect::<Vec<_>>(), &query.picks);
    let mut shown: Vec<Build> = found
        .into_iter()
        .filter(|s| query.picks.take(s, None))
        .collect();
    order(&mut shown, &query.order(), &places);
    let viewing = viewing(&state.db, viewer).await?;
    // A build its copies stack to the one width picked says so, as it
    // does to a width searched for.
    let mut noted = terms.clone();
    if let [width] = query.picks.of(Field::Width)[..]
        && width.parse::<i64>().is_ok()
    {
        noted.extend(search::parse(&format!("{width}b")));
    }
    let listing = Listing {
        shown: &shown,
        total,
        query: &query,
        terms: &terms,
        noted: &noted,
        title: title(&query),
        rows: &rows,
        viewing: &viewing,
    };
    // A search as it is typed redraws only the results, and puts the
    // search in the address, so Back and reloading keep it.
    if fixi {
        return Ok((
            [("FX-Replace-Url", query.href())],
            list_view::home_results(&listing),
        )
            .into_response());
    }
    Ok(list_view::schematics(viewer, &listing).into_response())
}

/// Whether `s` carries a tag from every group, a group being one `tag`
/// value with its alternatives separated by commas, its design and flags
/// among its tags. A tag matches by name or token.
fn has_tags(s: &Build, groups: &[String]) -> bool {
    let tags = s.every_tag();
    groups.iter().all(|group| {
        group
            .split(',')
            .map(tag::name)
            .any(|wanted| tags.iter().any(|tag| tag.is_named(&wanted)))
    })
}

/// Orders builds by `sort`. An unknown latency goes last either way. The
/// top go by their places, as `places` gives them, the best of each
/// category taking turns, and a build with no place last, those going
/// most favourited, then most downloaded, first. Ties, and an empty sort,
/// fall back to newest first.
fn order(
    shown: &mut [Build],
    sort: &Sort,
    places: &HashMap<String, screen::Place>,
) {
    let last = (usize::MAX, usize::MAX);
    let place = |s: &Build| places.get(&s.id).copied().unwrap_or(last);
    shown.sort_by(|a, b| {
        for &(field, descending) in &sort.0 {
            let ordering = match field {
                SortField::Top => place(a)
                    .cmp(&place(b))
                    .then(b.entry.favorites.cmp(&a.entry.favorites))
                    .then(b.entry.downloads.cmp(&a.entry.downloads)),
                SortField::AddedAt => a.entry.added_at.cmp(&b.entry.added_at),
                SortField::Volume => a.volume().cmp(&b.volume()),
                SortField::Downloads => {
                    a.entry.downloads.cmp(&b.entry.downloads)
                }
                SortField::Latency => {
                    match (a.specs.latency, b.specs.latency) {
                        (Some(x), Some(y)) => x.cmp(&y),
                        (None, Some(_)) => return Ordering::Greater,
                        (Some(_), None) => return Ordering::Less,
                        (None, None) => Ordering::Equal,
                    }
                }
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
        b.entry.added_at.cmp(&a.entry.added_at)
    });
}

/// The menu's name for every category off the standard list.
const OTHER: &str = "other";

/// Whether `s` belongs under the menu's `category`: its own category, or
/// `other` for any category off the standard list. Commas give several.
fn in_category(s: &Build, category: &str) -> bool {
    category.split(',').any(|category| {
        if category == OTHER {
            s.kind().is_none()
        } else {
            s.category == category
        }
    })
}

/// The listing's title: the one category picked, such as `Adders`, or
/// `Builds` for none or several.
fn title(query: &HomeQuery) -> String {
    match query.category() {
        Some(OTHER) => "Other".to_owned(),
        Some(name) => Category::all()
            .into_iter()
            .find(|category| category.name == name)
            .map_or_else(|| "Builds".to_owned(), |category| category.plural),
        None => "Builds".to_owned(),
    }
}

/// What `viewer`'s cards show of them: their palette, and whether they
/// are signed in, with their favourites.
pub(crate) async fn viewing(
    db: &SqlitePool,
    viewer: Option<&User>,
) -> anyhow::Result<Viewing> {
    Ok(Viewing {
        palette: users::palette_key(db, viewer).await?,
        signed_in: viewer.is_some(),
        favorites: match viewer {
            Some(viewer) => builds::favorites(db, viewer.id)
                .await?
                .into_iter()
                .collect(),
            None => HashSet::new(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interface::Orientation;

    fn build(category: &str, unaligned: bool) -> Build {
        Build {
            unaligned,
            ..crate::search::tests::schematic(
                category,
                Orientation::Horizontal,
                &[],
            )
        }
    }

    /// A category none but unaligned builds fill has its tile all the
    /// same, counting them.
    #[test]
    fn a_category_of_unaligned_builds_has_a_tile() {
        let builds = [build("greater_than", true), build("adder", false)];
        let tiles = tiles(&Category::all(), &builds);
        let tile = tiles.iter().find(|t| t.category == "greater_than");
        assert_eq!(tile.map(|t| t.count), Some(1));
    }
}
