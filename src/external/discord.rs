//! Logging in with Discord: send the person to Discord with a random
//! `state`, then trade the code Discord sends back for their profile.

use anyhow::{Context, Result};
use serde::Deserialize;

/// Who Discord says a person is, from its `/users/@me`.
pub struct DiscordProfile {
    pub id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar: Option<String>,
}

const AUTHORIZE: &str = "https://discord.com/oauth2/authorize";
const TOKEN: &str = "https://discord.com/api/oauth2/token";
const ME: &str = "https://discord.com/api/users/@me";

/// The app's credentials, from `DISCORD_CLIENT_ID`,
/// `DISCORD_CLIENT_SECRET`, and `DISCORD_REDIRECT_URI`.
#[derive(Clone)]
pub struct Config {
    client_id: String,
    client_secret: String,
    redirect_uri: String,
}

impl Config {
    /// The settings, or `None` when any is missing, which turns login off.
    pub fn from_env() -> Option<Self> {
        let var = |name| dotenvy::var(name).ok().filter(|v| !v.is_empty());
        Some(Config {
            client_id: var("DISCORD_CLIENT_ID")?,
            client_secret: var("DISCORD_CLIENT_SECRET")?,
            redirect_uri: var("DISCORD_REDIRECT_URI")?,
        })
    }

    /// Where to send someone to approve the login. Only the `identify`
    /// scope is asked for: their ID, name, and avatar.
    pub fn authorize_url(&self, state: &str) -> String {
        let query = form_urlencoded::Serializer::new(String::new())
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.client_id)
            .append_pair("scope", "identify")
            .append_pair("redirect_uri", &self.redirect_uri)
            .append_pair("state", state)
            .finish();
        format!("{AUTHORIZE}?{query}")
    }

    /// The profile of whoever approved the login that produced `code`.
    pub async fn profile(
        &self,
        http: &reqwest::Client,
        code: &str,
    ) -> Result<DiscordProfile> {
        #[derive(Deserialize)]
        struct Token {
            access_token: String,
        }
        #[derive(Deserialize)]
        struct Me {
            id: String,
            username: String,
            global_name: Option<String>,
            avatar: Option<String>,
        }

        let body = form_urlencoded::Serializer::new(String::new())
            .append_pair("grant_type", "authorization_code")
            .append_pair("code", code)
            .append_pair("redirect_uri", &self.redirect_uri)
            .finish();
        let token: Token = http
            .post(TOKEN)
            .basic_auth(&self.client_id, Some(&self.client_secret))
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(body)
            .send()
            .await?
            .error_for_status()
            .context("Discord refused the login code")?
            .json()
            .await?;
        let me: Me = http
            .get(ME)
            .bearer_auth(&token.access_token)
            .send()
            .await?
            .error_for_status()
            .context("Discord would not say who logged in")?
            .json()
            .await?;
        Ok(DiscordProfile {
            id: me.id,
            username: me.username,
            display_name: me.global_name,
            avatar: me.avatar,
        })
    }
}
