//! Narrowing the schematic list with a query such as
//! `register vertical alpha -torchless`.
//!
//! Every term must match, so terms AND together. A term prefixed with `-`
//! or written after `not` must not match. A term matches a schematic when
//! it names one of its facts (category or one of its aliases, such as
//! `connect4`, orientation, a tag, its design or a flag, its width such as
//! `8b`, a threshold's `at least 3`, its latency such as `3t`, a builder,
//! contributor, or importer, `stackable`, `sync` or `unsync`, `comb` or
//! `seq`, `pipelined`). A name can be tied to its role, as in
//! `builder:embizone`, `contributor:alex_you`, or `importer:lookaside`, the
//! encoding is `encoding:bin` or `encoding:hex`, a release it works in is
//! `version:1.12` or `version:beta_1.3`, and the throughput is
//! `throughput:3`, or `3t` for a serial build, whose throughput leads its
//! name as its width would. A term that is no fact
//! of any schematic, nor in the singular, as `adders` is `adder`, matches
//! when it appears in a schematic's title, description, or credited names
//! instead, so `add` finds adders, `owen` finds Owenage_'s builds, and
//! `sync` does not find `unsync`. Quotes keep a multi-word term together,
//! as in `"carry lookahead"`; underscores work too, and a number before a
//! unit, as in `3 ticks` or `8 bit`, is one term.

use std::collections::HashSet;

use crate::models::schematic::{Measure, Schematic};

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

/// Units a number may be written before, as in `3 ticks`, so the two read
/// as one term.
const UNITS: [&str; 6] = ["bit", "bits", "tick", "ticks", "byte", "bytes"];

/// Splits on spaces, keeping quoted runs together and dropping the quotes;
/// a `-` before a quote stays with the quoted run, and a number stays with
/// a unit after it.
fn words(query: &str) -> Vec<String> {
    let mut joined: Vec<String> = Vec::new();
    for word in split(query) {
        let unit = UNITS.contains(&word.to_lowercase().as_str());
        match joined.last_mut() {
            Some(last)
                if unit
                    && !last.is_empty()
                    && last
                        .trim_start_matches('-')
                        .chars()
                        .all(|c| c.is_ascii_digit()) =>
            {
                last.push(' ');
                last.push_str(&word);
            }
            _ => joined.push(word),
        }
    }
    joined
}

/// Splits on spaces, keeping quoted runs together and dropping the quotes.
fn split(query: &str) -> Vec<String> {
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
    // Each term as a fact any schematic has, itself or in the singular, or
    // else as text to find.
    let terms: Vec<(&Term, Option<String>)> = terms
        .iter()
        .map(|term| (term, fact_named(&term.text, &known)))
        .collect();
    schematics
        .into_iter()
        .filter(|s| {
            let facts = facts(s);
            let text = text(s);
            terms.iter().all(|(term, fact)| {
                let found = match fact {
                    Some(fact) => facts.contains(fact),
                    None => forms(&term.text).any(|form| text.contains(&form)),
                };
                found != term.negated
            })
        })
        .collect()
}

/// The fact `text` names, as it is or in the singular, if any schematic has
/// it.
fn fact_named(text: &str, known: &HashSet<String>) -> Option<String> {
    forms(text).find(|form| known.contains(form))
}

/// `text` as it is, then as the singular it may be the plural of, as
/// `memories` may be `memory`.
fn forms(text: &str) -> impl Iterator<Item = String> + '_ {
    let singular = [
        text.strip_suffix("ies").map(|stem| format!("{stem}y")),
        text.strip_suffix("es").map(str::to_owned),
        text.strip_suffix('s').map(str::to_owned),
    ];
    std::iter::once(Some(text.to_owned()))
        .chain(singular)
        .flatten()
        .filter(|form| !form.is_empty())
}

/// What a term that is no fact is looked for in: the title, description,
/// and credited names, lowercase, names with spaces for underscores.
fn text(s: &Schematic) -> String {
    let names = s
        .builders
        .iter()
        .chain(&s.contributors)
        .map(|name| name.to_lowercase().replace('_', " "))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "{} {} {names}",
        s.title(),
        s.description.as_deref().unwrap_or("")
    )
    .to_lowercase()
}

