//! The upload page: choosing a file, with how to get one from Open
//! Redstone and how to mark a build so the site can read it; and the file
//! fields every page that takes a file shares.

use maud::{Markup, html};

use crate::models::user::User;

use super::{Connection, PAGE_TITLE, TABLE_CELL, TABLE_HEAD, base, schemati};

/// The fields for choosing a file, the same wherever one is chosen: the
/// file or a Schemati link. `form` names the form they belong to when they
/// stand outside it.
pub(super) fn file_fields(form: Option<&str>, schemati: &Connection) -> Markup {
    html! {
        label class="field" {
            span { "Schematic file" }
            input type="file" name="file" accept=".schem" form=[form] class="input";
        }
        label class="field" {
            span { "Or a Schemati link" }
            input type="url" name="schemati" inputmode="url" autocomplete="off" form=[form]
                placeholder="https://schemati.openredstone.org/download?file=…"
                aria-describedby="schemati-help" class="input";
            span id="schemati-help" class="text-meta font-normal" {
                "Run " code { "//download <name>" } " in game to get one. It works for seven days."
            }
        }
        (schemati::own_file_field(schemati, form))
    }
}

/// The upload page's first step: choosing a file.
pub fn upload(user: &User, message: &str, connection: &Connection) -> Markup {
    base(
        Some(user),
        "Upload",
        "/upload",
        &html! {
            div class="max-w-calm" {
                h1 class={ (PAGE_TITLE) " mb-6" } { "Upload a build" }

                form action="/upload/preview" method="post" enctype="multipart/form-data"
                    class="mb-10 flex flex-col gap-4 rounded-overlay border border-line bg-raised p-5 shadow-sm" {
                    (file_fields(None, connection))
                    @if !message.is_empty() {
                        p role="alert" class="text-danger" { (message) }
                    }
                    div { button type="submit" class="btn btn-primary" { "Preview" } }
                }
                p class="mb-6 max-w-prose text-muted" {
                    "The site tries to recognize it. A build it recognizes goes up standardized, with every detail measured; "
                    "any other keeps its own blocks, and you describe it."
                }
                details class="mb-4 max-w-prose" {
                    summary class="cursor-pointer font-semibold" { "Getting a file from Open Redstone" }
                    div class="mt-3" { (schemati_guide()) }
                }
                details class="max-w-prose" {
                    summary class="cursor-pointer font-semibold" { "Marking a build so it's recognized" }
                    div class="mt-3" { (marking_guide()) }
                }
            }
        },
    )
}

/// How to get a Schemati link for a build on the Open Redstone Engineers
/// server.
fn schemati_guide() -> Markup {
    html! {
        section class="leading-prose" {
            ol class="flex list-decimal flex-col gap-1.5 pl-5" {
                li { "Select the build with " code { "//wand" } ", then run " code { "//copy" } "." }
                li { "Run " code { "//save <name>" } " to keep it in Schemati." }
                li {
                    "Pick it from your Schemati above. Without Schemati connected, run "
                    code { "//download <name>" } " and paste the link instead."
                }
            }
        }
    }
}

/// One role in the marking guide: its colours, as swatch and name, and
/// what goes with them.
struct Marking {
    role: &'static str,
    colours: &'static [(&'static str, &'static str)],
    block: &'static str,
    rule: &'static str,
}

/// Each part of a build and how to mark it, with Minecraft's concrete
/// colours for the swatches.
const MARKINGS: [Marking; 7] = [
    Marking {
        role: "Input A",
        colours: &[("bg-[#5ea818]", "lime")],
        block: "concrete",
        rule: "A lever on the south face of each bit.",
    },
    Marking {
        role: "Inputs B, C, D",
        colours: &[
            ("bg-[#2389c6]", "light blue"),
            ("bg-[#157788]", "cyan"),
            ("bg-[#2c2e8f]", "blue"),
        ],
        block: "concrete",
        rule: "A lever on any side of each bit.",
    },
    Marking {
        role: "Control line",
        colours: &[("bg-[#a9309f]", "magenta")],
        block: "concrete",
        rule: "A lever or button, and a sign naming it, such as carry in or reset. The only one on an adder, incrementer, or decrementer needs no sign.",
    },
    Marking {
        role: "Outputs 1, 2, 3",
        colours: &[
            ("bg-[#f0af15]", "yellow"),
            ("bg-[#e06100]", "orange"),
            ("bg-[#603b1f]", "brown"),
        ],
        block: "concrete or stained glass",
        rule: "When they make one value, yellow holds the lowest bits.",
    },
    Marking {
        role: "Status line",
        colours: &[("bg-[#d5658e]", "pink")],
        block: "concrete or stained glass",
        rule: "A sign naming it, such as carry out or overflow. The only one on an adder, incrementer, or decrementer needs no sign.",
    },
    Marking {
        role: "First copy only",
        colours: &[("bg-[#373a3e]", "gray")],
        block: "concrete or stained glass",
        rule: "Parts only the first copy of a stack keeps, such as an input's driver. Kept gray only if the build stacks with them left out of later copies; otherwise turned white.",
    },
    Marking {
        role: "Everything else",
        colours: &[("bg-[#cfd5d6]", "white")],
        block: "concrete or stained glass",
        rule: "Use no other colours or blocks. Redstone parts stay as they are.",
    },
];

/// How a build marks its inputs and outputs so the site can read it, from
/// `conventions.md`, kept to what an upload needs.
fn marking_guide() -> Markup {
    html! {
        section {
            p class="mb-4 max-w-prose text-muted" {
                "The site reads a build's inputs and outputs from its colours and signs."
            }
            div class="relative overflow-x-auto" {
                table class="w-full border-collapse" {
                    thead {
                        tr {
                            th scope="col" class=(TABLE_HEAD) { "Part" }
                            th scope="col" class=(TABLE_HEAD) { "Block" }
                            th scope="col" class=(TABLE_HEAD) { "Rule" }
                        }
                    }
                    tbody {
                        @for marking in &MARKINGS {
                            tr {
                                th scope="row" class={ (TABLE_CELL) " text-left font-semibold whitespace-nowrap" } { (marking.role) }
                                td class=(TABLE_CELL) {
                                    span class="flex flex-wrap items-center gap-x-3 gap-y-1" {
                                        @for (swatch, name) in marking.colours {
                                            span class="flex items-center gap-1.5 whitespace-nowrap" {
                                                span aria-hidden="true" class={ "size-3.5 shrink-0 rounded-[3px] border border-black/15 " (swatch) } {}
                                                (name)
                                            }
                                        }
                                    }
                                    span class="text-meta text-muted" { (marking.block) }
                                }
                                td class=(TABLE_CELL) { (marking.rule) }
                            }
                        }
                    }
                }
            }
            ul class="mt-4 flex max-w-prose list-disc flex-col gap-1.5 pl-5" {
                li { "Each colour is one bus. Its first bit is the one nearest the south-east bottom corner." }
                li {
                    "To start a bus from the other end, put a sign reading " code { "lsb" } " on its first bit. "
                    "The buses running alongside it follow, unless they have their own."
                }
                li { "To name a bus, put a sign on any of its blocks, such as " code { "addend" } "." }
                li { "Place each lever so it touches only its input block." }
                li { "Save every lever off. A lever saved on makes its input inverted." }
                li { "A build with its own layout, such as a display, needs none of this: it keeps its own blocks." }
            }
        }
    }
}
