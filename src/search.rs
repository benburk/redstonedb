//! Narrowing the schematic list with a query such as
//! `register vertical alpha -torchless`.
//!
//! Every term must match, so terms AND together. A term prefixed with `-`
//! or written after `not` must not match. A term matches a schematic when
//! it names one of its facts (function, orientation, encoding, a tag, its
//! width such as `8b`, its author or importer, `stackable`, `sync` or
//! `unsync`). A term that is no fact of any schematic matches when it
//! appears in a schematic's title, filename, or description instead, so `add` finds
//! adders while `sync` does not find `unsync`. Quotes keep a multi-word term together, as in
//! `"carry lookahead"`; underscores work too.

use std::collections::HashSet;

use crate::models::schematic::Schematic;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    /// Lowercase, with spaces for underscores.
    pub text: String,
    pub negated: bool,
}

impl Term {
    /// The term as typed in a query, so it can be removed or shown.
    pub fn display(&self) -> String {
        let text = if self.text.contains(' ') {
            format!("\"{}\"", self.text)
        } else {
            self.text.clone()
        };
        if self.negated {
            format!("-{text}")
        } else {
            text
        }
    }
}

pub fn parse(query: &str) -> Vec<Term> {
    let mut terms = Vec::new();
    let mut negate_next = false;
    for word in words(query) {
        if word.eq_ignore_ascii_case("not") && !negate_next {
            negate_next = true;
            continue;
        }
        let (negated, text) = match word.strip_prefix('-') {
            Some(rest) if !rest.is_empty() => (true, rest),
            _ => (negate_next, word.as_str()),
        };
        negate_next = false;
        let text = text.to_lowercase().replace('_', " ");
        let text = text.trim().to_owned();
        if !text.is_empty() {
            terms.push(Term { text, negated });
        }
    }
    terms
}

/// Splits on spaces, keeping quoted runs together and dropping the quotes;
/// a `-` before a quote stays with the quoted run.
fn words(query: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in query.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// The schematics every term matches, in their order.
pub fn matching(terms: &[Term], schematics: Vec<Schematic>) -> Vec<Schematic> {
    let known: HashSet<String> = schematics.iter().flat_map(facts).collect();
    schematics
        .into_iter()
        .filter(|s| {
            let facts = facts(s);
            let text = format!(
                "{} {} {}",
                s.title(),
                s.filename(),
                s.description.as_deref().unwrap_or("")
            )
            .to_lowercase();
            terms.iter().all(|term| {
                let found = if known.contains(&term.text) {
                    facts.contains(&term.text)
                } else {
                    text.contains(&term.text)
                };
                found != term.negated
            })
        })
        .collect()
}

/// Every word a term can name a schematic by.
fn facts(s: &Schematic) -> Vec<String> {
    let mut facts = vec![
        s.function.replace('_', " "),
        s.orientation.clone(),
        s.orientation_label().to_owned(),
        s.encoding.clone(),
        s.encoding_label().to_owned(),
    ];
    for tag in &s.tags {
        facts.push(tag.label());
        facts.push(tag.token().replace('_', " "));
    }
    if let Some(w) = s.width {
        facts.extend([
            format!("{w}b"),
            format!("{w:02}b"),
            format!("{w}-bit"),
            format!("{w} bit"),
        ]);
    }
    facts.extend(s.author.clone());
    facts.extend(s.imported_by.clone());
    if !s.stacking.is_empty() {
        facts.push("stackable".to_owned());
    }
    if !s.conventional {
        facts.push("unconventional".to_owned());
    }
    match s.synchronized {
        Some(true) => {
            facts.extend(["sync".to_owned(), "synchronized".to_owned()]);
        }
        Some(false) => {
            facts.extend(["unsync".to_owned(), "unsynchronized".to_owned()]);
        }
        None => {}
    }
    facts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::schematic::Tag;

    fn schematic(
        function: &str,
        orientation: &str,
        tags: &[&str],
    ) -> Schematic {
        Schematic {
            id: String::new(),
            function: function.to_owned(),
            width: Some(8),
            orientation: orientation.to_owned(),
            encoding: "bin".to_owned(),
            latency: None,
            synchronized: None,
            size: (1, 1, 1),
            stacking: Vec::new(),
            author: Some("embizone".to_owned()),
            built_on: None,
            imported_by: None,
            conventional: true,
            description: None,
            sha256: String::new(),
            added_at: String::new(),
            tags: tags
                .iter()
                .map(|name| Tag {
                    name: (*name).to_owned(),
                    token: None,
                    category: "other".to_owned(),
                })
                .collect(),
        }
    }

    fn find(query: &str, s: &Schematic) -> bool {
        !matching(&parse(query), vec![s.clone()]).is_empty()
    }

    #[test]
    fn terms_and_together() {
        let register = schematic("register", "vert", &["alpha"]);
        assert!(find("register vertical alpha", &register));
        assert!(find("register vert", &register));
        assert!(!find("register horizontal", &register));
        assert!(!find("adder alpha", &register));
    }

    #[test]
    fn minus_and_not_exclude() {
        let register = schematic("register", "vert", &["alpha", "torchless"]);
        assert!(!find("register -torchless", &register));
        assert!(!find("register not torchless", &register));
        assert!(find("register -carry_lookahead", &register));
    }

    #[test]
    fn quotes_and_underscores_keep_words_together() {
        let adder = schematic("adder", "horz", &["carry_lookahead"]);
        assert!(find("\"carry lookahead\"", &adder));
        assert!(find("carry_lookahead", &adder));
        assert!(!find("-\"carry lookahead\"", &adder));
    }

    #[test]
    fn width_and_unknown_words() {
        let adder = schematic("adder", "horz", &[]);
        assert!(find("8b", &adder));
        assert!(find("08b", &adder));
        assert!(!find("16b", &adder));
        assert!(find("add", &adder), "falls back to the title");
    }

    #[test]
    fn a_known_word_does_not_match_inside_another() {
        let mut unsynced = schematic("adder", "horz", &[]);
        unsynced.synchronized = Some(false);
        let mut synced = unsynced.clone();
        synced.synchronized = Some(true);
        let found = matching(&parse("sync"), vec![unsynced, synced]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].synchronized, Some(true));
    }
}
