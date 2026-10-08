//! Fetching a build from Schemati, the schematic store of the Open Redstone
//! Engineers server. In game, `//download <name>` gives a link, such as
//! `http://schemati.openredstone.org/download?file=2H2C2q6GlAdNM7gr`, that
//! anyone can open for seven days without logging in.

use std::time::Duration;

use reqwest::{Url, header, redirect};

const HOST: &str = "schemati.openredstone.org";

/// The most a download may be, as for an uploaded file.
const LIMIT: usize = 2 * 1024 * 1024;

/// The key in a Schemati download link, such as `2H2C2q6GlAdNM7gr`, or a
/// sentence saying why the text is not one. The scheme may be left out.
pub fn download_key(link: &str) -> Result<String, String> {
    let link = link.trim();
    let with_scheme = if link.contains("://") {
        link.to_owned()
    } else {
        format!("https://{link}")
    };
    let not_a_link = || {
        format!(
            "That is not a Schemati download link. In game, //download <name> \
             gives one like https://{HOST}/download?file=2H2C2q6GlAdNM7gr."
        )
    };
    let url = Url::parse(&with_scheme).map_err(|_| not_a_link())?;
    if url.host_str() != Some(HOST) || url.path() != "/download" {
        return Err(not_a_link());
    }
    let key = url
        .query_pairs()
        .find(|(name, _)| name == "file")
        .map(|(_, key)| key.into_owned())
        .ok_or_else(not_a_link)?;
    let plain = !key.is_empty()
        && key.len() <= 64
        && key.chars().all(|c| c.is_ascii_alphanumeric());
    if plain { Ok(key) } else { Err(not_a_link()) }
}

/// The file behind download `key`, or a sentence saying why it could not be
/// fetched. Only Schemati's own address is fetched, rebuilt from the key, so
/// a pasted link can send the server nowhere else.
pub async fn fetch(key: &str) -> Result<Vec<u8>, String> {
    let failed = |_| "Could not reach Schemati; try again shortly.".to_owned();
    let client = reqwest::Client::builder()
        .redirect(redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(failed)?;
    let mut response = client
        .get(format!("https://{HOST}/download?file={key}"))
        .send()
        .await
        .map_err(failed)?;
    // An expired or mistyped link gets Schemati's error page, not a file.
    let page = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|kind| kind.starts_with("text/html"));
    if !response.status().is_success() || page {
        return Err("Schemati has no download behind that link: links from \
                    //download last seven days, so run it again in game."
            .to_owned());
    }
    let too_large =
        || "That schematic is over 2 MB, the most the site takes.".to_owned();
    let mut data = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(failed)? {
        if data.len() + chunk.len() > LIMIT {
            return Err(too_large());
        }
        data.extend_from_slice(&chunk);
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_download_link_gives_its_key() {
        for link in [
            "http://schemati.openredstone.org/download?file=2H2C2q6GlAdNM7gr",
            "https://schemati.openredstone.org/download?file=2H2C2q6GlAdNM7gr",
            " schemati.openredstone.org/download?file=2H2C2q6GlAdNM7gr ",
        ] {
            assert_eq!(download_key(link).as_deref(), Ok("2H2C2q6GlAdNM7gr"));
        }
    }

    #[test]
    fn other_links_are_refused() {
        for link in [
            "https://example.com/download?file=2H2C2q6GlAdNM7gr",
            "https://schemati.openredstone.org.example.com/download?file=abc",
            "https://schemati.openredstone.org/schems/download?file=abc",
            "https://schemati.openredstone.org/download",
            "https://schemati.openredstone.org/download?file=../etc",
            "https://schemati.openredstone.org/download?file=a%26b",
            "not a link",
        ] {
            assert!(download_key(link).is_err(), "{link}");
        }
    }
}
