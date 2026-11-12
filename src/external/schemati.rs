//! Schemati, the schematic store of the Open Redstone Engineers server.
//! In game, `//download <name>` gives a link, such as
//! `http://schemati.openredstone.org/download?file=2H2C2q6GlAdNM7gr`, that
//! anyone can open for seven days without logging in.
//!
//! A player's own folder, the one `//save` and `//schem load` use, is open
//! to whoever holds their Schemati session: the `sessionId` cookie its
//! Discord login sets. Schemati has no API, only pages for people, so the
//! folder is read from its listing page, and a change is done by the
//! address its link or form would send, judged by where Schemati sends the
//! browser next: back to the listing once done, to its front page when it
//! no longer knows the session, or an error page saying what went wrong.

use std::time::Duration;

use reqwest::{Response, StatusCode, Url, header, redirect};

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
    let response = client()?
        .get(format!("https://{HOST}/download?file={key}"))
        .send()
        .await
        .map_err(offline)?;
    // An expired or mistyped link gets Schemati's error page, not a file.
    if !response.status().is_success() || is_page(&response) {
        return Err("Schemati has no download behind that link: links from \
                    //download last seven days, so run it again in game."
            .to_owned());
    }
    body(response).await
}

/// What a request to Schemati made with a player's session failed with.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Schemati no longer knows the session: it ran out, its player logged
    /// out there, or Schemati restarted.
    LoggedOut,
    /// Anything else, as a sentence for the person.
    Said(String),
}

impl From<String> for Refusal {
    fn from(sentence: String) -> Self {
        Refusal::Said(sentence)
    }
}

/// The session in `pasted`, the value of Schemati's `sessionId` cookie,
/// copied from the browser alone, as `sessionId=…`, or with the rest of
/// its row in the browser's cookie list; or a sentence saying it holds
/// none.
pub fn session(pasted: &str) -> Result<String, String> {
    pasted
        .split(|c: char| c.is_whitespace() || matches!(c, '=' | ';' | ':'))
        .filter(|word| !word.is_empty())
        .find(|word| !word.eq_ignore_ascii_case("sessionId"))
        .filter(|word| {
            (16..=64).contains(&word.len())
                && word.chars().all(|c| c.is_ascii_hexdigit())
        })
        .map(str::to_owned)
        .ok_or_else(|| {
            "That is not a Schemati session. Copy the value of the \
             sessionId cookie, 32 letters and digits."
                .to_owned()
        })
}

/// Whether Schemati takes `name` for a file: letters, digits, underscores,
/// and dots, ending `.schem` or `.schematic`.
pub fn valid_name(name: &str) -> bool {
    let stem = name
        .strip_suffix(".schem")
        .or_else(|| name.strip_suffix(".schematic"));
    stem.is_some_and(|stem| {
        !stem.is_empty()
            && stem
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.'))
    })
}

/// `name`, a file's name on someone's computer, as Schemati takes it:
/// each run of other characters an underscore, and `.schem` added when it
/// ends neither `.schem` nor `.schematic`.
pub fn file_name(name: &str) -> String {
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, extension @ ("schem" | "schematic"))) => (stem, extension),
        _ => (name, "schem"),
    };
    let mut kept = String::new();
    for c in stem.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '_' | '.') {
            kept.push(c);
        } else if !kept.ends_with('_') {
            kept.push('_');
        }
    }
    if kept.is_empty() {
        kept.push_str("build");
    }
    format!("{kept}.{extension}")
}

/// A player's Schemati folder.
#[derive(Debug, PartialEq, Eq)]
pub struct Folder {
    /// Their in-game name, as Schemati greets them.
    pub player: String,
    /// Their files: newest first when Schemati says when each was last
    /// saved, else by name.
    pub files: Vec<File>,
}

/// A file in a Schemati folder.
#[derive(Debug, PartialEq, Eq)]
pub struct File {
    pub name: String,
    /// When it was last saved, in milliseconds since 1970, as Schemati
    /// says.
    pub modified: Option<u64>,
}

impl Folder {
    /// Whether Schemati said when each file was saved, so the first is the
    /// newest.
    pub fn dated(&self) -> bool {
        !self.files.is_empty()
            && self.files.iter().all(|f| f.modified.is_some())
    }

    /// Whether it holds a file named `name`.
    pub fn has(&self, name: &str) -> bool {
        self.files.iter().any(|f| f.name == name)
    }
}

/// The folder of the player whose session `session` is.
pub async fn folder(session: &str) -> Result<Folder, Refusal> {
    let response = signed(session, "/schems", &[]).await?;
    if response.status().is_redirection() {
        return Err(Refusal::LoggedOut);
    }
    let page = response.text().await.map_err(offline)?;
    Ok(listing(&page))
}

