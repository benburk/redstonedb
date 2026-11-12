//! Where a build's page goes back to: the listing the viewer found it in,
//! a search, a category, a leaderboard, or a player's builds, which its
//! breadcrumbs link back to and deleting it returns to.

use axum::http::{HeaderMap, header};

/// The listing `from` names, a path on this site such as `/?q=adder`, if
/// it is one a build's page may go back to: the home page or a search,
/// the recent builds, a leaderboard, a player's or account's builds, or
/// the moderation log. `None` for anywhere else, or anywhere off the site.
pub fn listing(from: &str) -> Option<&str> {
    // A path of this site's alone, never one a browser reads as another
    // host's, as `//host` or `/\host` are.
    if !from.starts_with('/') || from.starts_with("//") || from.contains('\\') {
        return None;
    }
    let path = from.split(['?', '#']).next().unwrap_or_default();
    let listed = path == "/"
        || path == "/builds"
        || path == "/log"
        || path == "/leaderboards"
        || ["/leaderboards/", "/players/", "/users/"]
            .iter()
            .any(|prefix| path.starts_with(prefix));
    listed.then_some(from)
}

/// Where a build's page was come to from: `given`, a page's own `from`,
/// as its colour switch carries it, else the page that linked to it, the
/// request's referrer, when either is a listing of this site's, as
/// [`listing`] reads it.
pub fn came_from(headers: &HeaderMap, given: Option<&str>) -> Option<String> {
    if let Some(from) = given.and_then(listing) {
        return Some(from.to_owned());
    }
    let referrer = headers.get(header::REFERER)?.to_str().ok()?;
    let host = headers.get(header::HOST)?.to_str().ok()?;
    let (_, rest) = referrer.split_once("://")?;
    let path = rest.strip_prefix(host)?;
    let path = if path.is_empty() { "/" } else { path };
    listing(path).map(str::to_owned)
}

/// What a link back to `from`, a listing, says it goes back to: a search
/// by what was searched for.
pub fn label(from: &str) -> String {
    let (path, query) = from.split_once('?').unwrap_or((from, ""));
    let searched = form_urlencoded::parse(query.as_bytes())
        .find(|(key, value)| key == "q" && !value.trim().is_empty())
        .map(|(_, value)| value.trim().to_owned());
    match searched {
        Some(q) if path == "/" => format!("Back to “{q}”"),
        _ if path == "/builds" => "Back to recent builds".to_owned(),
        _ if path.starts_with("/leaderboards") => {
            "Back to the leaderboard".to_owned()
        }
        _ if path.starts_with("/players/") || path.starts_with("/users/") => {
            "Back to their builds".to_owned()
        }
        _ if path == "/log" => "Back to the log".to_owned(),
        _ => "Back to results".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    /// A listing of this site's is gone back to; any other page, or a path
    /// a browser would read as another site's, is not.
    #[test]
    fn only_a_listing_of_this_sites_is_gone_back_to() {
        for from in [
            "/",
            "/?q=8b+adder",
            "/?category=adder&sort=-latency",
            "/builds",
            "/leaderboards/adder?width=8",
            "/players/embizone",
        ] {
            assert_eq!(listing(from), Some(from), "{from}");
        }
        for from in [
            "https://example.com/",
            "//example.com/",
            "/\\example.com",
            "/schematics/abc/edit",
            "/upload",
            "/settings",
        ] {
            assert_eq!(listing(from), None, "{from}");
        }
    }

    /// A link back names a search by what was searched for, and any other
    /// listing by what it lists.
    #[test]
    fn a_link_back_names_what_it_goes_back_to() {
        assert_eq!(label("/?q=8b+adder&sort=-latency"), "Back to “8b adder”");
        assert_eq!(label("/?category=adder&tag=torchless"), "Back to results");
        assert_eq!(label("/builds"), "Back to recent builds");
        assert_eq!(
            label("/leaderboards/adder?width=8"),
            "Back to the leaderboard"
        );
    }

    /// A page's own `from` comes first, else the referrer, only when it is
    /// this site's: the host it was asked of.
    #[test]
    fn a_page_comes_from_its_own_from_else_its_referrer() {
        let mut headers = HeaderMap::new();
        headers
            .insert(header::HOST, HeaderValue::from_static("redstonedb.com"));
        headers.insert(
            header::REFERER,
            HeaderValue::from_static("https://redstonedb.com/?q=adder"),
        );
        assert_eq!(came_from(&headers, None).as_deref(), Some("/?q=adder"));
        assert_eq!(
            came_from(&headers, Some("/builds")).as_deref(),
            Some("/builds")
        );
        headers.insert(
            header::REFERER,
            HeaderValue::from_static("https://example.com/?q=adder"),
        );
        assert_eq!(came_from(&headers, None), None);
        headers.insert(
            header::REFERER,
            HeaderValue::from_static("https://redstonedb.com"),
        );
        assert_eq!(came_from(&headers, None).as_deref(), Some("/"));
    }
}
