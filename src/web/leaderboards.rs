use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
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

/// A leaderboard's address: `?width=8&orientation=horz&design=carry_cancel`,
/// each optional, with a value of each of the category's measures, such as
/// `throughput` for a serial one and `at_least` for a threshold.
#[derive(Deserialize)]
pub struct BoardQuery {
    width: Option<String>,
    throughput: Option<String>,
    bytes: Option<String>,
    at_least: Option<String>,
    logic: Option<String>,
    encoding: Option<String>,
    orientation: Option<String>,
    design: Option<String>,
}

/// A standard category's leaderboard, narrowed to one width, orientation,
/// and design.
pub async fn show(
    Path(category): Path<String>,
    Query(query): Query<BoardQuery>,
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Result<Response, AppError> {
    let all = schematic::all(&state.db).await?;
    let categories = catalogue::categories(&state.db).await?;
    let Some(chosen) = categories.iter().find(|c| c.name == category) else {
        return Err(AppError::NotFound);
    };
    let facets = facets(&categories, &all, &category);
    let given = |value: Option<String>| value.filter(|v| !v.is_empty());
    let asked = [
        (Measure::Width, query.width),
        (Measure::Bytes, query.bytes),
        (Measure::Throughput, query.throughput),
        (Measure::AtLeast, query.at_least),
    ];
    let wanted = Wanted {
        measures: asked
            .into_iter()
            .filter_map(|(m, n)| Some((m, given(n)?.parse().ok()?)))
            .collect(),
        logic: given(query.logic),
        encoding: given(query.encoding),
        orientation: given(query.orientation),
        design: given(query.design),
    };
    let builds: Vec<&Schematic> =
        all.iter().filter(|s| s.category == category).collect();
    let board = leaderboard::board(&builds, &wanted, &chosen.measures);
    Ok(pages::leaderboard(
        user_opt.as_ref(),
        &pages::LeaderboardPage {
            category: chosen,
            board: &board,
            facets: &facets,
            people: leaderboard::most_built(&all).len(),
        },
    )
    .into_response())
}

/// The people leaderboard: who built, added, and holds the records for the
/// most builds.
pub async fn people(
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Result<Response, AppError> {
    let all = schematic::all(&state.db).await?;
    let categories = catalogue::categories(&state.db).await?;
    Ok(pages::people(
        user_opt.as_ref(),
        &pages::PeoplePage {
            built: &leaderboard::most_built(&all),
            added: &leaderboard::most_added(&all),
            records: &leaderboard::most_records(&all),
            facets: &facets(&categories, &all, ""),
        },
    )
    .into_response())
}

/// The leaderboard sidebar: the categories with a build to rank, and
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