/// The file `name` in the folder of `session`'s player.
pub async fn file(session: &str, name: &str) -> Result<Vec<u8>, Refusal> {
    let response =
        signed(session, "/schems/download", &[("file", name)]).await?;
    if response.status().is_success() && !is_page(&response) {
        return Ok(body(response).await?);
    }
    Err(outcome(response).await.err().unwrap_or_else(|| {
        Refusal::Said(format!("Schemati did not send {name}."))
    }))
}

/// Saves `data` as `name` in the folder of `session`'s player, in place of
/// any file of that name. Schemati refuses a file `WorldEdit` cannot read.
pub async fn put(
    session: &str,
    name: &str,
    data: Vec<u8>,
) -> Result<(), Refusal> {
    check(name)?;
    // A multipart form of the one file, as Schemati's upload button sends.
    let boundary = format!("rsdb{}", uuid::Uuid::new_v4().simple());
    let mut form = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"toUpload\"; \
         filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
    )
    .into_bytes();
    form.extend_from_slice(&data);
    form.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let response = client()?
        .post(format!("https://{HOST}/schems/upload"))
        .header(header::COOKIE, cookie(session))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(form)
        .send()
        .await
        .map_err(offline)?;
    outcome(response).await
}

/// Refuses a name Schemati would, before asking it.
fn check(name: &str) -> Result<(), Refusal> {
    if valid_name(name) {
        Ok(())
    } else {
        Err(Refusal::Said(format!(
            "Schemati only takes names of letters, digits, and underscores \
             ending .schem, not {name}."
        )))
    }
}

/// A client that reports redirects rather than following them, since where
/// Schemati sends the browser is how it answers.
fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .redirect(redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(offline)
}

/// Why Schemati gave no answer, for `map_err`, which hands the error over.
#[allow(clippy::needless_pass_by_value)]
fn offline(err: reqwest::Error) -> String {
    if err.is_timeout() {
        "Schemati took too long to answer; try again shortly.".to_owned()
    } else {
        "Could not reach Schemati; try again shortly.".to_owned()
    }
}

/// The cookie that carries `session`. It is hex, as `session` checks, so
/// it cannot break the header.
fn cookie(session: &str) -> String {
    format!("sessionId={session}")
}

/// A GET of `path` on Schemati with `query`, as `session`'s player.
async fn signed(
    session: &str,
    path: &str,
    query: &[(&str, &str)],
) -> Result<Response, String> {
    let url = Url::parse_with_params(&format!("https://{HOST}{path}"), query)
        .map_err(|_| {
        "That is not a file Schemati could have.".to_owned()
    })?;
    client()?
        .get(url)
        .header(header::COOKIE, cookie(session))
        .send()
        .await
        .map_err(offline)
}

/// What Schemati's answer to a change says: done when it sends the browser
/// on to the listing, logged out when to its front page, and otherwise the
/// error its page gives.
async fn outcome(response: Response) -> Result<(), Refusal> {
    let status = response.status();
    if status.is_redirection() {
        let location = response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        return if location.trim_end_matches('/').ends_with("/schems") {
            Ok(())
        } else {
            Err(Refusal::LoggedOut)
        };
    }
    if status != StatusCode::OK {
        return Err(Refusal::Said(format!(
            "Schemati answered {status}; try again shortly."
        )));
    }
    let page = response.text().await.map_err(offline)?;
    Err(Refusal::Said(match error_text(&page) {
        Some(error) => format!("Schemati says: {error}."),
        None => "Schemati refused, without saying why.".to_owned(),
    }))
}

/// Whether a response is a page rather than a file.
fn is_page(response: &Response) -> bool {
    response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|kind| kind.starts_with("text/html"))
}

/// A response's body, up to the most an upload may be.
async fn body(mut response: Response) -> Result<Vec<u8>, String> {
    let mut data = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(offline)? {
        if data.len() + chunk.len() > LIMIT {
            return Err("That schematic is over 2 MB, the most the site \
                        takes."
                .to_owned());
        }
        data.extend_from_slice(&chunk);
    }
    Ok(data)
}

