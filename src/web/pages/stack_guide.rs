//! How to stack a build in game, as the site stacks it: paste it without
//! the parts only its first copy keeps, `//rstack` it, and paste those
//! parts back; and what looks wrong in a stacking its builder gave, rather
//! than the simulation finding it: where two copies disagree, and whether
//! they make a bigger build of the same shape.

use maud::{Markup, html};

use crate::{
    models::schematic::{Schematic, StackDirection},
    stack::{FirstCopy, Review},
};

use super::SECTION_TITLE;

/// The most disagreeing spots listed; the rest are counted.
const LISTED: usize = 8;

/// The section for a stackable build's page; nothing for one that does
/// not stack. `decided` when the simulation found the stacking, so it
/// needs no warnings.
pub fn stacking_section(
    s: &Schematic,
    reviews: &[Review],
    decided: bool,
) -> Markup {
    if reviews.is_empty() {
        return html! {};
    }
    html! {
        section aria-labelledby="stacking-heading" class="mt-10 max-w-prose" {
            h2 id="stacking-heading" class={ (SECTION_TITLE) " mb-3" } { "Stacking in game" }
            @for review in reviews {
                div class="mb-6 flex flex-col gap-2" {
                    @if reviews.len() > 1 {
                        h3 class="font-semibold" { "Stacking " (way(review.direction)) }
                    }
                    (steps(s, review))
                    @if !decided {
                        (warnings(review))
                    }
                }
            }
        }
    }
}

/// A direction as words, such as `west and up`.
fn way(direction: StackDirection) -> String {
    direction.name().replace('-', " and ")
}

/// The `//rstack` command for `copies` copies of a build stacked
/// `direction`: its direction as the `RedstoneTools` plugin takes it, the copies after
/// the first, and the step.
fn rstack(direction: StackDirection, copies: i64) -> String {
    let token = match direction.name().as_str() {
        "west" => "w",
        "up" => "u",
        "north" => "n",
        _ => "wu",
    };
    let step = direction.west.max(direction.up).max(direction.north);
    format!("//rstack {token} {} {step}", copies - 1)
}

/// The steps to stack it one way, with the files they paste.
fn steps(s: &Schematic, review: &Review) -> Markup {
    let direction = review.direction;
    let copies = direction.max_copies.unwrap_or(8);
    let command = rstack(direction, copies);
    let download = |part: &str| {
        format!(
            "/schematics/{}/download?part={part}&direction={}",
            s.id,
            direction.name()
        )
    };
    html! {
        @if review.first_copy.is_empty() {
            p {
                "Paste it and run " code { (command) } " for " (copies)
                " copies, the count being the copies wanted less one."
            }
        } @else {
            ol class="flex list-decimal flex-col gap-1 pl-5" {
                li {
                    "Paste the build without the parts only its first copy keeps: "
                    a href=(download("without")) { "download it" } "."
                }
                li {
                    "Run " code { (command) } " for " (copies)
                    " copies, the count being the copies wanted less one."
                }
                li {
                    "From the same spot, paste those parts back with " code { "//paste -a" } ": "
                    a href=(download("first")) { "download them" } "."
                }
            }
            p class="text-meta text-muted" {
                "Only the first copy keeps " (parts(&review.first_copy)) "."
            }
        }
    }
}

/// What only the first copy keeps, in words, such as `8 data input and
/// output blocks and the input at (3, 1, 5)`.
fn parts(first_copy: &FirstCopy) -> String {
    let mut parts = Vec::new();
    match first_copy.buses.len() {
        0 => {}
        1 => parts.push("a data input or output block, whose bus runs on through the copy before".to_owned()),
        n => parts.push(format!("{n} data input and output blocks, whose buses run on through the copy before")),
    }
    match first_copy.gray {
        0 => {}
        1 => parts.push("a gray block".to_owned()),
        n => parts.push(format!("{n} gray blocks")),
    }
    for [x, y, z] in &first_copy.inputs {
        parts.push(format!(
            "the input at ({x}, {y}, {z}), which the copy before feeds"
        ));
    }
    match parts.as_slice() {
        [] => "nothing".to_owned(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// What looks wrong in stacking it one way.
fn warnings(review: &Review) -> Markup {
    let spots = &review.disagreements;
    html! {
        @if !spots.is_empty() {
            div class="rounded-control border border-line p-3" {
                p {
                    strong { "Copies disagree " }
                    @match spots.len() {
                        1 => "at a spot",
                        n => { "at " (n) " spots" }
                    }
                    ", where the next copy, " (way(review.direction)) ", puts another block over this one's. "
                    "If the step is right, look at these; a barrel replaced, or dust cut, may stop a copy working."
                }
                ul class="mt-2 flex list-disc flex-col gap-0.5 pl-5 text-meta" {
                    @for spot in spots.iter().take(LISTED) {
                        @let [x, y, z] = spot.pos;
                        li { "(" (x) ", " (y) ", " (z) "): " (spot.earlier) " replaced by " (spot.later) }
                    }
                    @if spots.len() > LISTED {
                        li class="text-muted" { "and " (spots.len() - LISTED) " more" }
                    }
                }
            }
        }
        @if let Some(shape) = &review.shape {
            p class="rounded-control border border-line p-3" { strong { "Shape: " } (shape) }
        }
    }
}
