//! Narrowing the schematic list with a query such as
//! `register vertical alpha -torchless`.
//!
//! Every term must match, so terms AND together. A term prefixed with `-` or
//! written after `not` must not match. A term matches a schematic when it names
//! one of its facts (category or one of its aliases, such as `connect4`,
//! orientation, a tag, its design or a flag, its width such as `8b`, or a width
//! its copies stack to where they add bits, as `16b` finds a 1-bit adder slice
//! that stacks, a threshold's `at least 3`, its latency such as `3t`, a
//! builder, contributor, or importer, `stackable`, `sync` or `unsync`,
//! `stateless` or `stateful`, as `comb` and `seq` still are, `pipelined`). A
//! name can be tied to its role, as in `builder:embizone`,
//! `contributor:alex_you`, or `importer:lookaside`, a release it works in is
//! `version:1.12` or `version:beta_1.3`, and the throughput is `throughput:3`,
//! or `3t` for a serial build, whose throughput leads its name as its width
//! would. A term that is no fact of any schematic, nor in the singular, as
//! `adders` is `adder`, matches when it appears in a schematic's title,
//! description, or credited names instead, so `add` finds adders, `owen` finds
//! Owenage_'s builds, and `sync` does not find `unsync`. Six or more hex
//! digits, as `3234d4a4`, find the build whose id holds them. Quotes keep a
//! multi-word term together, as in `"carry lookahead"`; underscores work too,
//! and a number before a unit, as in `3 ticks` or `8 bit`, is one term.

use std::collections::HashSet;

use crate::{
    category::{Flag, Measure, NO_DESIGN},
    models::schematic::{Logic, Schematic},
    recognize::colour,
};

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
                    None => {
                        forms(&term.text).any(|form| text.contains(&form))
                            || in_id(&term.text, s)
                    }
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

/// The fewest hex digits a term is taken as part of an id by, so a short
/// word of them, as `add` or `8b`, is not.
const ID_PART: usize = 6;

