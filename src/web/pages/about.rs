//! The About page, and its guide to how builds are sorted: categories,
//! designs, measures, facts, flags, tags, search, size, and similar builds.

use std::sync::LazyLock;

use maud::{Markup, PreEscaped, html};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::{
    category::Stacking,
    models::{catalogue::Category, user::User},
};

use super::{
    PAGE_TITLE, SECTION_TITLE, TABLE_CELL, TABLE_HEAD, base, sections,
};

/// What the site is, linking to how builds are sorted.
pub fn about(viewer: Option<&User>) -> Markup {
    base(
        viewer,
        "About",
        "/about",
        &html! {
            div class="max-w-prose leading-prose" {
                h1 class={ (PAGE_TITLE) " mb-4" } { "About" }
                p class="mb-4" {
                    "Minecraft redstone schematics, each stored with its facts so it can be found, "
                    "compared, and downloaded under a descriptive name."
                }
                p class="mb-4" {
                    a href="/about/categories" { "How builds are sorted" }
                    ": categories, designs, measures, tags, search, and leaderboards."
                }
                p class="mb-4" {
                    a href="/about/conventions" { "Building conventions" }
                    ": block palette, inputs and outputs, signs, bit order, stacking, and bounding box."
                }
                p class="mb-4" {
                    a href="/about/simulation" { "What the site checks" }
                    ": which categories it simulates, recognizes in an upload, reads from the file, and "
                    "finds the stacking of."
                }
            }
        },
    )
}

/// The building conventions, from `conventions.md` as the site was built.
static CONVENTIONS: LazyLock<String> =
    LazyLock::new(|| markdown(include_str!("../../../conventions.md")));

/// The building conventions every build follows.
pub fn conventions(viewer: Option<&User>) -> Markup {
    base(
        viewer,
        "Building conventions",
        "/about",
        &html! {
            div class="doc max-w-prose" { (PreEscaped(CONVENTIONS.as_str())) }
        },
    )
}

/// `text`, in Markdown, as HTML. A link to another of the repository's
/// Markdown files, which the site does not serve, keeps only its words.
fn markdown(text: &str) -> String {
    let mut in_file_link = false;
    let events = Parser::new_ext(text, Options::ENABLE_TABLES).filter(
        |event| match event {
            Event::Start(Tag::Link { dest_url, .. })
                if markdown_file(dest_url) =>
            {
                in_file_link = true;
                false
            }
            Event::End(TagEnd::Link) if in_file_link => {
                in_file_link = false;
                false
            }
            _ => true,
        },
    );
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, events);
    html
}

