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
                    "A collection of Minecraft redstone schematics. Each build is stored with its facts, "
                    "so it can be found, compared, and downloaded under a name that describes it."
                }
                p class="mb-4" {
                    a href="/about/categories" { "How builds are sorted" }
                    " explains categories, designs, measures, tags, and what search and the leaderboards go by."
                }
                p class="mb-4" {
                    a href="/about/conventions" { "Building conventions" }
                    " sets out how a build is laid out: its block palette, inputs and outputs, signs, "
                    "bit order, stacking, and bounding box."
                }
                p class="mb-4" {
                    a href="/about/simulation" { "What the site checks" }
                    " lists the categories it simulates, recognizes in an upload, reads from the file, and "
                    "works out the stacking of."
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
                    "Every build has one category and one design, a few numbers it is named and ranked by, "
                    "and facts that are measured from its file or typed in. This page explains each, "
                    "and what the site does with it."
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
            "A category is one function. Two builds share a category only when they have the same truth "
            "table, the same outputs for the same inputs, so either could stand in for the other. A subtractor "
            "is no adder and a decrementer no incrementer, though each is the other with a torch on every bit "
            "of a bus. Inverting a few lines, whatever the width, does not split a category: XOR and XNOR "
            "gates share one, told apart by a flag, as do signed and unsigned comparators and monostables "
            "that pulse as their input turns on or off."
        }
        p class="mb-3" {
            "The sidebar groups the categories into sections. A build that fits none of them names its own "
            "category, such as 3x3 piston door, and is listed under Other."
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
            "A design is how a build works, such as ripple carry or carry lookahead for an adder. "
            "Every design in a category does the same job, so a leaderboard ranks them together "
            "unless you pick one. A build whose design is unknown, or not on its category's list, is Other."
        }
    }
}

/// The numbers builds are named and ranked by.
fn measure_part() -> Markup {
    html! {
        (heading("measures", "Measures"))
        p class="mb-3" {
            "Each category has measures, the numbers its builds are named and ranked by, in order. "
            "Most have one, the width in bits, as in 8-bit Carry Lookahead Adder. Three kinds have others:"
        }
        ul class="mb-3 list-disc pl-5" {
            li { "RAM and serial write memories add their capacity in bytes, as in 8-bit 32-byte RAM." }
            li { "A serial adder goes by its throughput, the ticks from one input bit to the next, as in 2-tick Serial Adder." }
            li { "A threshold adds k, the fewest inputs on that turn it on, as in 8-bit At Least 3 Threshold." }
        }
        p class="mb-3" {
            "Builds compete only with builds at the same value of every measure. On a leaderboard, a build "
            "also meets only builds that are stateless or stateful as it is, of the same encoding and orientation."
        }
    }
}

/// The facts a build has, measured or typed.
fn fact_part() -> Markup {
    html! {
        (heading("facts", "Facts about a build"))
        dl class="mb-3 grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2" {
            dt class="font-semibold" { "Orientation" }
            dd { "Horizontal, vertical, or diagonal, the way its bits run. An adder's is read from its lime inputs, or for a 1-bit adder from the way its copies stack." }
            dt class="font-semibold" { "Encoding" }
            dd { "Binary, or hex for inputs that take a signal strength." }
            dt class="font-semibold" { "Stateless" }
            dd {
                "Whether, once its inputs stop changing, its outputs depend only on them, as an adder's do, or it "
                "remembers something, as a register does, which makes it stateful. Each category is usually one or "
                "the other, and a build says so only when it differs, as a stateful multiplier does."
            }
            dt class="font-semibold" { "Latency" }
            dd { "Ticks from input A changing until the slowest output settles, leaving out status lines such as carry out. A register's is data in to data out, with a cell writing and read, so a data loop's latency is its ALU's plus its registers'." }
            dt class="font-semibold" { "Synchronized" }
            dd { "Whether every output bit changes once, all on the same tick." }
            dt class="font-semibold" { "Throughput" }
            dd { "The fewest ticks from one input to the next that still give right outputs." }
            dt class="font-semibold" { "Works from" }
            dd {
                "The oldest Minecraft: Java Edition release with every redstone component and mechanic "
                "the build uses, such as 1.5 for a comparator or 1.16 for a target block. Building blocks "
                "do not count, since any solid block can stand in for another, but dust on a transparent "
                "block does, as it could not go on one before 1.2.1. Every build here works in the latest "
                "release."
            }
            dt class="font-semibold" { "Pipelined" }
            dd { "A stateless, synchronized build whose throughput is below its latency, so a new input goes in before the last result comes out. The site works this out; nobody types it." }
        }
        p class="mb-3" {
            "A build of a category the site simulates is run through its inputs. Once every case "
            "comes out right, its latency and synchronized come from the simulation, not from what was "
            "typed. " a href="/about/simulation" { "What the site checks" } " lists those categories."
        }
    }
}

/// Flags, the yes-or-no facts of one category.
fn flag_part() -> Markup {
    html! {
        (heading("flags", "Flags"))
        p class="mb-3" {
            "Flags are yes-or-no facts that mean something in one category only. Most opt a build into "
            "columns of its category's truth table beyond the ones every build has. An ALU's flags name the "
            "operations it offers beyond adding and subtracting, such as AND, XOR, and shift left. An "
            "adder's carry in and carry out add an input and an output, and a multiplier's upper product "
            "adds the high half of the product."
        }
        p class="mb-3" {
            "Builds with different flags still share their category, since the columns they have in common "
            "agree. Flags show on the build's page and match in search, but do not split leaderboards."
        }
    }
}

