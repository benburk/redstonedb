//! Minecraft players, looked up with Mojang. A player's name is the same in
//! any capitals, but Mojang knows the one spelling the player chose, and a
//! UUID that stays theirs through renames.

use std::time::Duration;

use serde::Deserialize;

const LOOKUP: &str =
    "https://api.minecraftservices.com/minecraft/profile/lookup/bulk/byname";

/// The most names Mojang takes in one lookup.
const BATCH: usize = 10;

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
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(failed)?;
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

/// `names` as their players spell them, or a sentence naming those no
/// player has, to be checked. When Mojang cannot be reached, the names as
/// given, so an outage does not stop anyone crediting a build.
pub async fn respell(names: &[String]) -> Result<Vec<String>, String> {
    let Ok(found) = profiles(names).await else {
        return Ok(names.to_vec());
    };
    let unknown = unknown(names, &found);
    match unknown.as_slice() {
        [] => Ok(spelled(names, &found)),
        [one] => Err(format!(
            "No Minecraft player is named {one}. Check the spelling."
        )),
        many => Err(format!(
            "No Minecraft players are named {}. Check the spelling.",
            many.join(", ")
        )),
    }
}

/// The names in `names` that none of `found` has.
fn unknown<'a>(names: &'a [String], found: &[Profile]) -> Vec<&'a str> {
    names
        .iter()
        .filter(|name| !found.iter().any(|p| p.name.eq_ignore_ascii_case(name)))
        .map(String::as_str)
        .collect()
}

/// `names` with each one `found` knows in its spelling.
fn spelled(names: &[String], found: &[Profile]) -> Vec<String> {
    names
        .iter()
        .map(|name| {
            found
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(name))
                .map_or_else(|| name.clone(), |p| p.name.clone())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_no_player_has_are_named() {
        let found = [Profile {
            uuid: "1218d0e691294ce09d98fded20551c1c".to_owned(),
            name: "_zebek".to_owned(),
        }];
        let names = ["_ZEBEK".to_owned(), "Zebek__".to_owned()];
        assert_eq!(unknown(&names, &found), ["Zebek__"]);
    }

    #[test]
    fn known_names_take_their_players_spelling() {
        let found = [Profile {
            uuid: "927c830fece6456182f9b2a3142b37b0".to_owned(),
            name: "Pishleback".to_owned(),
        }];
        let names = ["pishleback".to_owned(), "Someone_Renamed".to_owned()];
        assert_eq!(
            spelled(&names, &found),
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
