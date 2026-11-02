use axum::{
    extract::{Path, Query, RawQuery, State},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;

use crate::{
    category::Category,
    leaderboard::{self, Wanted},
    models::{
        schematic::{self, Schematic},
        user::User,
    },
    web::{app_state::AppState, error::AppError},
};

use super::pages;

/// The category the menu of leaderboards chose, as in `?category=adder`.
#[derive(Deserialize)]
pub struct Chosen {
    category: Option<String>,
}

/// The gate categories that held a gate and its inverse together, each
/// with the gate its leaderboard's address now leads to, as XOR/XNOR
/// Gate's leads to XOR Gate's.
const SPLIT: [(&str, &str); 3] = [
    ("xor_xnor_gate", "xor_gate"),
    ("and_nand_gate", "and_gate"),
    ("or_nor_gate", "or_gate"),
];

/// A standard category's leaderboard, narrowed to one width and state, and
/// any orientation, design, latency, and tags.
pub async fn show(
    Path(category): Path<String>,
    RawQuery(raw): RawQuery,
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Result<Response, AppError> {
    if let Some((_, gate)) = SPLIT.iter().find(|(old, _)| *old == category) {
        let path = match &raw {
            Some(raw) => format!("/leaderboards/{gate}?{raw}"),
            None => format!("/leaderboards/{gate}"),
        };
        return Ok(Redirect::permanent(&path).into_response());
    }
    // Private builds compete with no one.
    let all = schematic::visible(&state.db, None).await?;
    let categories = Category::all();
    let Some(chosen) = categories.iter().find(|c| c.name == category) else {
        return Err(AppError::NotFound);
    };
    let facets = facets(&categories, &all, &category);
    let wanted = Wanted::parse(raw.as_deref().unwrap_or_default());
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
    let categories = Category::all();
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
