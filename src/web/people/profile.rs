use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use crate::{
    account::User,
    build::{Build, valid_player_name},
    external::mojang,
    store::builds,
    web::{
        self,
        app_state::AppState,
        error::AppError,
        people::users_view::{self, ProfileBuilds, ProfileList},
    },
};

/// Which of a profile's lists of builds to show, by its name; every one
/// of theirs when none is named.
#[derive(Deserialize)]
pub struct Shown {
    show: Option<String>,
}

/// An account's page: who they are, and their builds, filtered to those
/// they built, contributed to, or uploaded, or on one's own, those kept
/// private or favourited.
pub async fn profile(
    Path(username): Path<String>,
    State(state): State<AppState>,
    user_opt: Option<User>,
    Query(Shown { show }): Query<Shown>,
) -> Result<Response, AppError> {
    let account = User::get_by_username(&state.db, &username)
        .await?
        .ok_or(AppError::NotFound)?;
    show_profile(&state, user_opt.as_ref(), &account, show.as_deref()).await
}

/// The player `name` names, spelled as they spell it and with their UUID,
/// or a sentence saying it names none. When Mojang cannot be asked, the
/// name as typed, with no UUID.
pub(crate) async fn player_named(
    name: &str,
) -> Result<mojang::Profile, String> {
    if !valid_player_name(name) {
        return Err(format!(
            "{name} is not a Minecraft name: letters, digits, and \
             underscores, up to 16."
        ));
    }
    match mojang::profile(name).await {
        Ok(Some(found)) => Ok(found),
        Ok(None) => Err(format!(
            "No Minecraft player is named {name}. Check the spelling."
        )),
        Err(_) => Ok(mojang::Profile {
            uuid: String::new(),
            name: name.to_owned(),
        }),
    }
}

/// Stores `player` as account `id`'s Minecraft name, or clears it for
/// `None`. The inner `Err` says the name is another account's.
pub(crate) async fn saved(
    state: &AppState,
    id: uuid::Uuid,
    player: Option<&mojang::Profile>,
) -> Result<Result<(), String>, AppError> {
    let name = player.map(|p| p.name.as_str());
    let uuid = player.map(|p| p.uuid.as_str()).filter(|u| !u.is_empty());
    Ok(User::set_minecraft_name(&state.db, id, name, uuid).await?)
}

async fn show_profile(
    state: &AppState,
    viewer: Option<&User>,
    account: &User,
    show: Option<&str>,
) -> Result<Response, AppError> {
    let own = viewer.is_some_and(|viewer| viewer.id == account.id);
    let mut all = builds::visible(&state.db, viewer).await?;
    // Newest first, as on the home page.
    all.sort_by(|a, b| b.entry.added_at.cmp(&a.entry.added_at));
    // Credits are in-game names, so they match the profile's Minecraft
    // name, whatever its case.
    let credited = |names: &[String]| {
        account.minecraft_name.as_deref().is_some_and(|own| {
            names.iter().any(|name| name.eq_ignore_ascii_case(own))
        })
    };
    let pick = |keep: &dyn Fn(&Build) -> bool| -> Vec<_> {
        all.iter().filter(|s| keep(s)).cloned().collect()
    };
    let uploaded = |s: &Build| account.uploaded(s);
    let mut lists = vec![
        ProfileList {
            name: "all",
            label: "All",
            builds: pick(&|s| {
                credited(&s.credits.authors)
                    || credited(&s.credits.contributors)
                    || uploaded(s)
            }),
        },
        ProfileList {
            name: "built",
            label: "Built",
            builds: pick(&|s| credited(&s.credits.authors)),
        },
        ProfileList {
            name: "contributed",
            label: "Contributed to",
            builds: pick(&|s| credited(&s.credits.contributors)),
        },
        ProfileList {
            name: "uploaded",
            label: "Uploaded",
            builds: pick(&uploaded),
        },
    ];
    if own {
        lists.push(ProfileList {
            name: "private",
            label: "Private",
            builds: pick(&|s| uploaded(s) && !s.entry.public),
        });
        // Most lately favourited first.
        let ids = builds::favorites(&state.db, account.id).await?;
        let favourites = ids
            .iter()
            .filter_map(|id| all.iter().find(|s| &s.id == id).cloned())
            .collect();
        lists.push(ProfileList {
            name: "favourites",
            label: "Favourites",
            builds: favourites,
        });
    }
    let shown = show
        .and_then(|show| lists.iter().position(|list| list.name == show))
        .unwrap_or(0);
    let builds = ProfileBuilds {
        lists,
        shown,
        viewing: web::viewing(&state.db, viewer).await?,
    };
    Ok(users_view::profile(viewer, account, &builds).into_response())
}
