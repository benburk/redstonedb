//! Accounts and their roles.

use anyhow::{Context, Result};
use app::models::user::{Role, User};
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
