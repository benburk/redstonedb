//! Narrowing the schematic list with a query such as
//! `register vertical alpha -torchless`.
//!
//! Every term must match, so terms AND together. A term prefixed with `-`
//! or written after `not` must not match. A term matches a schematic when
//! it names one of its facts (category, orientation, encoding, a tag, its
//! width such as `8b`, a builder, contributor, or importer, `stackable`,
//! `sync` or `unsync`). A name can be tied to its role, as in
//! `builder:embizone`, `contributor:alex_you`, or `importer:lookaside`. A
//! term that is no fact of any schematic matches when it appears in a
//! schematic's title or description instead, so `add` finds adders while
//! `sync` does not find `unsync`. Quotes keep a multi-word term together,
//! as in `"carry lookahead"`; underscores work too.

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
                "{} {}",
                s.title(),
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
        s.category.replace('_', " "),
        s.orientation.clone(),
        s.orientation_label().to_lowercase(),
    ];
    facts.extend(s.encoding.clone());
    facts.extend(s.encoding_label().map(str::to_lowercase));
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
    if let Some(b) = s.bytes {
        facts.extend([
            format!("{b}-byte"),
            format!("{b} byte"),
            format!("{b} bytes"),
        ]);
    }
    if let Some(p) = s.period {
        facts.extend([
            format!("{p}t"),
            format!("{p}-tick"),
            format!("{p} tick"),
        ]);
    }
    // In-game names keep their capitals and underscores, but terms are
    // lowercase, with spaces for underscores.
    let person =
        |name: &String| name.to_lowercase().replace('_', " ").trim().to_owned();
    let people = s.builders.iter().chain(&s.contributors);
    facts.extend(
        people
            .chain(&s.imported_by)
            .chain(&s.uploader)
            .chain(&s.uploader_name)
            .map(person),
    );
    let roles = [
        ("builder", s.builders.iter().collect::<Vec<_>>()),
        ("contributor", s.contributors.iter().collect()),
        (
            "importer",
            s.imported_by
                .iter()
                .chain(&s.uploader)
                .chain(&s.uploader_name)
                .collect(),
        ),
    ];
    for (role, names) in roles {
        facts.extend(
            names
                .into_iter()
                .map(|name| format!("{role}:{}", person(name))),
        );
    }
    if !s.stacking.is_empty() {
        facts.push("stackable".to_owned());
    }
    if !s.conventional {
        facts.push("unconventional".to_owned());
    }
    facts.push(s.design.replace('_', " "));
    if let Some(short) = &s.design_abbreviation {
        facts.push(short.to_lowercase());
    }
    for flag in &s.flags {
        facts.push(flag.name.replace('_', " "));
        // The short names a carry line's sign may use.
        match flag.name.as_str() {
            "carry_in" => facts.push("cin".to_owned()),
            "carry_out" => facts.push("cout".to_owned()),
            _ => {}
        }
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
        category: &str,
        orientation: &str,
        tags: &[&str],
    ) -> Schematic {
        Schematic {
            id: String::new(),
            category: category.to_owned(),
            category_label: None,
            width: Some(8),
            orientation: orientation.to_owned(),
            encoding: Some("bin".to_owned()),
            latency: None,
            synchronized: None,
            period: None,
            bytes: None,
            size: (1, 1, 1),
            stacking: Vec::new(),
            builders: vec!["embizone".to_owned()],
            contributors: Vec::new(),
            built_on: None,
            imported_by: None,
            conventional: true,
            design: "other".to_owned(),
            design_abbreviation: None,
            design_given_label: None,
            flags: Vec::new(),
            description: None,
            given_name: None,
            uploader: None,
            uploader_name: None,
            sha256: String::new(),
            added_at: String::new(),
            edited_at: None,
            downloads: 0,
            tags: tags
                .iter()
                .map(|name| Tag {
                    name: (*name).to_owned(),
                    token: None,
                    kind: "other".to_owned(),
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
    fn a_capacity_matches_in_bytes_not_bits() {
        let ram = Schematic {
            bytes: Some(32),
            ..schematic("ram", "horz", &[])
        };
        assert!(find("32-byte", &ram));
        assert!(!find("32b", &ram), "32b means 32 bits wide");
    }

    #[test]
    fn a_period_matches_in_ticks() {
        let serial = Schematic {
            period: Some(2),
            ..schematic("serial_adder", "horz", &[])
        };
        assert!(find("2t", &serial));
        assert!(find("2-tick", &serial));
        assert!(!find("3t", &serial));
    }

    #[test]
    fn a_builder_matches_whatever_its_case_and_underscores() {
        let s = Schematic {
            builders: vec!["FEARLESS_Z_".to_owned()],
            ..schematic("adder", "horz", &[])
        };
        let terms = parse("fearless_z_");
        assert_eq!(matching(&terms, vec![s]).len(), 1);
    }

    #[test]
    fn a_role_ties_a_name_to_it() {
        let built = Schematic {
            builders: vec!["Alex_you".to_owned()],
            ..schematic("adder", "horz", &[])
        };
        let helped = Schematic {
            contributors: vec!["alex_you".to_owned()],
            ..schematic("register", "horz", &[])
        };
        let found = matching(&parse("builder:Alex_you"), vec![built, helped]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].category, "adder");
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