/// Whether `text` is part of build `s`'s id, as the start of one the site
/// gives, such as `3234d4a4`.
fn in_id(text: &str, s: &Schematic) -> bool {
    text.len() >= ID_PART
        && text.chars().all(|c| c.is_ascii_hexdigit())
        && s.id.contains(text)
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

/// The widest a stack is searched by, for a build whose copies add bits
/// with no limit: past any width there is a build of.
const WIDEST: i64 = 64;

/// The widths build `s` comes in, each with the copies that make it: its
/// own, one copy, then each a stack of its copies reaches where they add
/// bits, as a 1-bit adder slice's do, up to the most copies that work.
pub fn widths(s: &Schematic) -> Vec<(i64, i64)> {
    let Some(w) = s.width.filter(|&w| w > 0) else {
        return Vec::new();
    };
    let mut widths = vec![(w, 1)];
    for direction in &s.stacking {
        if !direction.adds_bits(s.orientation) {
            continue;
        }
        let most = direction.max_copies.unwrap_or(WIDEST / w);
        widths.extend(
            (2..=most)
                .map(|copies| (w * copies, copies))
                .take_while(|&(width, _)| width <= WIDEST),
        );
    }
    widths.sort_unstable();
    widths.dedup_by_key(|(width, _)| *width);
    widths
}

/// The width `terms` ask for that build `s` reaches only by stacking, and
/// the copies it takes, such as 16 bits as 16 copies of a 1-bit slice.
pub fn stacked_width(terms: &[Term], s: &Schematic) -> Option<(i64, i64)> {
    let asked = terms.iter().filter(|term| !term.negated).find_map(|term| {
        let number = ["-bit", " bits", " bit", "b"]
            .iter()
            .find_map(|unit| term.text.strip_suffix(unit))?;
        number.parse::<i64>().ok()
    })?;
    widths(s)
        .into_iter()
        .find(|&(width, copies)| width == asked && copies > 1)
}

/// Every word a term can name a schematic by.
fn facts(s: &Schematic) -> Vec<String> {
    let mut facts = vec![s.category.replace('_', " ")];
    facts.extend(s.aliases.iter().cloned());
    // Its tags as its page shows them, its design, orientation, flags, and
    // stacking among them, each by its name, label, and token.
    for tag in s.every_tag() {
        facts.extend([
            tag.name.replace('_', " "),
            tag.label().to_lowercase(),
            tag.token().to_lowercase().replace('_', " "),
        ]);
    }
    // Its own width, and each its copies reach where they add bits.
    for (w, _) in widths(s) {
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
    // A monostable's name leads with its pulse, so its ticks find it too.
    if let Some(p) = s.pulse {
        facts.push(format!("pulse:{p}"));
        facts.extend(ticks(p));
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
    if !s.conventional {
        facts.push("unconventional".to_owned());
    }
    // Its design by its own label, and no design as `other`, which is no
    // tag.
    facts.push(s.design.as_deref().unwrap_or(NO_DESIGN).replace('_', " "));
    if let Some(label) = &s.design_given_label {
        facts.push(label.to_lowercase());
    }
    for flag in &s.flags {
        match flag.name {
            // The short names a carry or borrow line's sign may use.
            Flag::CARRY_IN => facts.push(colour::CARRY_IN[1].to_owned()),
            Flag::CARRY_OUT => facts.push(colour::CARRY_OUT[1].to_owned()),
            Flag::BORROW_IN => facts.push(colour::BORROW_IN[1].to_owned()),
            Flag::BORROW_OUT => facts.push(colour::BORROW_OUT[1].to_owned()),
            // How one line reads, which no tag says.
            Flag::INVERTED_CARRY_IN => {
                facts.push(flag.name.replace('_', " "));
            }
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

/// The words for whether a build is synchronized, stateless or stateful,
/// as combinational and sequential still find them, and pipelined.
fn timing(s: &Schematic) -> Vec<&'static str> {
    let mut words = match s.synchronized {
        Some(true) => vec!["sync", "synced", "synchronized"],
        Some(false) => vec!["unsync", "unsynced", "unsynchronized"],
        None => Vec::new(),
    };
    match s.logic {
        Some(Logic::Stateless) => {
            words.extend(["stateless", "comb", "combinational"]);
        }
        Some(Logic::Stateful) => {
            words.extend(["stateful", "seq", "sequential"]);
        }
        None => {}
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
    crate::schem::works_from::known()
        .into_iter()
        .filter(|release| crate::schem::works_from::works_in(from, release))
        .map(|release| format!("version:{}", release.to_lowercase()))
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::models::{
        schematic::{Encoding, Orientation},
        tag::Tag,
    };

    /// A build of `category` with nothing but its orientation and `tags`,
    /// for tests elsewhere too.
    pub(crate) fn schematic(
        category: &str,
        orientation: Orientation,
        tags: &[&str],
    ) -> Schematic {
        Schematic {
            id: String::new(),
            category: category.to_owned(),
            category_label: None,
            width: Some(8),
            orientation,
            encoding: Some(Encoding::Binary),
            latency: None,
            synchronized: None,
            logic: Some(Logic::Stateless),
            own_logic: None,
            throughput: None,
            at_least: None,
            pulse: None,
            measures: vec![Measure::Width],
            aliases: Vec::new(),
            torches: None,
            works_from: None,
            works_from_by: None,
            bytes: None,
            size: (1, 1, 1),
            stacking: Vec::new(),
            widened: Vec::new(),
            stacked_from: None,
            builders: vec!["embizone".to_owned()],
            contributors: Vec::new(),
            built_on: None,
            imported_by: None,
            conventional: true,
            keeps_paste: false,
            design: None,
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
            public: true,
            unverified: None,
            tags: tags
                .iter()
                .map(|name| Tag {
                    name: (*name).to_owned(),
                    token: None,
                    kind: crate::models::tag::Kind::Other,
                })
                .collect(),
        }
    }

    fn find(query: &str, s: &Schematic) -> bool {
        !matching(&parse(query), vec![s.clone()]).is_empty()
    }

    #[test]
    fn terms_and_together() {
        let register = schematic("register", Orientation::Vertical, &["alpha"]);
        assert!(find("register vertical alpha", &register));
        assert!(find("register vert", &register));
        assert!(!find("register horizontal", &register));
        assert!(!find("adder alpha", &register));
    }

    #[test]
    fn minus_and_not_exclude() {
        let register = schematic(
            "register",
            Orientation::Vertical,
            &["alpha", "torchless"],
        );
        assert!(!find("register -torchless", &register));
        assert!(!find("register not torchless", &register));
        assert!(find("register -carry_lookahead", &register));
    }

    #[test]
    fn quotes_and_underscores_keep_words_together() {
        let adder =
            schematic("adder", Orientation::Horizontal, &["carry_lookahead"]);
        assert!(find("\"carry lookahead\"", &adder));
        assert!(find("carry_lookahead", &adder));
        assert!(!find("-\"carry lookahead\"", &adder));
    }

    #[test]
    fn part_of_a_credited_name_finds_their_builds() {
        let s = Schematic {
            builders: vec!["Don_Manuel_1229".to_owned()],
            contributors: vec!["Owenage_".to_owned()],
            ..schematic("adder", Orientation::Horizontal, &[])
        };
        assert!(find("don manuel", &s));
        assert!(find("owen", &s));
        assert!(!find("zebek", &s));
    }

    #[test]
    fn a_plural_finds_its_singular() {
        let s = schematic("multiplier", Orientation::Horizontal, &[]);
        assert!(find("multipliers", &s));
        let memory =
            schematic("serial_write_memory", Orientation::Horizontal, &[]);
        assert!(find("memories", &memory));
    }

    #[test]
    fn latency_and_a_number_before_its_unit() {
        let s = Schematic {
            latency: Some(3),
            ..schematic("adder", Orientation::Horizontal, &[])
        };
        assert!(find("3t", &s));
        assert!(find("3 ticks", &s));
        assert!(find("adder 3 ticks", &s));
        assert!(!find("4 ticks", &s));
        assert!(find("8 bits", &s));
    }

    #[test]
    fn a_stack_answers_to_the_widths_its_copies_reach() {
        use crate::models::schematic::StackDirection;
        let stacks = |width, west, north, max_copies| {
            let mut s = schematic("adder", Orientation::Horizontal, &[]);
            s.width = Some(width);
            s.stacking = vec![StackDirection {
                west,
                up: 0,
                north,
                max_copies,
                faces: None,
            }];
            s
        };
        let slice = stacks(1, 2, 0, None);
        assert!(find("16b", &slice) && find("16 bit adder", &slice));
        assert_eq!(
            stacked_width(&parse("16 bit adder"), &slice),
            Some((16, 16))
        );
        let eight = stacks(8, 9, 0, None);
        assert!(find("16b", &eight) && !find("12b", &eight));
        let capped = stacks(1, 2, 0, Some(8));
        assert!(find("8b", &capped) && !find("16b", &capped));
        let file = stacks(8, 0, 4, None);
        assert!(!find("16b", &file));
        assert_eq!(stacked_width(&parse("8b"), &eight), None);
        // Each tag its page shows finds it, how far apart copies stack too.
        assert!(find("stackable", &slice) && find("2-wide_stackable", &slice));
        assert!(!find("2-wide_stackable", &eight));
    }

    #[test]
    fn width_and_unknown_words() {
        let adder = schematic("adder", Orientation::Horizontal, &[]);
        assert!(find("8b", &adder));
        assert!(find("08b", &adder));
        assert!(!find("16b", &adder));
        assert!(find("add", &adder), "falls back to the title");
    }

    #[test]
    fn a_capacity_matches_in_bytes_not_bits() {
        let ram = Schematic {
            bytes: Some(32),
            ..schematic("ram", Orientation::Horizontal, &[])
        };
        assert!(find("32-byte", &ram));
        assert!(!find("32b", &ram), "32b means 32 bits wide");
    }

    #[test]
    fn a_serial_throughput_matches_in_ticks() {
        let serial = Schematic {
            throughput: Some(2),
            measures: vec![Measure::Throughput],
            ..schematic("serial_adder", Orientation::Horizontal, &[])
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
            ..schematic("threshold", Orientation::Horizontal, &[])
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
            ..schematic("adder", Orientation::Horizontal, &[])
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
            ..schematic("adder", Orientation::Horizontal, &[])
        };
        assert!(find("throughput:3", &adder));
        assert!(!find("3t", &adder), "3t is a latency");
    }

    #[test]
    fn a_monostable_is_found_by_its_pulse() {
        let monostable = Schematic {
            pulse: Some(3),
            latency: Some(1),
            ..schematic("monostable", Orientation::Horizontal, &[])
        };
        assert!(find("pulse:3", &monostable));
        assert!(find("3t", &monostable), "its name leads with 3-tick");
        assert!(find("1t", &monostable), "1t is its latency");
        assert!(!find("pulse:1", &monostable));
    }

    #[test]
    fn state_and_pipelining_are_words() {
        let piped = Schematic {
            throughput: Some(3),
            latency: Some(8),
            synchronized: Some(true),
            ..schematic("multiplier", Orientation::Horizontal, &[])
        };
        assert!(find("comb pipelined", &piped));
        assert!(find("stateless pipelined", &piped));
        assert!(!find("seq", &piped));
        let sequential = Schematic {
            logic: Some(Logic::Stateful),
            ..piped
        };
        assert!(find("sequential", &sequential));
        assert!(find("stateful", &sequential));
        assert!(!find("pipe", &sequential));
    }

    #[test]
    fn a_builder_matches_whatever_its_case_and_underscores() {
        let s = Schematic {
            builders: vec!["FEARLESS_Z_".to_owned()],
            ..schematic("adder", Orientation::Horizontal, &[])
        };
        let terms = parse("fearless_z_");
        assert_eq!(matching(&terms, vec![s]).len(), 1);
    }

    #[test]
    fn a_role_ties_a_name_to_it() {
        let built = Schematic {
            builders: vec!["Alex_you".to_owned()],
            ..schematic("adder", Orientation::Horizontal, &[])
        };
        let helped = Schematic {
            contributors: vec!["alex_you".to_owned()],
            ..schematic("register", Orientation::Horizontal, &[])
        };
        let found = matching(&parse("builder:Alex_you"), vec![built, helped]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].category, "adder");
    }

    #[test]
    fn part_of_an_id_finds_its_build() {
        let s = Schematic {
            id: "3234d4a46edc0fffa584336cb45bf12e".to_owned(),
            ..schematic("adder", Orientation::Horizontal, &[])
        };
        assert!(find("3234d4a4", &s));
        assert!(find("3234D4A46EDC0FFFA584336CB45BF12E", &s));
        assert!(find("edc0ff", &s), "any part of it");
        assert!(!find("3234d4a5", &s));
        assert!(!find("-3234d4a4", &s));
        assert!(!find("3234d", &s), "too short to be taken as an id");
    }

    #[test]
    fn a_known_word_does_not_match_inside_another() {
        let mut unsynced = schematic("adder", Orientation::Horizontal, &[]);
        unsynced.synchronized = Some(false);
        let mut synced = unsynced.clone();
        synced.synchronized = Some(true);
        let found = matching(&parse("sync"), vec![unsynced, synced]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].synchronized, Some(true));
    }
}