/// Whether `url` leads to a Markdown file beside this one, such as
/// `features.md#lint-checks`, rather than to a page.
fn markdown_file(url: &str) -> bool {
    let path = url.split('#').next().unwrap_or_default();
    !url.contains(':')
        && std::path::Path::new(path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

/// How builds are sorted, with the standard categories under their
/// sections.
pub fn sorting(viewer: Option<&User>, categories: &[Category]) -> Markup {
    base(
        viewer,
        "How builds are sorted",
        "/about",
        &html! {
            div class="max-w-prose leading-prose" {
                h1 class={ (PAGE_TITLE) " mb-4" } { "How builds are sorted" }
                p class="mb-4" {
                    "Each build has a category, a design, measures it is named and ranked by, and facts "
                    "read from its file or typed."
                }
                (category_part(categories))
                (design_part())
                (measure_part())
                (fact_part())
                (flag_part())
                (tag_part())
                (search_part())
                (size_part())
                (similar_part())
            }
        },
    )
}

/// A section's heading, `id` naming it for links.
fn heading(id: &str, text: &str) -> Markup {
    html! { h2 id=(id) class={ (SECTION_TITLE) " mt-10 mb-3" } { (text) } }
}

/// Categories, with the standard ones under their sections.
fn category_part(categories: &[Category]) -> Markup {
    html! {
        (heading("categories", "Categories"))
        p class="mb-3" {
            "A category is one function: one truth table, up to inverted lines, such as a torch on each "
            "bit of a bus. Inverted lines are flags; a subtractor is an adder with B inverted, flagged "
            "Subtract. Each shares one category: adders with carry in, carry out, or output inverted or "
            "not; XOR and XNOR gates; signed and unsigned comparators; monostables pulsing as their input "
            "turns on or off; registers reading cells inverted or not."
        }
        p class="mb-3" {
            "The sidebar groups categories into sections. A build fitting none names its own category, "
            "such as 3x3 piston door, under Other."
        }
        dl class="mb-3 grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1.5" {
            @for (key, title, members) in sections(categories) {
                @if key != "other" {
                    dt class="font-semibold" { (title) }
                    dd {
                        @for (i, category) in members.iter().enumerate() {
                            @if i > 0 { ", " }
                            a href={ "/?category=" (category.name) } { (category.label) }
                        }
                    }
                }
            }
        }
    }
}

/// What a design is.
fn design_part() -> Markup {
    html! {
        (heading("designs", "Designs"))
        p class="mb-3" {
            "How a build works, such as an adder's ripple carry or carry lookahead. A category's designs "
            "do the same job, so leaderboards rank them together unless you pick one. Unknown or unlisted "
            "designs are Other."
        }
    }
}

/// The numbers builds are named and ranked by.
fn measure_part() -> Markup {
    html! {
        (heading("measures", "Measures"))
        p class="mb-3" {
            "The numbers a category's builds are named and ranked by, in order; usually width in bits, "
            "as 8-bit Carry Lookahead Adder. Exceptions:"
        }
        ul class="mb-3 list-disc pl-5" {
            li { "RAM and serial write memories add capacity in bytes: 8-bit 32-byte RAM." }
            li { "Serial adders go by throughput, ticks from one input bit to the next: 2-tick Serial Adder." }
            li { "Thresholds add k, the fewest inputs on that turn it on: 8-bit At Least 3 Threshold." }
        }
        p class="mb-3" {
            "Builds compete only at equal measures. Leaderboards also split by stateless or stateful, "
            "encoding, and orientation."
        }
    }
}

/// The facts a build has, measured or typed.
fn fact_part() -> Markup {
    html! {
        (heading("facts", "Facts about a build"))
        dl class="mb-3 grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2" {
            dt class="font-semibold" { "Orientation" }
            dd { "Horizontal, vertical, or diagonal: the way its bits run. Read from an adder's lime inputs, or a 1-bit adder's stacking." }
            dt class="font-semibold" { "Encoding" }
            dd { "Binary, or hex for inputs that take a signal strength." }
            dt class="font-semibold" { "Stateless" }
            dd {
                "Settled outputs depend only on inputs, as an adder's. One that remembers, as a register, is "
                "stateful. Set per category; a build records it only when it differs, as a stateful multiplier."
            }
            dt class="font-semibold" { "Latency" }
            dd { "Ticks from input A changing until the slowest output settles, excluding status lines such as carry out. A register's runs data in to data out through a cell's write and read, so a data loop's latency is its ALU's plus its registers'." }
            dt class="font-semibold" { "Synchronized" }
            dd { "Every output bit changes once, all on the same tick." }
            dt class="font-semibold" { "Throughput" }
            dd { "Fewest ticks between inputs that still give right outputs." }
            dt class="font-semibold" { "Works from" }
            dd {
                "Oldest Minecraft: Java Edition release with every redstone component and mechanic used, "
                "such as 1.5 for a comparator, 1.16 for a target block. Building blocks don't count, being "
                "interchangeable; dust on a transparent block does, impossible before 1.2.1. Every build "
                "works in the latest release."
            }
            dt class="font-semibold" { "Pipelined" }
            dd { "Stateless and synchronized, with throughput below latency: a new input enters before the last result leaves. Derived, never typed." }
        }
        p class="mb-3" {
            "Builds of simulated categories run through their inputs; once every case is right, latency "
            "and synchronized come from the simulation, not what was typed. "
            a href="/about/simulation" { "What the site checks" } " lists those categories."
        }
    }
}

/// Flags, the yes-or-no facts of one category.
fn flag_part() -> Markup {
    html! {
        (heading("flags", "Flags"))
        p class="mb-3" {
            "Yes-or-no facts of one category, most adding truth-table columns: an ALU's operations beyond "
            "add and subtract, such as AND, XOR, and shift left; an adder's carry in and carry out; a "
            "multiplier's upper product, the product's high half."
        }
        p class="mb-3" {
            "Flags split neither categories, since shared columns agree, nor leaderboards. They show on the "
            "build's page and match in search."
        }
    }
}

/// Tags, by kind.
fn tag_part() -> Markup {
    html! {
        (heading("tags", "Tags"))
        p class="mb-3" {
            "Optional and cross-category; a missing tag says nothing. Colour gives the kind:"
        }
        dl class="mb-3 grid grid-cols-[auto_minmax(0,1fr)] items-center gap-x-4 gap-y-2" {
            dt { span class="badge badge-restriction" { "Restriction" } }
            dd {
                "Components the build does without, read from the file; nobody adds or removes them, and the "
                "upload form shows them once the file is read. Torchless: no redstone torch, standing or on a "
                "wall. Alpha: works from Alpha, using only dust, torches, levers, buttons, and pressure plates, "
                "none on a transparent block."
            }
            dt { span class="badge badge-other" { "Other" } }
            dd { "Anything else worth searching by, such as instant." }
            dt { span class="badge badge-failing" { "Simulation" } }
            dd {
                "Fails simulation: the simulated build got some cases wrong. Nobody adds or removes it; a "
                "passing new version drops it."
            }
        }
    }
}

/// What search matches.
fn search_part() -> Markup {
    html! {
        (heading("search", "Search"))
        p class="mb-3" {
            "Every word must match one of: category, design, flag, tag, orientation, width ("
            code { "8b" } "), latency (" code { "3t" } "), " code { "stateless" } " or " code { "stateful" }
            ", " code { "sync" } ", " code { "pipelined" } ", " code { "throughput:3" } ", "
            code { "encoding:hex" } ", a release it works in (" code { "version:1.12" } "), or a credited "
            "person. " code { "-" } " before a word excludes it. Six or more characters of an id, such as "
            code { "3234d4a4" } ", find that build. Category aliases match too: " code { "connect4" }
            " finds the four-in-a-row detector, " code { "inc" } " the incrementers, " code { "bcd" }
            " both BCD converters."
        }
    }
}

/// How size is measured, and a stackable build's volume per copy.
fn size_part() -> Markup {
    html! {
        (heading("size", "Size and stacking"))
        p class="mb-3" {
            "Size is the box less any outer layer holding only inputs and outputs."
        }
        p class="mb-3" {
            "Stackable builds repeat along a step, such as 2 blocks up; copies may overlap. Volume per "
            "copy, with the step in place of the length that way, is what is shown, sorted, and ranked: "
            "a 4×4×6 1-bit adder stacking every 2 up is 48; a 2×6×6 one stacking every 5 west is 60. An "
            "adder's step the site finds itself, by simulating stacked copies."
        }
    }
}

/// Which builds are listed as similar.
fn similar_part() -> Markup {
    html! {
        (heading("similar", "Similar builds"))
        p class="mb-3" {
            "A build's page lists same-category builds with at least 90% the same redstone: the share of "
            "dust, repeaters, comparators, torches, and other components lining up, facing the same way, "
            "after the best turn, mirror, and shift. Filler blocks don't count. An upload that close gets "
            "a warning linking to upload it as a new version of that build instead."
        }
    }
}

/// A category the site simulates, and what it works out for itself about
/// its builds.
pub struct Checked {
    pub name: String,
    pub plural: String,
    /// Whether an upload may be taken for one: `Yes`, `No`, or when.
    pub recognized: &'static str,
    /// What a conventional build's file says, over what was typed, such as
    /// `width`.
    pub read: Vec<String>,
    /// The flags its file says, by label.
    pub read_flags: Vec<String>,
    /// What a build that passes takes from its simulation, such as
    /// `latency`.
    pub measured: Vec<String>,
    /// The flags its simulation says, by label.
    pub measured_flags: Vec<String>,
    pub stacking: Stacking,
}

/// What the site works out for itself: a row for each category it
/// simulates, then those it does not, and what the simulator cannot run.
pub fn simulation_guide(
    viewer: Option<&User>,
    checked: &[Checked],
    unchecked: &[String],
) -> Markup {
    base(
        viewer,
        "What the site checks",
        "/about",
        &html! {
            div class="max-w-prose leading-prose" {
                h1 class={ (PAGE_TITLE) " mb-4" } { "What the site checks" }
                p class="mb-4" {
                    "Builds of these categories are simulated through every input case, or every case of each "
                    "byte when there are too many. One that passes every case takes the facts below from the "
                    "simulation or its file, not what was typed, each checked on its page."
                }
            }
            (checked_table(checked))
            (simulation_notes(unchecked))
        },
    )
}

/// A row for each category the site simulates: whether an upload is
/// taken for one, what is read from its file and its simulation, and how
/// its stacking is decided.
fn checked_table(checked: &[Checked]) -> Markup {
    let stacking = |stacking: Stacking| match stacking {
        Stacking::Inferred => "Found by the simulation",
        Stacking::Found => "Searched for; the builder's when none is found",
        Stacking::Given => "Given by the builder",
        Stacking::Never => "Never stacked",
    };
    // Such as `Width, orientation, and encoding`, then the flags beneath.
    let listed = |items: &[String], flags: &[String]| {
        let mut words = match items {
            [] => String::new(),
            [one] => one.clone(),
            [first, second] => format!("{first} and {second}"),
            [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
        };
        if let Some(first) = words.get(..1) {
            words.replace_range(..1, &first.to_uppercase());
        }
        html! {
            @if items.is_empty() && flags.is_empty() {
                span class="text-muted" { "Nothing" }
            }
            (words)
            @if !flags.is_empty() {
                span class="block text-meta text-muted" { "Flags: " (flags.join(", ")) }
            }
        }
    };
    html! {
    div class="relative overflow-x-auto" {
        table class="w-full border-collapse" {
            caption class="sr-only" { "What the site works out for each category it simulates" }
            thead {
                tr {
                    th scope="col" class=(TABLE_HEAD) { "Category" }
                    th scope="col" class=(TABLE_HEAD) { "Recognized in an upload" }
                    th scope="col" class=(TABLE_HEAD) { "Read from the file" }
                    th scope="col" class=(TABLE_HEAD) { "From the simulation" }
                    th scope="col" class=(TABLE_HEAD) { "Stacking" }
                }
            }
            tbody {
                @for row in checked {
                    tr {
                        th scope="row" class={ (TABLE_CELL) " text-left font-normal" } {
                            a href={ "/?category=" (row.name) } { (row.plural) }
                        }
                        td class=(TABLE_CELL) { (row.recognized) }
                        td class=(TABLE_CELL) { (listed(&row.read, &row.read_flags)) }
                        td class=(TABLE_CELL) { (listed(&row.measured, &row.measured_flags)) }
                        td class=(TABLE_CELL) { (stacking(row.stacking)) }
                    }
                }
            }
        }
    }
    }
}

/// What each column of the table means, the categories not simulated,
/// and what the simulator cannot run.
fn simulation_notes(unchecked: &[String]) -> Markup {
    html! {
    div class="max-w-prose leading-prose" {
        (heading("columns", "What each column means"))
        dl class="mb-3 grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2" {
            dt class="font-semibold" { "Recognized" }
            dd {
                "An upload is run quickly through every test its inputs and outputs fit; the best, at "
                "least 3 cases in 4, sets its starting category. ALU is tried only when control-line signs "
                "name operations, since any build with a control line could pass for one."
            }
            dt class="font-semibold" { "Read from the file" }
            dd {
                "Width, orientation, and encoding come from input A of a conventional build; an "
                "adder's or subtractor's carry or borrow flags from whether it has the lone lines."
            }
            dt class="font-semibold" { "From the simulation" }
            dd {
                "Once every case is right: latency and synchronized, a monostable's pulse, and flags the "
                "cases show, such as a comparator being signed or a monostable's edge."
            }
            dt class="font-semibold" { "Stacking" }
            dd {
                "Found by the simulation: adder copies stack at its bit spacing, or a 1-bit adder's at "
                "the shortest step that adds right; the result, unlimited copies or none, overrides what "
                "was typed. Searched for: copies stack west at each step up to the build's width, or two "
                "past it for a monostable; the shortest working step, with the most working copies up to "
                "16, replaces the builder's west step. Given: the builder's; saving checks only that two "
                "copies fit."
            }
        }
        (heading("not-simulated", "Not simulated yet"))
        p class="mb-3" {
            (unchecked.join(", ")) ", and any category off the list. Their facts are as typed."
        }
        (heading("limits", "What the simulator cannot run"))
        p class="mb-3" {
            "The simulator models dust, torches, repeaters, comparators, levers, stone buttons, lamps, "
            "and the barrels, furnaces, and hoppers a comparator reads. Builds with anything else, such "
            "as a piston, observer, or wooden button, or wider than 32 bits, are skipped rather than "
            "simulated wrongly; one in a category whose every detail it works out, as adders, goes under "
            "a category of its own. It steps a tick at a time, ignoring the game's block update order, "
            "so builds relying on that order may differ in game."
        }
    }
    }
}
