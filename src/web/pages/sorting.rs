//! How builds are sorted: the About page's guide to categories, designs,
//! measures, facts, flags, tags, search, size, and similar builds.

use maud::{Markup, html};

use crate::models::{schematic::Category, user::User};

use super::{PAGE_TITLE, SECTION_TITLE, base, sections};

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
            "is no adder, a decrementer no incrementer, and an XNOR gate no XOR gate, though each pair is "
            "built alike."
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
            "also meets only builds of the same logic, encoding, and orientation."
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
            dt class="font-semibold" { "Logic" }
            dd { "Combinational or sequential. Each category has a usual logic, and a build gives its own only when it differs, as a sequential multiplier does." }
            dt class="font-semibold" { "Latency" }
            dd { "Ticks from input A changing until the slowest output settles, leaving out status lines such as carry out." }
            dt class="font-semibold" { "Synchronized" }
            dd { "Whether every output bit changes once, all on the same tick." }
            dt class="font-semibold" { "Throughput" }
            dd { "The fewest ticks from one input to the next that still give right outputs." }
            dt class="font-semibold" { "Pipelined" }
            dd { "A combinational, synchronized build whose throughput is below its latency, so a new input goes in before the last result comes out. The site works this out; nobody types it." }
        }
        p class="mb-3" {
            "Adders, incrementers, decrementers, inc/decs, combinational multipliers, and combinational "
            "converters between binary and BCD are simulated through their inputs. Once every case of one "
            "comes out right, its latency and synchronized come from the simulation, not from what was typed."
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
                "Components the build does without. The site reads torchless from the file. A build gets it "
                "when it has no redstone torch, standing or on a wall, and nobody can add or remove it."
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
            code { "comb" } " or " code { "seq" } ", " code { "sync" } ", " code { "pipelined" } ", "
            code { "throughput:3" } ", " code { "encoding:hex" } ", and the people credited. "
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
