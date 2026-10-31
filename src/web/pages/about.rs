//! The About page, and its guide to how builds are sorted: categories,
//! designs, measures, facts, flags, tags, search, size, and similar builds.

use maud::{Markup, html};

use crate::{
    category::Stacking,
    guide,
    models::{catalogue::Category, user::User},
    web::guide_image,
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
                    a href="/about/uploading" { "Before you upload" }
                    ": what a build needs and what the site does with it."
                }
                p class="mb-4" {
                    a href="/about/categories" { "How builds are sorted" }
                    ": categories, designs, measures, tags, search, and leaderboards."
                }
                p class="mb-4" {
                    a href="/about/simulation" { "What the site checks" }
                    ": the categories it recognizes, and what it reads from the file and measures."
                }
            }
        },
    )
}

/// What an upload needs and what the site does with it: the terms a build
/// is taken on, read before uploading, each rule pictured by the examples
/// in [`crate::guide`].
pub fn before_upload(viewer: Option<&User>) -> Markup {
    base(
        viewer,
        "Before you upload",
        "/about/uploading",
        &html! {
            div class="max-w-calm leading-prose" {
                h1 class={ (PAGE_TITLE) " mb-4" } { "Before you upload" }
                h2 class={ (SECTION_TITLE) " mt-8 mb-2" } { "What a build needs" }
                (rule(
                    "Inputs",
                    "a lever on the side of a solid block, or a stone button for a pulse. \
                     The block drives at most two dusts, on its sides, besides torches and repeaters, and no other redstone touches it, though a target or redstone block may sit on top. \
                     A repeater must fit where the lever is, so a bus can drive it.",
                    &["input", "input-three-dusts", "repeater"],
                ))
                (rule(
                    "Outputs",
                    "the dust or repeater carrying each one out sits on a coloured block, any colour but white, gray, \
                     or an input's lime, light blue, cyan, blue, or magenta, that no block of its colour touches.",
                    &["output", "outputs-touching"],
                ))
                (rule(
                    "Buses",
                    "each in a line, evenly spaced: straight, or rising one block per block across. \
                     A bus may also be two such lines, its bits alternating.",
                    &["bus-straight", "bus-diagonal", "bus-alternating", "bus-uneven"],
                ))
                (rule(
                    "Stacking",
                    "copies repeat a box of the build; what only the first copy needs, such as a carry in, goes on an outer face.",
                    &[],
                ))
                (rule(
                    "Counters",
                    "input A as wide as the value, one button, the clock, and one lever, the write enable. \
                     The clock counts up or down; with write enable on, it loads A.",
                    &[],
                ))
                p class="mt-6 mb-4 max-w-prose" { "No other colours, signs, or bit-order marks are needed." }
                div class="max-w-prose" {
                    h2 class={ (SECTION_TITLE) " mt-8 mb-2" } { "What the site does" }
                    ol class="mb-4 flex list-decimal flex-col gap-1.5 pl-5" {
                        li { "Finds the inputs and outputs, and tests the build as each category it recognizes: adders, registers, ALUs, decoders, multipliers, counters, and shift registers so far." }
                        li { "Once recognized, works out each line's role, colours the inputs and outputs by it, standardizes the blocks, and takes out unused ones." }
                        li { "Measures latency, throughput, and how copies stack." }
                        li { "Shows it in each viewer's block theme." }
                    }
                    p class="mb-4" {
                        "A build it can't recognize goes up as built, off the list, with what's wrong listed. "
                        "A fixed new version moves it up."
                    }
                }
            }
        },
    )
}

/// One rule a build keeps: its name, what it says, and pictures of the
/// examples `names`, each captioned with whether it passes.
fn rule(name: &str, text: &str, names: &[&str]) -> Markup {
    let examples = guide::examples();
    html! {
        p class="mt-6 mb-3 max-w-prose" { strong { (name) ": " } (text) }
        div class="grid grid-cols-2 gap-x-4 gap-y-5 sm:grid-cols-3" {
            @for example in names.iter().filter_map(|&n| examples.iter().find(|e| e.name == n)) {
                figure class="m-0 flex flex-col gap-1.5" {
                    @if let Some(src) = guide_image(example.name) {
                        img class="aspect-[8/5] w-full" src=(src) width="480" height="300"
                            loading="lazy" alt=(example.caption);
                    }
                    figcaption class="text-meta text-muted" {
                        strong { @if example.kept { "Passes. " } @else { "Fails. " } }
                        (example.caption)
                    }
                }
            }
        }
    }
}

