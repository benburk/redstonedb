use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use crate::{
    app_state::AppState,
    error::internal_error,
    leaderboard::{self, Wanted},
    models::{
        schematic::{self, Schematic},
        user::User,
    },
};

use super::pages;

/// A leaderboard's address: `?width=8&orientation=horz&design=carry_cancel`,
/// each optional.
#[derive(Deserialize)]
pub struct BoardQuery {
    width: Option<String>,
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
    let ranked = |name: &str| {
        all.iter()
            .filter(|s| s.category == name && s.width.is_some())
            .count()
    };
    // Only categories with a build to rank, and the one shown.
    let facets: Vec<pages::Facet> = categories
        .iter()
        .map(|c| pages::Facet {
            name: c.name.clone(),
            label: c.plural.clone(),
            count: ranked(&c.name),
            section: c.section.clone(),
        })
        .filter(|f| f.count > 0 || f.name == category)
        .collect();
    let given = |value: Option<String>| value.filter(|v| !v.is_empty());
    let wanted = Wanted {
        width: given(query.width).and_then(|w| w.parse().ok()),
        orientation: given(query.orientation),
        design: given(query.design),
    };
    let builds: Vec<&Schematic> =
        all.iter().filter(|s| s.category == category).collect();
    let board =
        leaderboard::board(&builds, &wanted, chosen.designs_interchangeable);
    pages::leaderboard(
        user_opt.as_ref(),
        &pages::LeaderboardPage {
            category: chosen,
            board: &board,
            facets: &facets,
        },
    )
    .into_response()
}
