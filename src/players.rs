//! Who the Minecraft players behind a list of names are: what the site
//! keeps of each while it is fresh, and Mojang's answer for the rest.

use sqlx::SqlitePool;

use crate::{
    external::mojang::{self, Players},
    store::players::{known, remember},
};

/// What is known of each player `names` names, by name in lowercase: from
/// what is kept while fresh, else from Mojang, in one lookup for all the
/// rest, kept in turn, a name it doesn't return being no player's. Those
/// Mojang can't be asked about are left out; see [`Players`].
pub async fn look_up(
    db: &SqlitePool,
    names: &[String],
) -> Result<Players, sqlx::Error> {
    let mut players = Players::new();
    let mut ask: Vec<String> = Vec::new();
    for name in names {
        let name = name.to_lowercase();
        if players.contains_key(&name) || ask.contains(&name) {
            continue;
        }
        match known(db, &name).await? {
            Some((player, true)) => {
                players.insert(name, player);
            }
            _ => ask.push(name),
        }
    }
    if ask.is_empty() {
        return Ok(players);
    }
    let Ok(found) = mojang::profiles(&ask).await else {
        return Ok(players);
    };
    for name in ask {
        let player = found
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(&name))
            .cloned();
        remember(db, &name, player.as_ref()).await?;
        players.insert(name, player);
    }
    Ok(players)
}
