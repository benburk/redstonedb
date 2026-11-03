//! The About pages: what the site is, how it recognizes builds, each rule
//! pictured by the examples in [`crate::recognize::guide`], and the work it
//! is built on.

use maud::{Markup, PreEscaped, html};

use crate::{models::user::User, recognize::guide, web::guide_image};

use super::{PAGE_TITLE, SECTION_TITLE, base, description::player_badge};

/// What the site is and what it verifies.
pub fn about(viewer: Option<&User>) -> Markup {
    base(
        viewer,
        "About",
        "/about",
        &html! {
            div class="max-w-prose leading-prose" {
                h1 class={ (PAGE_TITLE) " mb-4" } { "About" }
                p class="mb-4" {
                    "RedstoneDB is a site for sharing, verifying, and archiving high-quality redstone builds. "
                    "It was launched by embizone in October 2026, taking inspiration from and building on "
                    "many earlier plot-of-logic projects."
                }
                p class="mb-2" { "Depending on the circuit, RedstoneDB tests every build automatically for:" }
                ul class="mb-4 flex list-disc flex-col gap-1 pl-5" {
                    li { "build type: adder, register, ALU, and so on" }
                    li { "correctness, across every input case, or every case of each byte for wide builds" }
                    li { "latency, in ticks" }
                    li { "throughput: how often it can take a new input" }
                    li { "whether its outputs change in sync" }
                    li { "orientation: horizontal, vertical, or diagonal" }
                    li { "stackability, and how it ranks stacked to 8, 16, and 32 bits" }
                    li { "the weakest signal strength its inputs need, and the weakest its outputs give" }
                    li { "the oldest Minecraft version it works in" }
                    li { "and more" }
                }
                p class="mb-4" {
                    "All of this is worked out mechanically, so it can't drift out of date with a "
                    "description, and an uploader can't enter incorrect information. Even an adder's "
                    "design, like ripple carry or carry lookahead, is read from the circuit."
                }
                p class="mb-2" { "A few other things:" }
                ul class="mb-4 flex list-disc flex-col gap-1 pl-5" {
                    li { "Builds RedstoneDB can't verify still go in their category, as built, with the specs their uploader gives: non-standard when their inputs and outputs don't fit, fails verification when they give wrong outputs. They're hidden until you tick Show unverified, and never rank." }
                    li { "Builds of types RedstoneDB can't test yet are still archived, as built, under Other." }
                    li { "Every build shows in your own block theme, and you can send any of them straight to your Schemati." }
                    li { "Builds keep the colours their builder gave them: switch between Original, Default, and your theme on any build's page." }
                    li { "Uploading a new version keeps the old ones, so a build's history stays downloadable." }
                    li { "Tags RedstoneDB finds itself, like an adder's design, its orientation, carry in, or stackable, are teal. Tags people add are other colours. Each category's page filters by its tags." }
                    li { "Untick Public on upload and a build is yours alone. Star any build to keep it under Favourites on your profile." }
                    li { "Write " code { "@name" } " in a description to link a player. They're listed on that player's page." }
                    li { "Search understands shorthand like " code { "8b" } ", " code { "3t" } ", " code { "torchless" } ", and " code { "version:1.12" } "." }
                }
                p class="mb-4" {
                    a href="/about/recognized" { "How builds are recognized" } " covers what a build needs to be tested."
                }
            }
        },
    )
}

