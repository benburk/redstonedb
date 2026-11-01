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
                    "RSDB is a site for sharing, verifying, and archiving high quality redstone builds. "
                    "It was launched by embizone in October 2026, taking inspiration from and building "
                    "upon many other plot-of-logic projects."
                }
                p class="mb-2" { "Depending on the circuit, RSDB verifies builds by automatically testing for:" }
                ul class="mb-4 flex list-disc flex-col gap-1 pl-5" {
                    li { "build type" }
                    li { "latency" }
                    li { "throughput" }
                    li { "orientation (diagonal, vertical, horizontal)" }
                    li { "stackability" }
                    li { "correctness" }
                    li { "the weakest signal strength its inputs need" }
                    li { "the weakest signal strength its outputs give" }
                    li { "and more" }
                }
                p class="mb-4" {
                    "All of this is recognized mechanically. It can't drift out of date with a description, "
                    "and an uploader can't specify incorrect information."
                }
                p class="mb-4" {
                    "Builds RSDB can't test yet are still archived, as built, under Other."
                }
                p class="mb-4" {
                    a href="/about/recognized" { "How builds are recognized" }
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
                    "Each type of build has an input and output contract. RSDB finds a build's inputs and "
                    "outputs, then searches through the ways they could fit each type's truth table."
                }
                h2 class={ (SECTION_TITLE) " mt-8 mb-2" } { "Inputs and outputs" }
                (rule(
                    "Outputs",
                    "need to be lone coloured blocks. The dust, repeater, or comparator on top is the bit. \
                     Any colour but white, gray, or an input colour, with no other block of that colour touching it.",
                    &["output", "outputs-touching"],
                ))
                (rule(
                    "Data inputs",
                    "must be a lever on the side of a block, with no redstone on top or below. \
                     The block powers at most two dusts, and needs room for a repeater where the lever is, so a bus could drive it.",
                    &["input", "input-three-dusts", "repeater", "data-dust-on-top"],
                ))
                (rule(
                    "Control lines",
                    "like a carry in, can have dust on top.",
                    &["control-dust-on-top"],
                ))
                (rule(
                    "Buses",
                    "are evenly spaced lines, straight or diagonal, or two lines with their bits alternating.",
                    &["bus-straight", "bus-diagonal", "bus-alternating", "bus-uneven"],
                ))
                div class="max-w-prose" {
                    h2 class={ (SECTION_TITLE) " mt-8 mb-2" } { "Stackability" }
                    ul class="mb-4 flex list-disc flex-col gap-1.5 pl-5" {
                        li { "RSDB tries a few different bounding boxes and sees if they " code { "//rstack" } " correctly." }
                        li { "You can keep parts out of the stack, like a carry in, by putting them on an outer face." }
                    }
                    h2 class={ (SECTION_TITLE) " mt-8 mb-2" } { "Per-category rules" }
                    ul class="mb-4 flex list-disc flex-col gap-1.5 pl-5" {
                        li { "Some builds have optional control lines and data inputs, like an adder's carry in or an incrementer's increment line." }
                        li { "Gates take A and B and give one output as wide." }
                        li { "Multipliers take A and B, and at most one calculate button. No reset." }
                        li { "Counters take A, one clock button, and one write enable lever." }
                        li { "Popcounts and binary to hex converters can give their answer as one output's signal strength." }
                    }
                    p class="mb-4" {
                        "Recognized so far: adders, ALUs, multipliers, incrementers, decrementers, comparators, "
                        "carry outs, absolute values, popcounts, thresholds, binary-to-hex converters, gates, "
                        "inverters, decoders, barrel shifters, monostables, registers, buffers, counters, and "
                        "shift registers."
                    }
                    p class="mb-4" {
                        "Anything else still goes up, as built, under Other, with a note saying what's in the way."
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
        p class="mt-6 mb-3 max-w-prose" { strong { (name) " " } (text) }
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