/// Tags, by kind.
fn tag_part() -> Markup {
    html! {
        (heading("tags", "Tags"))
        p class="mb-3" {
            "Tags are optional and cross categories. A build without a tag says nothing about it. "
            "A tag's colour gives its kind:"
        }
        dl class="mb-3 grid grid-cols-[auto_minmax(0,1fr)] items-center gap-x-4 gap-y-2" {
            dt { span class="badge badge-restriction" { "Restriction" } }
            dd {
                "Components the build does without. The site reads both from the file, and nobody can add "
                "or remove them; the upload form shows them as soon as the file is read. A build is torchless "
                "when it has no redstone torch, standing or on a wall, and alpha when it works from Alpha, "
                "using nothing newer than dust, torches, levers, buttons, and pressure plates, none of them "
                "on a transparent block."
            }
            dt { span class="badge badge-other" { "Other" } }
            dd { "Anything else worth searching by, such as instant." }
            dt { span class="badge badge-failing" { "Simulation" } }
            dd {
                "Fails simulation means simulating the build got some cases wrong. Nobody can add or "
                "remove it; a new version that passes drops it."
            }
        }
    }
}

/// What search matches.
fn search_part() -> Markup {
    html! {
        (heading("search", "Search"))
        p class="mb-3" {
            "Search matches each word against these facts: category, design, flag, tag, orientation, "
            "a width such as " code { "8b" } ", a latency such as " code { "3t" } ", "
            code { "stateless" } " or " code { "stateful" } ", " code { "sync" } ", " code { "pipelined" } ", "
            code { "throughput:3" } ", " code { "encoding:hex" } ", a release it works in such as "
            code { "version:1.12" } ", and the people credited. "
            "Six or more characters of a build's id, such as " code { "3234d4a4" } ", find that build. "
            "A category also has aliases, other names people use: " code { "connect4" } " finds the "
            "four-in-a-row detector, " code { "inc" } " the incrementers, and " code { "bcd" }
            " both BCD converters. Every word must match, and " code { "-" } " in front of a word excludes it."
        }
    }
}

/// How size is measured, and a stackable build's volume per copy.
fn size_part() -> Markup {
    html! {
        (heading("size", "Size and stacking"))
        p class="mb-3" {
            "Size is measured from the build's box, less any outer layer that holds only its inputs and "
            "outputs. Those stick out to be driven and read, so they do not make the build bigger."
        }
        p class="mb-3" {
            "Stackable builds repeat along a step, such as every 2 blocks up, and their copies may overlap. "
            "Each copy's size is the build's size with the step in place of its length that way, and that is the "
            "volume the site shows, sorts by, and ranks by. A 1-bit adder 4×4×6 that stacks every 2 up "
            "is 48 blocks per copy, and one 2×6×6 that stacks every 5 west is 60. For an adder, the site finds "
            "the step itself, by stacking copies and simulating the wider adder they make."
        }
    }
}

/// Which builds are listed as similar.
fn similar_part() -> Markup {
    html! {
        (heading("similar", "Similar builds"))
        p class="mb-3" {
            "Each build's page lists the builds of its category whose redstone is at least 90% the same. "
            "That is the share of their dust, repeaters, comparators, torches, and other components that "
            "line up, facing the same way, once one build is turned, mirrored, and shifted to fit the other "
            "best. Filler blocks do not count. An upload that close to a stored build gets a warning, with a "
            "link to upload it as a new version of that build instead."
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
                    "The site runs builds of these categories in a redstone simulator, through every case "
                    "of their inputs, or every case of each byte when there are too many. A build that gets "
                    "every case right is marked as passing, and the facts below come from the simulation or "
                    "its file rather than from what was typed, each with a check beside it on the build's page."
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
    div class="overflow-x-auto" {
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
                "An upload is run quickly through every test its inputs and outputs fit, and the one "
                "it gets most right, at least three cases in four, chooses its category to start with. "
                "An ALU is tried only when its control lines' signs name operations, as any build with "
                "a control line might otherwise pass for one."
            }
            dt class="font-semibold" { "Read from the file" }
            dd {
                "Width, orientation, and encoding come from input A of a build that follows the "
                "building conventions; an adder's and a subtractor's carry or borrow flags from "
                "whether it has the lone lines for them."
            }
            dt class="font-semibold" { "From the simulation" }
            dd {
                "Measured once every case comes out right: latency and synchronized for every "
                "category, a monostable's pulse, and the flags the cases show, such as whether a "
                "comparator is signed or which edge a monostable pulses on."
            }
            dt class="font-semibold" { "Stacking" }
            dd {
                "Found by the simulation: an adder's copies are stacked the way its bits are spaced, "
                "or for a 1-bit adder at the shortest step that adds right, and it stacks any number "
                "of times or not at all, whatever was typed. Searched for: "
                "copies are stacked west at each step up to the build's width, or two past it for a "
                "monostable, and the shortest that works is kept with the most copies that do, up to "
                "16, in place of the builder's step west. Given: the builder says how it stacks, and "
                "saving checks only that two copies fit."
            }
        }
        (heading("not-simulated", "Not simulated yet"))
        p class="mb-3" {
            (unchecked.join(", ")) ", and any category off the list. Their facts are what was typed."
        }
        (heading("limits", "What the simulator cannot run"))
        p class="mb-3" {
            "The simulator models dust, torches, repeaters, comparators, levers, stone buttons, lamps, "
            "and the barrels, furnaces, and hoppers a comparator reads. A build with anything else, "
            "such as a piston, an observer, or a wooden button, is not simulated rather than simulated wrongly, nor is "
            "one wider than 32 bits; a build it can't run, of a category whose every detail it works out, as an adder's, goes under a category of its own. It runs a tick at a time, without the order in which the game "
            "updates blocks, so a build that relies on that order may behave differently in game."
        }
    }
    }
}
