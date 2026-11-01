//! The About pages: what the site is, and how it recognizes builds, each
//! rule pictured by the examples in [`crate::guide`].

use maud::{Markup, html};

use crate::{guide, models::user::User, web::guide_image};

use super::{PAGE_TITLE, SECTION_TITLE, base};

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
                    "colours or signs: once RedstoneDB knows what each line does, it colours them for you."
                }
                h2 class={ (SECTION_TITLE) " mt-8 mb-2" } { "Inputs and outputs" }
                (rule(
                    "outputs",
                    "Outputs",
                    "need to be lone blocks, with dust, a repeater, or a comparator on top to read. \
                     Any block works, wool, concrete, stone, whatever, as long as no other block like it touches it. Glass only counts stained a colour.",
                    &["output", "outputs-touching"],
                ))
                (rule(
                    "data-inputs",
                    "Data inputs",
                    "must be a lever on the side of a solid block, a lamp too, with no redstone on top or below. \
                     There has to be room for a repeater where the lever is, so a bus could drive it instead.",
                    &["input", "repeater", "data-dust-on-top", "input-dust-below"],
                ))
                (rule(
                    "control-lines",
                    "Control lines",
                    "like a carry in or a write enable, can have dust on top, since only a lever or button will ever drive them.",
                    &["control-dust-on-top"],
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
                        li { "If the whole box doesn't work, it tries smaller ones, leaving out as few blocks as it can." }
                        li { "You can keep parts out of the stack, like a carry in, by putting them on an outer face." }
                        li { "Stackable builds also compete on the leaderboards at 8, 16, and 32 bits." }
                    }
                    h2 id="per-category" class={ (SECTION_TITLE) " mt-8 mb-2 scroll-mt-4" } { "Per-category rules" }
                    ul class="mb-4 flex list-disc flex-col gap-1.5 pl-5" {
                        li { "Some builds have optional control lines and data inputs, like an adder's carry in or an incrementer's increment line." }
                        li { "Gates take A and B and give one output as wide. A torch on the output makes the inverse, so XOR and XNOR share a category, with a flag." }
                        li { "Multipliers take A and B, and at most one calculate button. No reset. A third input can be an addend, for A × B + C." }
                        li { "Counters take A, one clock button, and one write enable lever." }
                        li { "Hex adders take A and B a hex digit per block, a signal strength through dust on top, in place of levers. Every other category takes levers." }
                        li { "Thresholds find their own number: how many inputs it takes to turn them on." }
                        li { "Popcounts and binary-to-hex converters can give their answer as one output's signal strength." }
                    }
                    p class="mb-4" {
                        "Recognized so far: adders, hex adders, ALUs, multipliers, incrementers, decrementers, comparators, "
                        "carry outs, absolute values, popcounts, thresholds, binary-to-hex converters, gates, "
                        "inverters, decoders, barrel shifters, monostables, registers, buffers, counters, and "
                        "shift registers."
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