/// The open-source work the site runs on, under its licenses: MCHPRS,
/// which simulates every build, and schematic-renderer, which draws the 3D
/// view.
pub fn credits(viewer: Option<&User>) -> Markup {
    base(
        viewer,
        "Credits",
        "/about/credits",
        &html! {
            div class="max-w-prose leading-prose" {
                h1 class={ (PAGE_TITLE) " mb-4" } { "Credits" }
                p class="mb-4" {
                    "RedstoneDB is built by Ben Burk, " (badge("embizone")) " in game, "
                    "and " a href="https://github.com/benburk" { "benburk" } " on GitHub. "
                    "Two open-source projects do the heavy lifting."
                }
                h2 class={ (SECTION_TITLE) " mt-8 mb-2" } {
                    a href="https://github.com/MCHPR/MCHPRS" { "MCHPRS" }
                }
                p class="mb-4" {
                    "Every test, latency, and throughput on this site comes from running the build in "
                    "MCHPRS, the Minecraft High Performance Redstone Server, and its redstone compiler, "
                    "Redpiler, by " (badge("StackDoubleFlow")) " and its contributors. RedstoneDB uses "
                    a href="https://github.com/benburk/MCHPRS" { "a fork" }
                    " of it. MIT License."
                }
                h2 class={ (SECTION_TITLE) " mt-8 mb-2" } {
                    a href="https://github.com/Schem-at/schematic-renderer" { "schematic-renderer" }
                }
                p class="mb-4" {
                    "The 3D view on every build's page is schematic-renderer, by Schem.at, loaded unchanged "
                    "from jsDelivr with its resource pack. GNU AGPL v3; its source is on GitHub."
                }
                p class="mt-8 mb-4 text-meta text-muted" {
                    "RedstoneDB is not an official Minecraft product, and is not approved by or associated with Mojang or Microsoft."
                }
            }
        },
    )
}

/// The player `name` as a badge, as a description's `@name` shows them.
fn badge(name: &str) -> Markup {
    PreEscaped(player_badge(name))
}

