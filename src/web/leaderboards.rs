use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;

use crate::{
    app_state::AppState,
    error::AppError,
    leaderboard::{self, Wanted},
    models::{
        catalogue::{self, Category, Measure},
        schematic::{self, Schematic},
        user::User,
    },
};

use super::pages;

/// A leaderboard's address: `?width=8&orientation=horz&design=carry_cancel&ticks=3&tags=synced`,
/// each optional, with a value of each of the category's measures, such as
/// `throughput` for a serial one, `at_least` for a threshold, and `pulse`
/// for a monostable.
#[derive(Deserialize)]
pub struct BoardQuery {
    width: Option<String>,
    throughput: Option<String>,
    bytes: Option<String>,
    at_least: Option<String>,
    pulse: Option<String>,
    logic: Option<String>,
    orientation: Option<String>,
    design: Option<String>,
    ticks: Option<String>,
    /// Tag names joined by commas, as `synced,torchless`.
    tags: Option<String>,
}

/// The category the menu of leaderboards chose, as in `?category=adder`.
#[derive(Deserialize)]
pub struct Chosen {
    category: Option<String>,
}

/// A standard category's leaderboard, narrowed to one width and state, and
/// any orientation, design, latency, and tags.
pub async fn show(
    Path(category): Path<String>,
    Query(query): Query<BoardQuery>,
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Result<Response, AppError> {
    // Private builds compete with no one.
    let all = schematic::visible(&state.db, None).await?;
    let categories = catalogue::categories(&state.db).await?;
    let Some(chosen) = categories.iter().find(|c| c.name == category) else {
        return Err(AppError::NotFound);
    };
    let facets = facets(&categories, &all, &category);
    let asked = [
        (Measure::Width, query.width),
        (Measure::Bytes, query.bytes),
        (Measure::Throughput, query.throughput),
        (Measure::AtLeast, query.at_least),
        (Measure::Pulse, query.pulse),
    ];
    let wanted = Wanted {
        measures: asked
            .into_iter()
            .filter_map(|(m, n)| Some((m, parsed(n)?)))
            .collect(),
        logic: parsed(query.logic),
        orientation: parsed(query.orientation),
        design: query.design.filter(|d| !d.is_empty()),
        ticks: parsed(query.ticks),
        tags: query
            .tags
            .iter()
            .flat_map(|tags| tags.split(','))
            .filter(|tag| !tag.is_empty())
            .map(str::to_owned)
            .collect(),
    };
    let own: Vec<&Schematic> =
        all.iter().filter(|s| s.category == category).collect();
    // Copies of a narrower build stacked to a board's width compete on it.
    let stacks = leaderboard::stacked(own.iter().copied());
    let builds: Vec<&Schematic> = own.into_iter().chain(&stacks).collect();
    let board = leaderboard::board(&builds, &wanted, &chosen.measures);
    Ok(pages::leaderboard(
        user_opt.as_ref(),
        &pages::LeaderboardPage {
            category: chosen,
            board: &board,
            facets: &facets,
        },
    )
    .into_response())
}

/// A value an address gives; one naming nothing is as good as none.
fn parsed<T: std::str::FromStr>(value: Option<String>) -> Option<T> {
    value?.parse().ok()
}

/// The leaderboards' front page: the records most contested, and who
/// built, added, and holds the records for the most builds.
pub async fn people(
    State(state): State<AppState>,
    Query(chosen): Query<Chosen>,
    user_opt: Option<User>,
) -> Result<Response, AppError> {
    // The menu of leaderboards sends its choice here, as a form does.
    if let Some(category) = chosen.category.filter(|c| !c.is_empty()) {
        let path = format!("/leaderboards/{category}");
        return Ok(Redirect::to(&path).into_response());
    }
    let all = schematic::visible(&state.db, None).await?;
    let categories = catalogue::categories(&state.db).await?;
    let stacks = leaderboard::stacked(&all);
    let builds: Vec<&Schematic> = all.iter().chain(&stacks).collect();
    let records = leaderboard::records(&builds);
    Ok(pages::people(
        user_opt.as_ref(),
        &pages::PeoplePage {
            records: &records,
            built: &leaderboard::most_built(&all),
            added: &leaderboard::most_added(&all),
            holders: &leaderboard::most_records(&records),
            facets: &facets(&categories, &all, ""),
        },
    )
    .into_response())
}

/// The menu of leaderboards: the categories with a build to rank, and
/// `current`, the one shown, whatever it has.
fn facets(
    categories: &[Category],
    all: &[Schematic],
    current: &str,
) -> Vec<pages::Facet> {
    // A build missing a measure of its category competes on no board.
    let ranked = |name: &str| {
        all.iter()
            .filter(|s| {
                s.category == name
                    && !s.measures.is_empty()
                    && s.measures.iter().all(|m| m.of(s).is_some())
            })
            .count()
    };
    categories
        .iter()
        .map(|c| pages::Facet {
            name: c.name.clone(),
            label: c.plural.clone(),
            count: ranked(&c.name),
            section: c.section,
        })
        .filter(|f| f.count > 0 || f.name == current)
        .collect()
}