/// The player and files on Schemati's listing page: the name greeted in
/// its first `<span>`, and each file in an element `id="fileName<n>"`,
/// with when it was last saved, in milliseconds, if the element says so
/// in a `data-modified` attribute.
fn listing(page: &str) -> Folder {
    let player = between(page, "<span>", "</span>")
        .map(unescape)
        .unwrap_or_default();
    let files: Vec<File> = page
        .split("id=\"fileName")
        .skip(1)
        .filter_map(|rest| {
            let (tag, after) = rest.split_once('>')?;
            let name = unescape(after.split_once('<')?.0);
            let modified = between(tag, "data-modified=\"", "\"")
                .and_then(|millis| millis.parse().ok());
            Some(File { name, modified })
        })
        .filter(|file| valid_name(&file.name))
        .collect();
    let mut folder = Folder { player, files };
    if folder.dated() {
        folder.files.sort_by(|a, b| {
            b.modified
                .cmp(&a.modified)
                .then_with(|| a.name.cmp(&b.name))
        });
    } else {
        folder.files.sort_by(|a, b| a.name.cmp(&b.name));
    }
    folder
}

/// The error on one of Schemati's error pages: its content's first
/// paragraph.
fn error_text(page: &str) -> Option<String> {
    let (_, content) = page.split_once("id=\"content\"")?;
    let error = unescape(between(content, "<p>", "</p>")?);
    let error = error.trim().trim_end_matches('.');
    (!error.is_empty()).then(|| error.to_owned())
}

/// The text in `text` after the first `start` and before the next `end`.
fn between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let (_, rest) = text.split_once(start)?;
    Some(rest.split_once(end)?.0)
}

/// `text` with the entities an HTML page escapes put back.
fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
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

    #[test]
    fn a_session_is_found_however_it_is_copied() {
        let id = "0123456789abcdef0123456789abcdef";
        for pasted in [
            id.to_owned(),
            format!(" {id}\n"),
            format!("sessionId={id}"),
            format!("sessionId={id}; sid=7be454bc"),
            format!("sessionId\t{id}\tschemati.openredstone.org\t/\t2026"),
        ] {
            assert_eq!(session(&pasted).as_deref(), Ok(id), "{pasted}");
        }
        for pasted in ["", "sessionId", "not a cookie", "abc\r\nX-Evil: 1"] {
            assert!(session(pasted).is_err(), "{pasted}");
        }
    }

    #[test]
    fn names_are_checked_and_made_as_schemati_takes_them() {
        assert!(valid_name("cci_8bit.schem"));
        assert!(valid_name("old.v2.schematic"));
        for name in [".schem", "adder.nbt", "my adder.schem", "../x.schem"] {
            assert!(!valid_name(name), "{name}");
        }
        assert_eq!(file_name("My Adder (v2).schem"), "My_Adder_v2_.schem");
        assert_eq!(file_name("old.schematic"), "old.schematic");
        assert_eq!(file_name("adder"), "adder.schem");
        assert_eq!(file_name("été.schem"), "_t_.schem");
        assert_eq!(file_name(".schem"), "build.schem");
        for name in ["My Adder (v2).schem", "adder", "été.schem", ".schem"] {
            assert!(valid_name(&file_name(name)), "{name}");
        }
    }

    #[test]
    fn the_listing_gives_the_player_and_files() {
        let page = r#"<div id="content">
            <p><span>embizone</span>'s schematics:</p>
            <ul id="fileListing">
              <li class="fileEntry">
                <div class="fileEntryName middleAlign" id="fileName0">stuffs.schematic</div>
                <div class="fileEntryActions middleAlign" id="actionOptions0"><a href="/schems/rename?file=stuffs.schematic">Rename</a></div>
              </li>
              <li class="fileEntry">
                <div class="fileEntryName middleAlign" id="fileName1">1.schem</div>
              </li>
            </ul>"#;
        assert_eq!(
            listing(page),
            Folder {
                player: "embizone".to_owned(),
                files: ["1.schem", "stuffs.schematic"]
                    .map(|name| File {
                        name: name.to_owned(),
                        modified: None,
                    })
                    .into(),
            }
        );
    }

    /// When Schemati says when each file was saved, the newest comes first.
    #[test]
    fn a_dated_listing_puts_the_newest_first() {
        let page = r#"<p><span>embizone</span>'s schematics:</p>
            <div class="fileEntryName middleAlign" id="fileName0" data-modified="100">a.schem</div>
            <div class="fileEntryName middleAlign" id="fileName1" data-modified="300">b.schem</div>
            <div class="fileEntryName middleAlign" id="fileName2" data-modified="200">c.schem</div>"#;
        let folder = listing(page);
        assert!(folder.dated());
        let names: Vec<&str> =
            folder.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["b.schem", "c.schem", "a.schem"]);
        assert_eq!(folder.files[0].modified, Some(300));
    }

    #[test]
    fn an_error_page_gives_its_error() {
        let page = r#"<h2>Schemati</h2><div id="content">
            <p>File does not exist</p>
            <p class="center"><a href="javascript:history.back()">Back</a></p>
            </div>"#;
        assert_eq!(error_text(page).as_deref(), Some("File does not exist"));
    }
}