/// How builds are recognized: the input and output contract, each rule
/// pictured, then stacking and the rules of single categories.
pub fn recognized(viewer: Option<&User>) -> Markup {
    base(
        viewer,
        "How builds are recognized",
        "/about/recognized",
        &html! {
            div class="max-w-calm leading-prose" {
                h1 class={ (PAGE_TITLE) " mb-4" } { "How builds are recognized" }
                p class="mb-4 max-w-prose" {
                    "Each type of build has an input and output contract. RedstoneDB finds a build's inputs and "
                    "outputs, then searches through the ways they could fit each type's truth table, trying "
                    "lines in different roles and orders until one fits. You don't need any particular "
                    "colours or signs: once RedstoneDB knows what each line does, it keeps that with the build "
                    "and shows each line in its colour, whatever blocks you built with."
                }
                h2 class={ (SECTION_TITLE) " mt-8 mb-2" } { "Inputs and outputs" }
                (rule(
                    "outputs",
                    "Outputs",
                    "are repeaters or comparators that power nothing: the block in front of each is air. \
                     They can stand on any block, of any colour. A repeater gives a bit; a comparator gives a bit, \
                     or a strength from 0 to 15 where the category reads one, like a popcount's count. \
                     A repeater or comparator that powers a block or dust isn't one: that's just wiring. \
                     A sign naming one, like carry out, goes under the air it gives out into, on the block \
                     holding it up. An outer layer of nothing but outputs, the blocks holding them up, and \
                     their signs doesn't add to a build's size, nor one of nothing but its levers and the \
                     blocks they're on.",
                    &["output", "output-powering"],
                ))
                (rule(
                    "data-inputs",
                    "Inputs",
                    "need a lever on the side of a solid block, a lamp too. Dust on top of the block or underneath it is fine. \
                     There has to be room for a repeater where the lever is, so a bus could drive it instead. \
                     An input taking a strength from 0 to 15, a hex digit, is a comparator with air behind it, \
                     giving out into the build: RedstoneDB drives it with a barrel there.",
                    &["input", "repeater", "data-dust-on-top", "input-dust-below", "strength-input"],
                ))
                (rule(
                    "control-lines",
                    "Control lines",
                    "like a carry in or a write enable, can have a block behind the lever, since only a lever or button will ever drive them.",
                    &[],
                ))
                (rule(
                    "buses",
                    "Buses",
                    "need to be evenly spaced lines, straight or diagonal, or two lines with their bits alternating.",
                    &["bus-straight", "bus-diagonal", "bus-alternating", "bus-uneven"],
                ))
                div class="max-w-prose" {
                    h2 class={ (SECTION_TITLE) " mt-8 mb-2" } { "Stackability" }
                    ul class="mb-4 flex list-disc flex-col gap-1.5 pl-5" {
                        li { "RedstoneDB tries a few different bounding boxes and checks whether " code { "//rstack" } "ing them gives a working, wider build." }
                        li { "Copies go west for a horizontal build and up for a vertical one, and must touch or overlap: copies with air between them are separate builds. The one exception is a build 1 block wide that way, which counts as 2 wide: its copies 2 apart, a block of air between each, can stack." }
                        li { "Copies that add bits are tried 1, 2, and 4 blocks a bit apart, the pitch: an 8-bit adder stacking every 16 blocks is pitch 2, as is a 1-bit gate stacking every 2. The tightest that works is its class, as in 2-wide stackable, and it counts as each looser class too." }
                        li { "Adders, incrementers, and other builds whose data crosses bits must chain into one wider build. Gates and monostables must still work copy by copy, without disturbing their neighbours, so a 1-wide one is 2-wide stackable without testing, as copies with air between can't meet. Inverters, registers, and buffers share lines, so the site counts how many copies still work. A register's copies can also add words on its shared bus, at any spacing." }
                        li { "If the whole box doesn't work, it tries smaller ones, leaving out as few blocks as it can." }
                        li { "You can keep parts out of the stack, like a carry in, by putting them on an outer face." }
                        li { "Stackable builds also compete on the leaderboards at 8, 16, and 32 bits." }
                    }
                    h2 id="records" class={ (SECTION_TITLE) " mt-8 mb-2 scroll-mt-4" } { "Records" }
                    ul class="mb-4 flex list-disc flex-col gap-1.5 pl-5" {
                        li { "A board is a category at one width and state, narrowed by up to three of: its latency, and tags RedstoneDB finds, like synced, torchless, alpha, a design, or an orientation." }
                        li { "First place on a board of two or more builds, fastest or smallest, is a record." }
                        li { "Each tag or latency has to change who's first. Most 8-bit adders are 3-tick CCAs, so there's no fastest CCA, but there is a smallest 3-tick adder." }
                        li { "Fastest counts only when speed decides it. When builds tie on ticks, the record is the smallest at that latency." }
                    }
                    h2 id="per-category" class={ (SECTION_TITLE) " mt-8 mb-2 scroll-mt-4" } { "Per-category rules" }
                    ul class="mb-4 flex list-disc flex-col gap-1.5 pl-5" {
                        li { "Some builds have optional control lines and data inputs, like an adder's carry in or an incrementer's increment line." }
                        li { "Gates take A and B and give one output as wide. Each gate is its own category: an XOR gate with a torch on its output is an XNOR gate." }
                        li { "Multipliers take A and B, and at most one calculate button. No reset. A third input can be an addend, for A × B + C." }
                        li { "Counters take A, one clock button, and one write enable lever." }
                        li { "Subtractors are adders that take B away: A − B, less a borrow in. They rank apart from adders." }
                        li { "T flip-flops take one input, a button or a lever, and give one output, which each press toggles." }
                        li { "Hex adders take A and B a hex digit each, a comparator with air behind it, in place of levers. Every other category takes levers." }
                        li { "Thresholds find their own number: how many inputs it takes to turn them on." }
                        li { "Popcounts and binary-to-hex converters can give their answer as one output's signal strength." }
                        li { "Parity circuits give one output, on when an odd number of inputs are on, or an even number, which is flagged Even." }
                    }
                    p class="mb-4" {
                        "Recognized so far: " (crate::category::Kind::listed()) "."
                    }
                    p class="mb-4" {
                        "A build that doesn't fit goes up as built, unverified, with a note on what's in the way. "
                        "Upload a fixed version later and it's verified."
                    }
                }
            }
        },
    )
}

/// One rule a build keeps, at `id`, which a fault's advice links to: its
/// name, what it says, and pictures of the examples `names`, each
/// captioned with whether it passes.
fn rule(id: &str, name: &str, text: &str, names: &[&str]) -> Markup {
    let examples = guide::examples();
    html! {
        p id=(id) class="mt-6 mb-3 max-w-prose scroll-mt-4" { strong { (name) " " } (text) }
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
