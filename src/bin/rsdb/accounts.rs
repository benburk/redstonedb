//! Accounts and their roles, and the Minecraft names credits and accounts
//! hold, spelled as their players spell them.

use anyhow::{Context, Result};
use app::{
    external::mojang,
    models::user::{Role, User},
};
use sqlx::SqlitePool;

/// Prints every account's role and username.
pub(super) async fn list_users(db: &SqlitePool) -> Result<()> {
    for user in User::all(db).await? {
        println!("{:<6} {}", user.role.as_str(), user.username);
    }
    Ok(())
}

/// Sets the role of the account with a Discord username.
pub(super) async fn set_role(
    db: &SqlitePool,
    username: &str,
    role: &str,
) -> Result<()> {
    let user = User::get_by_username(db, username).await?.with_context(
        || {
            format!(
                "no account named {username}; they log in with Discord first"
            )
        },
    )?;
    let role = Role::parse(role).context("unknown role")?;
    User::set_role(db, user.id, role).await?;
    println!("{} is now {}", user.username, role.as_str());
    Ok(())
}

/// Asks Mojang how each stored Minecraft name is spelled and respells it
/// wherever it differs. A name Mojang does not know is left as it is.
pub(super) async fn respell(db: &SqlitePool) -> Result<()> {
    let names = User::player_names(db).await?;
    let found = mojang::profiles(&names).await.map_err(anyhow::Error::msg)?;
    for player in &found {
        let changed =
            User::respell_player(db, &player.name, &player.uuid).await?;
        if changed > 0 {
            println!("{}  {changed} respelled", player.name);
        }
    }
    println!("{} of {} names known to Mojang", found.len(), names.len());
    Ok(())
}
