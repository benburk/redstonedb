use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use crate::{
    app_state::AppState,
    error::internal_error,
    leaderboard::{self, Measure, Wanted},
    models::{
        schematic::{self, Schematic},
        user::User,
    },
};

use super::pages;

/// A leaderboard's address: `?width=8&orientation=horz&design=carry_cancel`,
/// each optional, with `period` in place of `width` for a serial category.
#[derive(Deserialize)]
pub struct BoardQuery {
    width: Option<String>,
    period: Option<String>,
    bytes: Option<String>,
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
) -> Response {
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
    let Some(chosen) = categories.iter().find(|c| c.name == category) else {
        return super::not_found(user_opt.as_ref());
    };
    let facets = facets(&categories, &all, &category);
    let given = |value: Option<String>| value.filter(|v| !v.is_empty());
    let (by, measure) = if chosen.periodic {
        (Measure::Period, query.period)
    } else {
        (Measure::Width, query.width)
    };
    let wanted = Wanted {
        measure: given(measure).and_then(|n| n.parse().ok()),
        bytes: given(query.bytes).and_then(|n| n.parse().ok()),
        encoding: given(query.encoding),
        orientation: given(query.orientation),
        design: given(query.design),
    };
    let builds: Vec<&Schematic> =
        all.iter().filter(|s| s.category == category).collect();
    let board = leaderboard::board(
        &builds,
        &wanted,
        by,
        chosen.holds_bytes,
        chosen.designs_interchangeable,
    );
    pages::leaderboard(
        user_opt.as_ref(),
        &pages::LeaderboardPage {
            category: chosen,
            board: &board,
            facets: &facets,
            people: leaderboard::most_built(&all).len(),
        },
    )
    .into_response()
}

/// The people leaderboard: who built, added, and holds the records for the
/// most builds.
pub async fn people(
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Response {
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
    pages::people(
        user_opt.as_ref(),
        &pages::PeoplePage {
            built: &leaderboard::most_built(&all),
            added: &leaderboard::most_added(&all),
            records: &leaderboard::most_records(&all, splits(&categories)),
            facets: &facets(&categories, &all, ""),
        },
    )
    .into_response()
}

/// The leaderboard sidebar: the categories with a build to rank, and
/// `current`, the one shown, whatever it has.
fn facets(
    categories: &[schematic::Category],
    all: &[Schematic],
    current: &str,
) -> Vec<pages::Facet> {
    let ranked = |name: &str| {
        all.iter()
            .filter(|s| {
                s.category == name && (s.width.is_some() || s.period.is_some())
            })
            .count()
    };
    categories
        .iter()
        .map(|c| pages::Facet {
            name: c.name.clone(),
            label: c.plural.clone(),
            count: ranked(&c.name),
            section: c.section.clone(),
        })
        .filter(|f| f.count > 0 || f.name == current)
        .collect()
}

/// How each category's boards split: by width or period, and for a memory
/// by capacity too.
pub(super) fn splits(
    categories: &[schematic::Category],
) -> impl Fn(&str) -> (Measure, bool) + '_ {
    move |name| {
        let category = categories.iter().find(|c| c.name == name);
        let by = if category.is_some_and(|c| c.periodic) {
            Measure::Period
        } else {
            Measure::Width
        };
        (by, category.is_some_and(|c| c.holds_bytes))
    }
}
