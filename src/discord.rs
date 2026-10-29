//! Logging in with Discord: send the person to Discord with a random
//! `state`, then trade the code Discord sends back for their profile.

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::models::user::DiscordProfile;

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

    /// Where Discord sends the browser back to a page served at `host`:
    /// the configured address, or on a development build reached over the
    /// tailnet, as `tailscale serve` carries a dev server's port from
    /// another device, the same callback at that `*.ts.net` address. Discord
    /// takes only addresses registered with the app, so another host is
    /// still refused there.
    pub fn redirect_uri(&self, host: Option<&str>) -> String {
        match host {
            Some(host) if cfg!(debug_assertions) && tailnet(host) => {
                format!("https://{host}/auth/discord/callback")
            }
            _ => self.redirect_uri.clone(),
        }
    }

    /// Where to send someone to approve the login, Discord to send them
    /// back to `redirect_uri`. Only the `identify` scope is asked for:
    /// their ID, name, and avatar.
    pub fn authorize_url(&self, state: &str, redirect_uri: &str) -> String {
        let query = form_urlencoded::Serializer::new(String::new())
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.client_id)
            .append_pair("scope", "identify")
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("state", state)
            .finish();
        format!("{AUTHORIZE}?{query}")
    }

    /// The profile of whoever approved the login that produced `code`,
    /// sent back to `redirect_uri`, which Discord checks again here.
    pub async fn profile(
        &self,
        http: &reqwest::Client,
        code: &str,
        redirect_uri: &str,
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
            .append_pair("redirect_uri", redirect_uri)
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

/// Whether `host`, as a request names it, is a tailnet address such as
/// `crimson.example.ts.net:3001`: letters, digits, dots, and dashes, then
/// an optional port, so nothing else rides into the callback address.
fn tailnet(host: &str) -> bool {
    let name = host.rsplit_once(':').map_or(host, |(name, port)| {
        if port.chars().all(|c| c.is_ascii_digit()) {
            name
        } else {
            host
        }
    });
    name.ends_with(".ts.net")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::tailnet;

    #[test]
    fn only_a_plain_tailnet_host_counts() {
        assert!(tailnet("crimson.taile2df60.ts.net:3001"));
        assert!(tailnet("crimson.taile2df60.ts.net"));
        assert!(!tailnet("127.0.0.1:3001"));
        assert!(!tailnet("evil.com/x.ts.net"));
        assert!(!tailnet("a.ts.net@evil.com"));
        assert!(!tailnet("redstonedb.benburk.ca"));
    }
}
