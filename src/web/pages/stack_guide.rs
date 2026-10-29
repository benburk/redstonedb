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
        section id="stacking" aria-labelledby="stacking-heading" class="mt-10 max-w-prose" {
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
    let label = "text-muted tabular-nums";
    html! {
        dl class="grid grid-cols-[auto_minmax(0,1fr)] items-baseline gap-x-4 gap-y-1.5" {
            @if review.first_copy.is_empty() {
                dt class=(label) { "1  Paste" }
                dd { "the build" }
                dt class=(label) { "2  Stack" }
                dd { code { (command) } " (" (copies) " copies)" }
            } @else {
                dt class=(label) { "1  Paste" }
                dd { a href=(download("without")) { "without first-copy parts" } }
                dt class=(label) { "2  Stack" }
                dd { code { (command) } " (" (copies) " copies)" }
                dt class=(label) { "3  Paste" }
                dd {
                    a href=(download("first")) { "first-copy parts" } ", same spot: "
                    code { "//gmask air" } " " code { "//paste -a" } " " code { "//gmask" }
                }
                dt class=(label) { "First copy" }
                dd class="text-muted" { (parts(&review.first_copy, direction)) }
            }
        }
    }
}

/// What only the first copy keeps, in words, such as `its east layer,
/// with 8 levers`: the layers on the face the stack starts from.
fn parts(first_copy: &FirstCopy, direction: StackDirection) -> String {
    let face = match direction.name().as_str() {
        "west" => "east",
        "up" => "bottom",
        "north" => "south",
        _ => "start",
    };
    let layers = match first_copy.layers {
        1 => format!("its {face} layer"),
        n => format!("its {n} {face} layers"),
    };
    match first_copy.switches {
        0 => layers,
        1 => format!("{layers}, with 1 lever"),
        n => format!("{layers}, with {n} levers"),
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
                    ", where the next copy, " (way(review.direction)) ", covers a different block. "
                    "Logic only the first copy needs goes on gray blocks."
                }
                ul class="mt-2 flex list-disc flex-col gap-0.5 pl-5 text-meta" {
                    @for spot in spots.iter().take(LISTED) {
                        @let [x, y, z] = spot.pos;
                        @let [fx, fy, fz] = spot.from;
                        li {
                            "(" (x) ", " (y) ", " (z) "): " (spot.earlier) " replaced by the next copy's " (spot.later)
                            ", from (" (fx) ", " (fy) ", " (fz) ")"
                            @if let Some([ox, oy, oz]) = spot.on {
                                " on (" (ox) ", " (oy) ", " (oz) "). If only the first copy needs it, make ("
                                (ox) ", " (oy) ", " (oz) ") gray."
                            } @else {
                                "."
                            }
                        }
                    }
                    @if spots.len() > LISTED {
                        li class="text-muted" { "and " (spots.len() - LISTED) " more" }
                    }
                }
            }
        }
        @if let Some(better) = review.better {
            p class="rounded-control border border-line p-3" {
                strong { "Step: " }
                "copies " (better) " " (way(review.direction)) " apart agree wherever they overlap."
            }
        }
        @if let Some(shape) = &review.shape {
            p class="rounded-control border border-line p-3" { strong { "Shape: " } (shape) }
        }
    }
}
