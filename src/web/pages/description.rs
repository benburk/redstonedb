//! A build's description as its writer marked it up, the way Discord reads
//! it, where most were first written: `**bold**`, `*italic*` or `_italic_`,
//! `__underline__`, `~~strikethrough~~`, `` `code` ``, `> ` quoted lines,
//! and links as `[words](url)`, `<url>`, or a bare address; and `@name`
//! names a player by their in-game name, shown as a badge linking to their
//! page. Nothing else is markup: no headings, lists, tables, or HTML, which
//! shows as typed.

use std::fmt::Write as _;

use maud::{Markup, PreEscaped};

/// `text` as HTML: blank lines part paragraphs, lines within one break,
/// and a run of `> ` lines is a quote.
pub fn render(text: &str) -> Markup {
    let text = text.replace("\r\n", "\n");
    let mut html = String::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim().is_empty() {
            continue;
        }
        let quoted = quote(line).is_some();
        let mut block = vec![quote(line).unwrap_or(line)];
        while let Some(&next) = lines.peek() {
            if next.trim().is_empty() || quote(next).is_some() != quoted {
                break;
            }
            block.push(quote(next).unwrap_or(next));
            lines.next();
        }
        let body: Vec<String> = block.iter().map(|line| inline(line)).collect();
        let body = body.join("<br>");
        if quoted {
            let _ = write!(html, "<blockquote><p>{body}</p></blockquote>");
        } else {
            let _ = write!(html, "<p>{body}</p>");
        }
    }
    PreEscaped(html)
}

/// The words of a quoted line, after its `>`.
fn quote(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('>')?;
    Some(rest.strip_prefix(' ').unwrap_or(rest))
}

/// The paired marks, longest first so `**` is not read as two `*`, with
/// the element each makes.
const PAIRS: [(&str, &str); 5] = [
    ("**", "strong"),
    ("__", "u"),
    ("~~", "s"),
    ("*", "em"),
    ("_", "em"),
];

/// One line's inline markup as HTML, everything else escaped.
fn inline(text: &str) -> String {
    let mut html = String::new();
    let mut rest = text;
    'scan: while let Some(c) = rest.chars().next() {
        if c == '`'
            && let Some(end) = rest[1..].find('`')
            && end > 0
        {
            let _ = write!(html, "<code>{}</code>", escape(&rest[1..=end]));
            rest = &rest[end + 2..];
            continue;
        }
        if let Some((link, used)) = link(rest) {
            html.push_str(&link);
            rest = &rest[used..];
            continue;
        }
        let prior = html.chars().last();
        if let Some(name) = mention(rest, prior) {
            html.push_str(&player_badge(name));
            rest = &rest[name.len() + 1..];
            continue;
        }
        for (mark, element) in PAIRS {
            if let Some(inner) = paired(rest, mark, prior) {
                let _ =
                    write!(html, "<{element}>{}</{element}>", inline(inner));
                rest = &rest[inner.len() + 2 * mark.len()..];
                continue 'scan;
            }
        }
        html.push_str(&escape(&rest[..c.len_utf8()]));
        rest = &rest[c.len_utf8()..];
    }
    html
}

/// What `mark` encloses at the start of `text`, when it closes again
/// around words: `**bold**` but not `** x`, and `_` only at a word's
/// edges, as `snake_case` is no emphasis. `prior` is the character
/// before.
fn paired<'t>(
    text: &'t str,
    mark: &str,
    prior: Option<char>,
) -> Option<&'t str> {
    let body = text.strip_prefix(mark)?;
    if body.starts_with(char::is_whitespace) || body.starts_with(mark) {
        return None;
    }
    let word = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
    if mark == "_" && word(prior) {
        return None;
    }
    let mut from = 0;
    while let Some(at) = body[from..].find(mark) {
        let end = from + at;
        let inner = &body[..end];
        let after = body[end + mark.len()..].chars().next();
        // `**` inside `*…*` is bold, not the end.
        let opens_bold = mark == "*" && after == Some('*');
        let mid_word = mark == "_" && word(after);
        let closes = end > 0
            && !inner.ends_with(char::is_whitespace)
            && !mid_word
            && !opens_bold;
        if closes {
            return Some(inner);
        }
        from = end + mark.len();
    }
    None
}

/// A link at the start of `text`, as HTML, with how much of `text` it
/// took: `[words](url)`, `<url>`, or a bare `http(s)://` address, its
/// trailing punctuation left out.
fn link(text: &str) -> Option<(String, usize)> {
    if let Some(rest) = text.strip_prefix('[')
        && let Some(close) = rest.find("](")
        && let Some(end) = rest[close + 2..].find(')')
    {
        let words = &rest[..close];
        let url = &rest[close + 2..close + 2 + end];
        if web(url) && !words.is_empty() {
            return Some((anchor(url, &inline(words)), close + end + 4));
        }
    }
    if let Some(rest) = text.strip_prefix('<')
        && let Some(end) = rest.find('>')
        && web(&rest[..end])
    {
        let url = &rest[..end];
        return Some((anchor(url, &escape(url)), end + 2));
    }
    let end = text.find(char::is_whitespace).unwrap_or(text.len());
    let url = text[..end]
        .trim_end_matches(['.', ',', ';', ':', '!', '?', ')', '\'', '"']);
    if web(url) {
        return Some((anchor(url, &escape(url)), url.len()));
    }
    None
}

