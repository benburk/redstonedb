use axum::{
    extract::State,
    response::{IntoResponse, Response},
};

use crate::{
    app_state::AppState,
    category::{self, Lines},
    error::AppError,
    models::{
        catalogue::{self, Measure},
        simulation::simulated_flags,
        user::User,
    },
    simulate::Test,
};

use super::pages;

pub async fn about(user_opt: Option<User>) -> impl IntoResponse {
    pages::about(user_opt.as_ref())
}

/// What an upload needs and what the site does with it.
pub async fn before_upload(user_opt: Option<User>) -> impl IntoResponse {
    pages::before_upload(user_opt.as_ref())
}

/// The building conventions every build follows.
pub async fn conventions(user_opt: Option<User>) -> impl IntoResponse {
    pages::conventions(user_opt.as_ref())
}

/// How builds are sorted: categories, designs, measures, and the rest,
/// with the standard categories as they stand.
pub async fn categories(
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Result<Response, AppError> {
    let categories = catalogue::categories(&state.db).await?;
    Ok(pages::sorting(user_opt.as_ref(), &categories).into_response())
}

/// What the site works out for itself about each category's builds: which
/// it simulates, takes an upload for, reads from the file, and stacks.
pub async fn simulation(
    State(state): State<AppState>,
    user_opt: Option<User>,
) -> Result<Response, AppError> {
    let categories = catalogue::categories(&state.db).await?;
    let flags = catalogue::flags(&state.db).await?;
    let label = |category: &str, name: &str| {
        flags
            .iter()
            .find(|flag| flag.category == category && flag.name == name)
            .map_or_else(|| catalogue::words(name), |flag| flag.label.clone())
    };
    let mut checked = Vec::new();
    let mut unchecked = Vec::new();
    for c in &categories {
        let behaviour = category::behaviour(&c.name);
        let Some(test) = behaviour.test else {
            unchecked.push(c.plural.clone());
            continue;
        };
        let mut read = Vec::new();
        if behaviour.reads_file {
            read.extend(["width", "orientation", "encoding"].map(String::from));
        }
        let mut read_flags = Vec::new();
        if behaviour.flags_from_file
            && let Lines::Lone {
                control, status, ..
            } = behaviour.lines
        {
            for line in [control, status] {
                read_flags.push(label(&c.name, &line.replace(' ', "_")));
            }
        }
        let mut measured =
            vec!["latency".to_owned(), "synchronized".to_owned()];
        if c.has(Measure::Pulse) {
            measured.push("pulse".to_owned());
        }
        let measured_flags = simulated_flags(&c.name)
            .iter()
            .map(|flag| label(&c.name, flag))
            .collect();
        checked.push(pages::Checked {
            name: c.name.clone(),
            plural: c.plural.clone(),
            recognized: match test {
                Test::Alu => "When its signs name operations",
                _ if test.guessed() => "Yes",
                _ => "No",
            },
            read,
            read_flags,
            measured,
            measured_flags,
            stacking: behaviour.stacking,
        });
    }
    Ok(
        pages::simulation_guide(user_opt.as_ref(), &checked, &unchecked)
            .into_response(),
    )
}