/// Every word a term can name a schematic by.
fn facts(s: &Schematic) -> Vec<String> {
    let mut facts = vec![s.category.replace('_', " ")];
    facts.extend(s.aliases.iter().cloned());
    facts.extend([s.orientation.clone(), s.orientation_label().to_lowercase()]);
    // Not the bare words, so `binary` and `hex` find the converters by name.
    facts.extend(s.encoding.as_ref().map(|e| format!("encoding:{e}")));
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
            format!("{w} bits"),
        ]);
    }
    facts.extend(s.latency.into_iter().flat_map(ticks));
    if let Some(b) = s.bytes {
        facts.extend([
            format!("{b}-byte"),
            format!("{b} byte"),
            format!("{b} bytes"),
        ]);
    }
    if let Some(t) = s.throughput {
        facts.push(format!("throughput:{t}"));
        // Only a serial build's throughput goes by its ticks alone, as its
        // name does; another's `3t` is its latency.
        if s.has(Measure::Throughput) {
            facts.extend(ticks(t));
        }
    }
    if let Some(k) = s.at_least {
        facts.push(Measure::AtLeast.term(k));
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
    if let Some(label) = &s.design_given_label {
        facts.push(label.to_lowercase());
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
    facts.extend(timing(s).iter().map(|&word| word.to_owned()));
    facts.extend(versions(s));
    facts
}

/// The words for `n` ticks, such as `3t` and `3 ticks`.
fn ticks(n: i64) -> [String; 4] {
    [
        format!("{n}t"),
        format!("{n}-tick"),
        format!("{n} tick"),
        format!("{n} ticks"),
    ]
}

/// The words for whether a build is synchronized, combinational or
/// sequential, and pipelined.
fn timing(s: &Schematic) -> Vec<&'static str> {
    let mut words = match s.synchronized {
        Some(true) => vec!["sync", "synchronized"],
        Some(false) => vec!["unsync", "unsynchronized"],
        None => Vec::new(),
    };
    match s.logic.as_deref() {
        Some("comb") => words.extend(["comb", "combinational"]),
        Some("seq") => words.extend(["seq", "sequential"]),
        _ => {}
    }
    if s.pipelined() {
        words.extend(["pipe", "pipelined"]);
    }
    words
}

/// The releases a build works in, as `version:1.12` names them.
fn versions(s: &Schematic) -> Vec<String> {
    let Some(from) = &s.works_from else {
        return Vec::new();
    };
    crate::works_from::known()
        .into_iter()
        .filter(|release| crate::works_from::works_in(from, release))
        .map(|release| format!("version:{}", release.to_lowercase()))
        .collect()
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
            logic: Some("comb".to_owned()),
            own_logic: None,
            throughput: None,
            at_least: None,
            measures: vec![Measure::Width],
            aliases: Vec::new(),
            torches: None,
            works_from: None,
            works_from_by: None,
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
    fn part_of_a_credited_name_finds_their_builds() {
        let s = Schematic {
            builders: vec!["Don_Manuel_1229".to_owned()],
            contributors: vec!["Owenage_".to_owned()],
            ..schematic("adder", "horz", &[])
        };
        assert!(find("don manuel", &s));
        assert!(find("owen", &s));
        assert!(!find("zebek", &s));
    }

    #[test]
    fn a_plural_finds_its_singular() {
        let s = schematic("multiplier", "horz", &[]);
        assert!(find("multipliers", &s));
        let memory = schematic("serial_write_memory", "horz", &[]);
        assert!(find("memories", &memory));
    }

    #[test]
    fn latency_and_a_number_before_its_unit() {
        let s = Schematic {
            latency: Some(3),
            ..schematic("adder", "horz", &[])
        };
        assert!(find("3t", &s));
        assert!(find("3 ticks", &s));
        assert!(find("adder 3 ticks", &s));
        assert!(!find("4 ticks", &s));
        assert!(find("8 bits", &s));
    }

    #[test]
    fn encoding_is_named_with_its_role_not_by_the_bare_word() {
        let converter = schematic("hex_to_binary", "horz", &[]);
        let adder = schematic("adder", "horz", &[]);
        assert!(find("encoding:bin", &adder));
        assert!(find("binary", &converter));
        assert!(
            !find("binary", &adder),
            "an adder's encoding is not its name"
        );
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
    fn a_serial_throughput_matches_in_ticks() {
        let serial = Schematic {
            throughput: Some(2),
            measures: vec![Measure::Throughput],
            ..schematic("serial_adder", "horz", &[])
        };
        assert!(find("2t", &serial));
        assert!(find("2-tick", &serial));
        assert!(find("throughput:2", &serial));
        assert!(!find("3t", &serial));
    }

    #[test]
    fn a_category_is_found_by_its_aliases_and_a_threshold_by_its_k() {
        let threshold = Schematic {
            at_least: Some(3),
            aliases: vec!["majority".to_owned()],
            ..schematic("threshold", "horz", &[])
        };
        assert!(find("majority", &threshold));
        assert!(find("\"at least 3\"", &threshold));
        assert!(find("at_least_3", &threshold));
        assert!(!find("\"at least 4\"", &threshold));
    }

    #[test]
    fn a_build_is_found_in_every_release_it_works_in() {
        let s = Schematic {
            works_from: Some("1.5".to_owned()),
            ..schematic("adder", "horz", &[])
        };
        assert!(find("version:1.5", &s));
        assert!(find("version:1.12", &s));
        assert!(!find("version:1.4.2", &s));
        assert!(!find("version:beta_1.3", &s));
    }

    #[test]
    fn another_throughput_is_not_its_latency() {
        let adder = Schematic {
            throughput: Some(3),
            latency: Some(8),
            ..schematic("adder", "horz", &[])
        };
        assert!(find("throughput:3", &adder));
        assert!(!find("3t", &adder), "3t is a latency");
    }

    #[test]
    fn logic_and_pipelining_are_words() {
        let piped = Schematic {
            throughput: Some(3),
            latency: Some(8),
            synchronized: Some(true),
            ..schematic("multiplier", "horz", &[])
        };
        assert!(find("comb pipelined", &piped));
        assert!(!find("seq", &piped));
        let sequential = Schematic {
            logic: Some("seq".to_owned()),
            ..piped
        };
        assert!(find("sequential", &sequential));
        assert!(!find("pipe", &sequential));
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