/// The player `@name` names at the start of `text`, `prior` the character
/// before: an in-game name of 3 to 16 letters, digits, and underscores,
/// after no word, as an email address's `@` is not.
fn mention(text: &str, prior: Option<char>) -> Option<&str> {
    let rest = text.strip_prefix('@')?;
    if prior.is_some_and(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    let name = &rest[..end];
    (3..=16).contains(&name.len()).then_some(name)
}

/// Every player `text` names with `@name`, as written, each once whatever
/// its capitals.
pub fn mentions(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for line in text.lines() {
        let mut prior = None;
        for (at, c) in line.char_indices() {
            if let Some(name) = mention(&line[at..], prior)
                && !found.iter().any(|n| n.eq_ignore_ascii_case(name))
            {
                found.push(name.to_owned());
            }
            prior = Some(c);
        }
    }
    found
}

/// A player as a badge in running text: their head, blank until it loads,
/// and their name, linking to their page.
fn player_badge(name: &str) -> String {
    format!(
        r#"<a href="/players/{name}" class="mention"><img src="/players/{name}/head.png" alt="" width="16" height="16" loading="lazy">{name}</a>"#
    )
}

/// Whether `url` starts as a web address and has more after.
fn web(url: &str) -> bool {
    ["https://", "http://"]
        .iter()
        .any(|scheme| url.len() > scheme.len() && url.starts_with(scheme))
        && !url.contains(char::is_whitespace)
}

/// A link to `url` showing `words`, already HTML.
fn anchor(url: &str, words: &str) -> String {
    format!(
        r#"<a href="{}" rel="nofollow ugc">{words}</a>"#,
        escape(url)
    )
}

fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(c),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn html(text: &str) -> String {
        render(text).into_string()
    }

    #[test]
    fn inline_marks_become_their_elements() {
        assert_eq!(
            html("**bold** *it* _it_ __under__ ~~gone~~ `x < y`"),
            "<p><strong>bold</strong> <em>it</em> <em>it</em> <u>under</u> \
             <s>gone</s> <code>x &lt; y</code></p>"
        );
    }

    #[test]
    fn marks_that_dont_close_or_sit_inside_words_stay_as_typed() {
        assert_eq!(html("snake_case_name"), "<p>snake_case_name</p>");
        assert_eq!(html("2 * 3 * 4"), "<p>2 * 3 * 4</p>");
        assert_eq!(html("**open"), "<p>**open</p>");
        assert_eq!(html("14->1"), "<p>14-&gt;1</p>");
    }

    #[test]
    fn links_are_taken_three_ways_and_only_to_the_web() {
        assert_eq!(
            html("see <https://x.org/a?b=1&c=2>."),
            "<p>see <a href=\"https://x.org/a?b=1&amp;c=2\" rel=\"nofollow ugc\">\
             https://x.org/a?b=1&amp;c=2</a>.</p>"
        );
        assert_eq!(
            html("at https://youtu.be/abc, then"),
            "<p>at <a href=\"https://youtu.be/abc\" rel=\"nofollow ugc\">\
             https://youtu.be/abc</a>, then</p>"
        );
        assert_eq!(
            html("[**video**](https://y.com/v)"),
            "<p><a href=\"https://y.com/v\" rel=\"nofollow ugc\">\
             <strong>video</strong></a></p>"
        );
        assert_eq!(
            html("[x](javascript:alert(1))"),
            "<p>[x](javascript:alert(1))</p>"
        );
    }

    #[test]
    fn lines_break_paragraphs_part_and_quotes_gather() {
        assert_eq!(
            html("one\r\ntwo\r\n\r\nthree"),
            "<p>one<br>two</p><p>three</p>"
        );
        assert_eq!(
            html("> said\n> more\nreply"),
            "<blockquote><p>said<br>more</p></blockquote><p>reply</p>"
        );
    }

    #[test]
    fn a_player_named_with_an_at_is_a_badge_linking_to_their_page() {
        assert_eq!(
            html("by @mop_u's idea, thanks @Don_manuel_1229."),
            "<p>by <a href=\"/players/mop_u\" class=\"mention\">\
             <img src=\"/players/mop_u/head.png\" alt=\"\" width=\"16\" \
             height=\"16\" loading=\"lazy\">mop_u</a>&#39;s idea, thanks \
             <a href=\"/players/Don_manuel_1229\" class=\"mention\">\
             <img src=\"/players/Don_manuel_1229/head.png\" alt=\"\" \
             width=\"16\" height=\"16\" loading=\"lazy\">Don_manuel_1229\
             </a>.</p>"
        );
        // An email address, a name too short or long, and code are not.
        for typed in ["a@example.com", "@ab", "@abcdefghijklmnopq", "`@mop_u`"]
        {
            assert!(!html(typed).contains("mention"), "{typed}");
        }
        assert_eq!(
            mentions("@mop_u and @Mop_U\n> @Aminotreal, a@b.com"),
            ["mop_u", "Aminotreal"]
        );
    }

    #[test]
    fn html_shows_as_typed() {
        assert_eq!(
            html("<b>hi</b> \"q\""),
            "<p>&lt;b&gt;hi&lt;/b&gt; &quot;q&quot;</p>"
        );
    }
}
