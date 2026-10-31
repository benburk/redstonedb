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

/// What the site is: tested builds first, an archive second, linking to
/// the rest of the About pages.
pub fn about(viewer: Option<&User>) -> Markup {
    base(
        viewer,
        "What this is",
        "/about",
        &html! {
            div class="max-w-prose leading-prose" {
                h1 class={ (PAGE_TITLE) " mb-4" } { "What this is" }
                p class="mb-4" {
                    "Redstone Database is a library of redstone builds that work. Every adder, register, "
                    "multiplier, and the rest on the list has been run through a simulator, case by case, "
                    "so if a build says it adds in 3 ticks, it does. You can line builds up by speed and "
                    "size, see who built what, and send any of them straight to your Schemati."
                }
                p class="mb-4" {
                    "It's also an archive. Plenty of good redstone doesn't fit a category yet, or breaks "
                    "a rule the tests need. Those builds are kept too, as their builders made them, with "
                    "their names, dates, and credits, so they're still around to learn from."
                }
                p class="mb-4" {
                    "It's a side project for the redstone community, run for fun. Spotted a mistake, or "
                    "built something faster? Upload it."
                }
                ul class="mb-4 flex list-disc flex-col gap-1.5 pl-5" {
                    li { a href="/about/uploading" { "What a build needs" } ": the few rules that let a build be tested." }
                    li { a href="/about/categories" { "Finding builds" } ": categories, search, tags, and leaderboards." }
                    li { a href="/about/simulation" { "How we test" } ": what the simulator checks and measures." }
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
        "What a build needs",
        "/about/uploading",
        &html! {
            div class="max-w-calm leading-prose" {
                h1 class={ (PAGE_TITLE) " mb-4" } { "What a build needs" }
                p class="mb-4 max-w-prose" {
                    "We test a build by flipping its levers and watching its outputs, so we need to find "
                    "both. Here's what that takes. You don't need any particular colours, signs, or labels."
                }
                (rule(
                    "Inputs",
                    "put each lever on the side of a solid block, not the top or bottom, or use a stone button for anything you press. \
                     The block can power up to two dusts on its sides, plus torches and repeaters. Nothing else should touch it, \
                     though a target or redstone block on top is fine. Leave room for a repeater where the lever is, so a bus could drive it instead.",
                    &["input", "input-three-dusts", "repeater"],
                ))
                (rule(
                    "Outputs",
                    "each output bit is the dust, repeater, or comparator sitting on a coloured block. Any colour works except white, gray, \
                     and the input colours, lime, light blue, cyan, blue, and magenta, as long as no other block of that colour touches it.",
                    &["output", "outputs-touching"],
                ))
                (rule(
                    "Buses",
                    "lay each bus out in a line, evenly spaced, either straight or climbing one block for each block across. \
                     Two lines with the bits alternating between them works too.",
                    &["bus-straight", "bus-diagonal", "bus-alternating", "bus-uneven"],
                ))
                (rule(
                    "Stacking",
                    "if copies of your build stack, they should repeat one box of it. Anything only the first copy needs, like a carry in, goes on an outside face.",
                    &[],
                ))
                (rule(
                    "Gates",
                    "two inputs, A and B, and one output as wide. A bus could drive A and B, so keep dust off the top of their blocks.",
                    &[],
                ))
                (rule(
                    "Multipliers",
                    "A and B, and if it works step by step, one calculate button you press once they're set. No reset button.",
                    &[],
                ))
                (rule(
                    "Counters",
                    "A, as wide as the count, one clock button, and one write enable lever. \
                     Pressing the clock counts up or down; pressing it with write enable on loads A.",
                    &[],
                ))
                div class="max-w-prose" {
                    h2 class={ (SECTION_TITLE) " mt-8 mb-2" } { "What happens when you upload" }
                    ol class="mb-4 flex list-decimal flex-col gap-1.5 pl-5" {
                        li { "We find the inputs and outputs, and try the build as each category we know how to test: adders, ALUs, multipliers, incrementers, decrementers, comparators, carry outs, absolute values, popcounts, thresholds, binary-to-hex converters, gates, inverters, decoders, barrel shifters, monostables, registers, buffers, counters, and shift registers." }
                        li { "If it's one of them, we colour each line by what it does, tidy up the blocks, and take out any it doesn't use." }
                        li { "Then we measure how fast it is, how often it can take a new input, and whether copies stack." }
                        li { "Everyone sees it in their own block theme." }
                    }
                    p class="mb-4" {
                        "If we can't recognize it, it still goes up, as you built it, under Other, with a note saying "
                        "what got in the way. Upload a fixed version later and it moves onto the list."
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
        "Finding builds",
        "/about/categories",
        &html! {
            div class="max-w-prose leading-prose" {
                h1 class={ (PAGE_TITLE) " mb-4" } { "Finding builds" }
                p class="mb-4" {
                    "Every build has a category, a design, a number it's named and ranked by, and a handful "
                    "of facts. Here's what each one means, and how to search by them."
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
            "A category is one job: everything in it gives the same answers, give or take an inverted "
            "line, like a torch on every bit of a bus. Inversions become flags rather than new "
            "categories, so a subtractor is an adder flagged Subtract. That's why one category covers "
            "adders with or without a carry in and carry out, XOR and XNOR gates, signed and unsigned "
            "comparators, monostables that pulse on either edge, and registers that read their cells "
            "either way round."
        }
        p class="mb-3" {
            "These are the categories we can test so far. Anything else picks a name of its own, like "
            "3x3 piston door, and lives under Other, kept as built, until we learn to test it."
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
            "How a build does its job, such as ripple carry or carry lookahead for an adder. We work it "
            "out from the circuit, so nobody picks it. Every design in a category does the same job, so "
            "leaderboards rank them together unless you choose one."
        }
    }
}

/// The numbers builds are named and ranked by.
fn measure_part() -> Markup {
    html! {
        (heading("measures", "Measures"))
        p class="mb-3" {
            "The number a build is named and ranked by, usually its width in bits, as in 8-bit Carry "
            "Lookahead Adder."
        }
        p class="mb-3" {
            "Builds only compete with others of the same width. Leaderboards also split stateless from "
            "stateful, and by orientation."
        }
        p class="mb-3" {
            "If copies of a build stack into a wider one, it competes at 8, 16, and 32 bits as well. We "
            "stack it to each width, test and time it there, and count the space its copies fill."
        }
    }
}

/// The facts a build has, measured or typed.
fn fact_part() -> Markup {
    html! {
        (heading("facts", "Facts about a build"))
        dl class="mb-3 grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2" {
            dt class="font-semibold" { "Orientation" }
            dd { "Which way its bits run: horizontal, vertical, or diagonal. We read it from input A, or for a 1-bit build, from how it stacks." }
            dt class="font-semibold" { "Stateless" }
            dd {
                "Its outputs depend only on its inputs, like an adder's. A build that remembers things, like "
                "a register, is stateful. The simulation tells which."
            }
            dt class="font-semibold" { "Latency" }
            dd { "Ticks from input A changing until the slowest output settles, not counting status lines like carry out. For a register, it's data in to data out through a write and a read, so a loop's latency is its ALU's plus its registers'." }
            dt class="font-semibold" { "Synchronized" }
            dd { "Every output bit changes once, all on the same tick." }
            dt class="font-semibold" { "Throughput" }
            dd { "How few ticks you can leave between inputs and still get the right answers." }
            dt class="font-semibold" { "Works from" }
            dd {
                "The oldest Java Edition release with every redstone part it uses, such as 1.5 for a "
                "comparator or 1.16 for a target block. Plain building blocks don't count, but dust on a "
                "transparent block does, since that only works from 1.2.1. Everything works in the latest "
                "release."
            }
            dt class="font-semibold" { "Pipelined" }
            dd { "Stateless and synchronized, with a throughput shorter than its latency, so a new input goes in before the last answer comes out. We work this out; nobody types it." }
        }
        p class="mb-3" {
            "Once a recognized build gets every case right, all of these come from its file and the "
            "simulation. Builds under Other show what their builders told us. "
            a href="/about/simulation" { "How we test" } " has the details."
        }
    }
}

/// Flags, the yes-or-no facts of one category.
fn flag_part() -> Markup {
    html! {
        (heading("flags", "Flags"))
        p class="mb-3" {
            "Yes-or-no facts within one category, mostly extra things a build can do: an ALU's operations "
            "beyond add and subtract, like AND, XOR, and shift left, or an adder's carry in and carry out."
        }
        p class="mb-3" {
            "Flags don't split categories or leaderboards. They show on the build's page, and you can "
            "search for them."
        }
    }
}

/// Tags, by kind.
fn tag_part() -> Markup {
    html! {
        (heading("tags", "Tags"))
        p class="mb-3" {
            "Optional labels that work across categories, so a missing tag doesn't mean anything. The "
            "colour tells you the kind:"
        }
        dl class="mb-3 grid grid-cols-[auto_minmax(0,1fr)] items-center gap-x-4 gap-y-2" {
            dt { span class="badge badge-restriction" { "Restriction" } }
            dd {
                "Parts the build manages without, read from its file, so nobody adds or removes them by "
                "hand. Torchless means no redstone torches at all. Alpha means it would work in Alpha: only "
                "dust, torches, levers, buttons, and pressure plates, none of them on a transparent block."
            }
            dt { span class="badge badge-other" { "Other" } }
            dd { "Anything else worth searching for, like instant." }
            dt { span class="badge badge-failing" { "Simulation" } }
            dd {
                "Fails simulation: the build got some cases wrong. Nobody adds or removes this one either; "
                "a fixed new version clears it."
            }
        }
    }
}

/// What search matches.
fn search_part() -> Markup {
    html! {
        (heading("search", "Search"))
        p class="mb-3" {
            "Each word you type has to match something: a category, design, flag, tag, orientation, "
            "width (" code { "8b" } "), latency (" code { "3t" } "), " code { "stateless" } " or "
            code { "stateful" } ", " code { "sync" } ", " code { "pipelined" } ", " code { "throughput:3" }
            ", a release it works in (" code { "version:1.12" } "), or someone credited. Put "
            code { "-" } " in front of a word to leave those out. Six or more characters of a build's id, "
            "like " code { "3234d4a4" } ", find that build. Nicknames work too: " code { "connect4" }
            " finds the four-in-a-row detector, " code { "inc" } " the incrementers, and " code { "bcd" }
            " both BCD converters."
        }
    }
}

/// How size is measured, and a stackable build's volume per copy.
fn size_part() -> Markup {
    html! {
        (heading("size", "Size and stacking"))
        p class="mb-3" {
            "Size is the build's box, minus any outer layer that only holds inputs and outputs."
        }
        p class="mb-3" {
            "Builds that stack repeat every few blocks, and their copies can overlap. For those, we show "
            "and rank the volume of one copy, using the step in place of the length: a 4×4×6 1-bit adder "
            "that stacks every 2 up counts as 48, and a 2×6×6 one that stacks every 5 west counts as 60. "
            "For a recognized build, we find the step ourselves by simulating stacked copies."
        }
    }
}

/// Which builds are listed as similar.
fn similar_part() -> Markup {
    html! {
        (heading("similar", "Similar builds"))
        p class="mb-3" {
            "A build's page lists other builds in its category with at least 90% of the same redstone: "
            "dust, repeaters, comparators, torches, and the rest in the same places, facing the same way, "
            "after the best turn, mirror, and shift. Filler blocks don't count. Upload something that "
            "close to an existing build and we'll suggest adding it as a new version instead."
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
        "How we test",
        "/about/simulation",
        &html! {
            div class="max-w-prose leading-prose" {
                h1 class={ (PAGE_TITLE) " mb-4" } { "How we test" }
                p class="mb-4" {
                    "When you upload a build, we find its inputs, its levers and buttons, and its outputs, "
                    "the dust, repeaters, or comparators on coloured blocks, then try them to see what the "
                    "build does. If it matches one of the categories below, we run it through every input "
                    "case, or every case of each byte when there are too many, and take its facts from "
                    "that. Everything else goes up as built, under a name of its own, with whatever facts "
                    "its builder gives."
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
        Stacking::Inferred => "Found by testing",
        Stacking::Given => "Set by the builder",
        Stacking::Never => "Doesn't stack",
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
                "Width and orientation come from input A. An adder's carry flags come from whether it has a "
                "carry in and a carry out."
            }
            dt class="font-semibold" { "From the simulation" }
            dd {
                "Once every case is right: latency, whether it's synchronized, throughput, whether it holds "
                "state, and any flags its answers show, like an ALU's operations."
            }
            dt class="font-semibold" { "Stacking" }
            dd {
                "Found by testing: we stack copies the way " code { "//rstack" } " does, by the box its later "
                "copies repeat, leaving out as few blocks as we can, and whatever works, any number of "
                "copies or none, is what the build gets. Doesn't stack: copies don't make a wider build."
            }
        }
        (heading("limits", "What the simulator can't run"))
        p class="mb-3" {
            "The simulator knows dust, torches, repeaters, comparators, levers, stone buttons, lamps, and "
            "the barrels, furnaces, and hoppers a comparator can read. Anything with other parts, like a "
            "piston, observer, or wooden button, or anything wider than 32 bits, can't be tested and goes "
            "under Other. It also steps one tick at a time and ignores the game's block update order, so a "
            "build that depends on that order might behave differently in game."
        }
    }
    }
}