/// How builds are sorted, with the standard categories under their
/// sections.
pub fn sorting(viewer: Option<&User>, categories: &[Category]) -> Markup {
    base(
        viewer,
        "How builds are sorted",
        "/about/categories",
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
            "Only adders, registers, ALUs, decoders, multipliers, counters, and shift registers are on the list so far: a build is one when probing "
            "recognizes it. Any other names its own category, such as 3x3 piston door, and goes up as "
            "built, off the list, under Other, until its category is recognized too."
        }
        dl class="mb-3 grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1.5" {
            @for (key, title, members) in sections(categories) {
                @let members: Vec<_> = members.into_iter().filter(|c| crate::category::BLESSED.contains(&c.name.as_str())).collect();
                @if key != "other" && !members.is_empty() {
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
            "How a build works, such as an adder's ripple carry or carry lookahead, worked out from its "
            "circuit, never chosen. A category's designs do the same job, so leaderboards rank them "
            "together unless you pick one."
        }
    }
}

/// The numbers builds are named and ranked by.
fn measure_part() -> Markup {
    html! {
        (heading("measures", "Measures"))
        p class="mb-3" {
            "The number a category's builds are named and ranked by: width in bits, as 8-bit Carry "
            "Lookahead Adder."
        }
        p class="mb-3" {
            "Builds compete only at equal widths. Leaderboards also split by stateless or stateful, and "
            "orientation."
        }
        p class="mb-3" {
            "A build whose copies stack into a wider one also competes at 8, 16, and 32 bits: its copies "
            "are stacked to each width, tested, and timed, and take the box they fill, each copy adding its step."
        }
    }
}

/// The facts a build has, measured or typed.
fn fact_part() -> Markup {
    html! {
        (heading("facts", "Facts about a build"))
        dl class="mb-3 grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2" {
            dt class="font-semibold" { "Orientation" }
            dd { "Horizontal, vertical, or diagonal: the way its bits run. Read from input A, or a 1-bit build's stacking." }
            dt class="font-semibold" { "Stateless" }
            dd {
                "Settled outputs depend only on inputs, as an adder's. One that remembers, as a register, is "
                "stateful. Found by the simulation."
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
            "A recognized build runs through its inputs; once every case is right, every fact above comes "
            "from its file and the simulation. A build off the list has what its builder gives. "
            a href="/about/simulation" { "What the site checks" } " says more."
        }
    }
}

/// Flags, the yes-or-no facts of one category.
fn flag_part() -> Markup {
    html! {
        (heading("flags", "Flags"))
        p class="mb-3" {
            "Yes-or-no facts of one category, most adding truth-table columns: an ALU's operations beyond "
            "add and subtract, such as AND, XOR, and shift left; an adder's carry in and carry out."
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
            "a release it works in (" code { "version:1.12" } "), or a credited "
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
            "a 4×4×6 1-bit adder stacking every 2 up is 48; a 2×6×6 one stacking every 5 west is 60. A "
            "recognized build's step the site finds itself, by simulating stacked copies."
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

/// What the site works out for itself: a row for each blessed category,
/// then what the simulator cannot run.
pub fn simulation_guide(viewer: Option<&User>, checked: &[Checked]) -> Markup {
    base(
        viewer,
        "What the site checks",
        "/about/simulation",
        &html! {
            div class="max-w-prose leading-prose" {
                h1 class={ (PAGE_TITLE) " mb-4" } { "What the site checks" }
                p class="mb-4" {
                    "An upload is probed: its levers and buttons are its inputs, its outputs the coloured blocks "
                    "with dust or a repeater on them, and the site finds what each does by trying them. One "
                    "recognized as a category below is simulated through every input case, or every case of "
                    "each byte when there are too many, and takes the facts below from the simulation or its "
                    "file, each checked on its page. Every other build goes up as built, off the list, under a "
                    "name of its own, with its facts as its builder gives them."
                }
            }
            (checked_table(checked))
            (simulation_notes())
        },
    )
}

/// A row for each blessed category: what is read from its file and its
/// simulation, and how its stacking is decided.
fn checked_table(checked: &[Checked]) -> Markup {
    let stacking = |stacking: Stacking| match stacking {
        Stacking::Inferred => "Found by the simulation",
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

/// What each column of the table means, and what the simulator cannot
/// run.
fn simulation_notes() -> Markup {
    html! {
    div class="max-w-prose leading-prose" {
        (heading("columns", "What each column means"))
        dl class="mb-3 grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2" {
            dt class="font-semibold" { "Read from the file" }
            dd {
                "Width and orientation come from input A, as probing finds it; an adder's carry flags "
                "from whether it has a carry in and a carry out."
            }
            dt class="font-semibold" { "From the simulation" }
            dd {
                "Once every case is right: latency, synchronized, throughput, whether it holds state, and "
                "flags the cases show, such as an ALU's operations."
            }
            dt class="font-semibold" { "Stacking" }
            dd {
                "Found by the simulation: copies stack as " code { "//rstack" } " stacks them, by the box "
                "the build's later copies repeat, the fewest blocks left out first, and the result, "
                "unlimited copies or none, is the build's. Never stacked: copies make no wider build."
            }
        }
        (heading("limits", "What the simulator cannot run"))
        p class="mb-3" {
            "The simulator models dust, torches, repeaters, comparators, levers, stone buttons, lamps, "
            "and the barrels, furnaces, and hoppers a comparator reads. A build with anything else, such "
            "as a piston, observer, or wooden button, or wider than 32 bits, isn't recognized, and goes "
            "up off the list. It steps a tick at a time, ignoring the game's block update order, so "
            "builds relying on that order may differ in game."
        }
    }
    }
}
