//! Minecraft players, looked up with Mojang. A player's name is the same in
//! any capitals, but Mojang knows the one spelling the player chose, and a
//! UUID that stays theirs through renames.

use std::{collections::HashMap, sync::LazyLock, time::Duration};

use serde::Deserialize;

const LOOKUP: &str =
    "https://api.minecraftservices.com/minecraft/profile/lookup/bulk/byname";

/// The most names Mojang takes in one lookup.
const BATCH: usize = 10;

/// The one client every lookup goes through, so each needn't connect
/// afresh. `None` if it could not be built.
static CLIENT: LazyLock<Option<reqwest::Client>> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .ok()
});

/// A player as Mojang knows them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Profile {
    #[serde(rename = "id")]
    pub uuid: String,
    pub name: String,
}

/// The players `names` name, whatever their capitals; a name no player has
/// is left out. `Err` says Mojang could not be asked.
pub async fn profiles(names: &[String]) -> Result<Vec<Profile>, String> {
    let failed = |_| "Could not reach Mojang to check the name.".to_owned();
    let client = CLIENT.as_ref().ok_or_else(|| {
        "Could not reach Mojang to check the name.".to_owned()
    })?;
    let mut found = Vec::new();
    for batch in names.chunks(BATCH) {
        let response = client
            .post(LOOKUP)
            .json(batch)
            .send()
            .await
            .map_err(failed)?;
        if !response.status().is_success() {
            return Err(format!(
                "Mojang answered {} when asked about the name.",
                response.status()
            ));
        }
        found.extend(response.json::<Vec<Profile>>().await.map_err(failed)?);
    }
    Ok(found)
}

/// The player named `name`, whatever its capitals, or `None` if no player
/// has it.
pub async fn profile(name: &str) -> Result<Option<Profile>, String> {
    Ok(profiles(&[name.to_owned()]).await?.into_iter().next())
}

/// What is known of each player asked about, by name in lowercase: the
/// player, or `None` for a name no player has. A name missing from it was
/// not asked about, as when Mojang cannot be reached.
pub type Players = HashMap<String, Option<Profile>>;

/// `names` as their players spell them, or a sentence naming those no
/// player has, to be checked, by what `players` knows of them. A name it
/// doesn't know stays as given, so an outage does not stop anyone
/// crediting a build.
pub fn respell(
    names: &[String],
    players: &Players,
) -> Result<Vec<String>, String> {
    let unknown = unknown(names, players);
    match unknown.as_slice() {
        [] => Ok(names
            .iter()
            .map(|name| match players.get(&name.to_lowercase()) {
                Some(Some(player)) => player.name.clone(),
                _ => name.clone(),
            })
            .collect()),
        [one] => Err(format!(
            "No Minecraft player is named {one}. Check the spelling."
        )),
        many => Err(format!(
            "No Minecraft players are named {}. Check the spelling.",
            many.join(", ")
        )),
    }
}

/// The names in `names` that `players` says no player has, as given.
pub fn unknown(names: &[String], players: &Players) -> Vec<String> {
    names
        .iter()
        .filter(|name| matches!(players.get(&name.to_lowercase()), Some(None)))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn players(known: &[(&str, Option<&str>)]) -> Players {
        known
            .iter()
            .map(|&(name, spelling)| {
                let player = spelling.map(|spelling| Profile {
                    uuid: "1218d0e691294ce09d98fded20551c1c".to_owned(),
                    name: spelling.to_owned(),
                });
                (name.to_owned(), player)
            })
            .collect()
    }

    /// A name no player has is named, as given; one not asked about, as
    /// when Mojang can't be reached, is not.
    #[test]
    fn names_no_player_has_are_named() {
        let known = players(&[("_zebek", Some("_zebek")), ("zebek__", None)]);
        let names = ["_ZEBEK".to_owned(), "Zebek__".to_owned(), "x".to_owned()];
        assert_eq!(unknown(&names, &known), ["Zebek__"]);
        assert_eq!(
            respell(&names, &known).unwrap_err(),
            "No Minecraft player is named Zebek__. Check the spelling."
        );
    }

    /// A name a player has takes their spelling; one not asked about stays
    /// as given.
    #[test]
    fn known_names_take_their_players_spelling() {
        let known = players(&[("pishleback", Some("Pishleback"))]);
        let names = ["pishleback".to_owned(), "Someone_Renamed".to_owned()];
        assert_eq!(
            respell(&names, &known).unwrap(),
            ["Pishleback".to_owned(), "Someone_Renamed".to_owned()]
        );
    }

    #[test]
    fn a_profile_reads_from_mojangs_answer() {
        let answer =
            r#"[{"id":"03b081848ce94e60be931fd4796c7e2d","name":"embizone"}]"#;
        let found: Vec<Profile> = serde_json::from_str(answer).unwrap();
        assert_eq!(found[0].name, "embizone");
        assert_eq!(found[0].uuid, "03b081848ce94e60be931fd4796c7e2d");
    }
}
