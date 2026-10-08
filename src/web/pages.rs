//! Full page templates.
//!
//! These return complete HTML documents wrapped in the base layout.
//! For fixi partial updates, use components directly.

use maud::{Markup, PreEscaped, html};

use crate::{
    APP_NAME,
    interface::{Bit, Interface},
    lint::Violation,
    models::{
        schematic::{self, Design, Function, Schematic, StackDirection, Tag},
        user::{Role, User},
    },
    search::Term,
};

use super::{components::base, details};

const PAGE_TITLE: &str = "text-page font-bold";
const SECTION_TITLE: &str = "text-section font-semibold";
const TABLE_HEAD: &str = "border-b border-line py-2 pr-4 text-left text-meta font-semibold text-muted";
const TABLE_CELL: &str = "border-b border-line py-2.5 pr-4 align-top";

/// How the home page lays out its schematics.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    Cards,
    List,
}

/// The home page's schematics and how they were asked for.
pub struct Listing<'a> {
    /// The schematics matching the search.
    pub shown: &'a [Schematic],
    /// How many schematics there are in all.
    pub total: usize,
    pub view: View,
    pub query: &'a str,
    pub terms: &'a [Term],
}

pub fn schematics(viewer: Option<&User>, listing: &Listing) -> Markup {
    let Listing {
        shown,
        total,
        view,
        query,
        terms,
    } = *listing;
    let switch = "rounded-control px-3 py-1 text-muted hover:bg-selection hover:text-ink hover:no-underline aria-[current=page]:bg-raised aria-[current=page]:font-semibold aria-[current=page]:text-ink aria-[current=page]:shadow-sm";
    let builds = if total == 1 { "build" } else { "builds" };
    let count = if shown.len() == total {
        format!("{total} {builds}")
    } else {
        format!("{} of {total} {builds}", shown.len())
    };
    base(
        viewer,
        APP_NAME,
        "/",
        &html! {
            div class="mb-4 flex flex-wrap items-center gap-x-4 gap-y-2" {
                h1 class=(PAGE_TITLE) { "Schematics" }
                p class="mr-auto self-end pb-1 text-muted tabular-nums" { (count) }
                nav aria-label="Layout" class="flex gap-1 rounded-control bg-sidebar p-1" {
                    a href=(home(View::Cards, query)) class=(switch) aria-current=[(view == View::Cards).then_some("page")] { "Cards" }
                    a href=(home(View::List, query)) class=(switch) aria-current=[(view == View::List).then_some("page")] { "List" }
                }
            }
            form action="/" method="get" role="search" class="mb-6 flex flex-col gap-2" {
                @if view == View::List {
                    input type="hidden" name="view" value="list";
                }
                div class="flex gap-2" {
                    label for="search" class="sr-only" { "Search schematics" }
                    input type="search" id="search" name="q" value=(query)
                        placeholder="register vertical alpha -torchless"
                        aria-describedby="search-help"
                        class="input flex-1";
                    button type="submit" class="btn btn-primary" { "Search" }
                }
                p id="search-help" class="text-meta text-muted" {
                    "Every word must match: a function, orientation, tag, width such as 8b, builder, "
                    code { "stackable" } ", or part of the name. Put " code { "-" } " or "
                    code { "not" } " before a word to leave those builds out, and quotes around a tag with spaces."
                }
                @if !terms.is_empty() {
                    ul class="flex flex-wrap items-center gap-1.5" aria-label="Search terms" {
                        @for (i, term) in terms.iter().enumerate() {
                            li {
                                a href=(home(view, &without(terms, i)))
                                    aria-label={ "Remove " (term.display()) }
                                    class={ "badge gap-1.5 pr-2 hover:no-underline "
                                        (if term.negated { "bg-danger/10 text-danger" } else { "bg-selection text-ink" }) } {
                                    (if term.negated { "not " } else { "" }) (term.text)
                                    span aria-hidden="true" { "×" }
                                }
                            }
                        }
                        li {
                            a href=(home(view, "")) class="px-1.5 text-meta" { "Clear" }
                        }
                    }
                }
            }
            @if total == 0 {
                p class="text-muted" { "No schematics yet. Import one with " code { "just rsdb add" } "." }
            } @else if shown.is_empty() {
                p class="text-muted" { "No builds match. Remove a term to widen the search." }
            } @else if view == View::Cards {
                (cards(shown))
            } @else {
                (table(shown))
            }
        },
    )
}

/// The home page's address for `view` and `query`.
fn home(view: View, query: &str) -> String {
    let mut params = form_urlencoded::Serializer::new(String::new());
    if view == View::List {
        params.append_pair("view", "list");
    }
    if !query.is_empty() {
        params.append_pair("q", query);
    }
    let params = params.finish();
    if params.is_empty() {
        "/".to_owned()
    } else {
        format!("/?{params}")
    }
}

/// The query with term `index` taken out.
fn without(terms: &[Term], index: usize) -> String {
    terms
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != index)
        .map(|(_, term)| term.display())
        .collect::<Vec<_>>()
        .join(" ")
}

fn cards(schematics: &[Schematic]) -> Markup {
    html! {
        ul class="grid grid-cols-[repeat(auto-fill,minmax(16rem,1fr))] gap-5" {
            @for s in schematics {
                li {
                    a href={ "/schematics/" (s.id) }
                        class="group flex h-full flex-col overflow-hidden rounded-overlay border border-line bg-raised text-ink shadow-sm transition-colors duration-150 ease-calm hover:border-boundary hover:no-underline" {
                        img src={ "/schematics/" (s.id) "/thumbnail.png?v=" (&s.sha256[..12.min(s.sha256.len())]) }
                            alt="" loading="lazy" width="640" height="480"
                            class="aspect-[4/3] w-full border-b border-line bg-sidebar object-contain";
                        div class="flex flex-1 flex-col gap-2 p-4" {
                            h2 class="text-section font-semibold group-hover:text-brand" { (s.name()) }
                            @if !s.tags.is_empty() {
                                ul class="flex flex-wrap gap-1.5" aria-label="Tags" {
                                    @for tag in &s.tags {
                                        li { (badge(tag)) }
                                    }
                                }
                            }
                            p class="mt-auto pt-1 text-meta text-muted tabular-nums" { (facts(s)) }
                        }
                    }
                }
            }
        }
    }
}

/// The few facts a card has room for, such as `8 ticks · 19×11×18 ·
/// 3762 blocks · embizone`.
fn facts(s: &Schematic) -> String {
    [
        s.latency.map(|t| format!("{t} ticks")),
        Some(size(s)),
        Some(volume(s)),
        (!s.builders.is_empty()).then(|| s.builders.join(", ")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ")
}

fn table(schematics: &[Schematic]) -> Markup {
    // Columns a phone has no room for; the name and its tags come first.
    let secondary = "hidden desktop:table-cell";
    html! {
                div class="overflow-x-auto" {
                    table class="w-full border-collapse" {
                        thead {
                            tr {
                                th scope="col" class=(TABLE_HEAD) { "Name" }
                                th scope="col" class={ (TABLE_HEAD) " " (secondary) } { "Orientation" }
                                th scope="col" class={ (TABLE_HEAD) " " (secondary) } { "Encoding" }
                                th scope="col" class=(TABLE_HEAD) { "Latency" }
                                th scope="col" class={ (TABLE_HEAD) " " (secondary) } { "Size" }
                                th scope="col" class={ (TABLE_HEAD) " " (secondary) } { "Volume" }
                                th scope="col" class=(TABLE_HEAD) { "Builders" }
                                th scope="col" class={ (TABLE_HEAD) " " (secondary) } { "Built" }
                            }
                        }
                        tbody {
                            @for s in schematics {
                                tr class="hover:bg-selection/50" {
                                    th scope="row" class={ (TABLE_CELL) " text-left font-normal" } {
                                        a href={ "/schematics/" (s.id) } class="font-semibold" { (s.name()) }
                                        @if !s.tags.is_empty() {
                                            ul class="mt-1.5 flex flex-wrap gap-1.5" aria-label="Tags" {
                                                @for tag in &s.tags {
                                                    li { (badge(tag)) }
                                                }
                                            }
                                        }
                                    }
                                    td class={ (TABLE_CELL) " " (secondary) } { (s.orientation_label()) }
                                    td class={ (TABLE_CELL) " " (secondary) } { (s.encoding_label()) }
                                    td class={ (TABLE_CELL) " tabular-nums" } { (latency(s)) }
                                    td class={ (TABLE_CELL) " tabular-nums " (secondary) } { (size(s)) }
                                    td class={ (TABLE_CELL) " tabular-nums " (secondary) } { (volume(s)) }
                                    td class=(TABLE_CELL) { (s.builders.join(", ")) }
                                    td class={ (TABLE_CELL) " tabular-nums whitespace-nowrap " (secondary) } {
                                        (s.built_on.as_deref().unwrap_or(""))
                                    }
                                }
                            }
                        }
                    }
                }
    }
}

/// What the stored file says about a build.
pub struct Analysis {
    pub interface: Interface,
    pub violations: Vec<Violation>,
}

/// One schematic's page, with an edit button for a mod.
pub fn schematic(
    viewer: Option<&User>,
    s: &Schematic,
    analysis: &Result<Analysis, String>,
) -> Markup {
    let name = s.name();
    let path = format!("/schematics/{}", s.id);
    base(
        viewer,
        &name,
        &path,
        &html! {
            div class="mb-6 flex flex-wrap items-start justify-between gap-4" {
                h1 class=(PAGE_TITLE) { (name) }
                @if viewer.is_some_and(User::can_edit) {
                    a href={ (path) "/edit" } class="btn btn-quiet" { "Edit" }
                }
            }
            (beside_viewer(&format!("{path}/download"), &schematic_info(s)))
            @match analysis {
                _ if !s.conventional => {
                    p class="mt-10 max-w-prose text-muted" {
                        "This build follows its own layout rather than the building conventions, so its inputs and outputs are not read from the file and it is not linted."
                    }
                }
                Ok(analysis) => (interface_section(analysis)),
                Err(err) => {
                    p class="mt-10 text-muted" { "Could not read the file's inputs and outputs: " (err) }
                }
            }
        },
    )
}

fn interface_section(analysis: &Analysis) -> Markup {
    let Interface { inputs, outputs } = &analysis.interface;
    html! {
        section aria-labelledby="interface-heading" class="mt-12" {
            h2 id="interface-heading" class={ (SECTION_TITLE) " mb-1" } { "Inputs and outputs" }
            p class="mb-4 text-meta text-muted" {
                "Read from the file's levers, yellow output blocks, and signs."
            }
            div class="overflow-x-auto" {
                table class="w-full max-w-calm border-collapse" {
                    caption class="sr-only" { "Inputs" }
                    thead {
                        tr {
                            th scope="col" class=(TABLE_HEAD) { "Input" }
                            th scope="col" class=(TABLE_HEAD) { "Bits" }
                            th scope="col" class=(TABLE_HEAD) { "Strength" }
                            th scope="col" class=(TABLE_HEAD) { "Drive" }
                        }
                    }
                    tbody {
                        @for port in inputs {
                            tr {
                                th scope="row" class={ (TABLE_CELL) " text-left font-semibold" } { (port.name) }
                                td class={ (TABLE_CELL) " tabular-nums" } { (port.bits.len()) }
                                td class={ (TABLE_CELL) " tabular-nums" } { (port.strength) " or more" }
                                td class=(TABLE_CELL) { (uniform(&port.bits, |b| (if b.hard { "hard" } else { "soft" }).to_owned())) }
                            }
                        }
                    }
                }
            }
            div class="mt-8 overflow-x-auto" {
                table class="w-full max-w-calm border-collapse" {
                    caption class="sr-only" { "Outputs" }
                    thead {
                        tr {
                            th scope="col" class=(TABLE_HEAD) { "Output" }
                            th scope="col" class=(TABLE_HEAD) { "Bits" }
                        }
                    }
                    tbody {
                        @for port in outputs {
                            tr {
                                th scope="row" class={ (TABLE_CELL) " text-left font-semibold" } { (port.name) }
                                td class={ (TABLE_CELL) " tabular-nums" } { (port.bits.len()) }
                            }
                        }
                    }
                }
            }
        }
        @if !analysis.violations.is_empty() {
            section aria-labelledby="lint-heading" class="mt-12 max-w-calm" {
                h2 id="lint-heading" class={ (SECTION_TITLE) " mb-3" } { "Breaks the building conventions" }
                ul class="flex flex-col gap-1.5" {
                    @for violation in &analysis.violations {
                        li class="flex gap-3" {
                            span class="w-28 shrink-0 text-muted tabular-nums" {
                                @if let Some([x, y, z]) = violation.pos { (x) ", " (y) ", " (z) }
                            }
                            span { (violation.message) }
                        }
                    }
                }
            }
        }
    }
}

/// One value shared by every bit, or `mixed`.
fn uniform(bits: &[Bit], value: impl Fn(&Bit) -> String) -> String {
    let mut values = bits.iter().map(value);
    let first = values.next().unwrap_or_default();
    if values.all(|v| v == first) {
        first
    } else {
        "mixed".to_owned()
    }
}

/// The choices for downloading a build: copies of a stackable one, and
/// glass corner markers for `WorldEdit`.
fn download_dialog(s: &Schematic) -> Markup {
    let path = format!("/schematics/{}", s.id);
    html! {
        dialog id="download" aria-labelledby="download-heading"
            class="m-auto w-full max-w-form rounded-overlay border border-line bg-raised p-5 text-ink shadow-overlay backdrop:bg-black/30" {
            form action={ (path) "/download" } method="get" class="flex flex-col gap-4" {
                h2 id="download-heading" class=(SECTION_TITLE) { "Download" }
                @if !s.stacking.is_empty() {
                    div class="flex gap-2" {
                        @for direction in &s.stacking {
                            label class="field flex-1" {
                                span { "Copies " (direction.name().replace('-', " and ")) }
                                input type="number" name=(direction.name()) min="1"
                                    max=[direction.max_copies] value="1" required
                                    class="input tabular-nums";
                            }
                        }
                    }
                }
                label class="flex items-start gap-2" {
                    input type="checkbox" name="markers" value="yes" class="mt-1 size-4 appearance-auto";
                    span { "Add a glass block off each corner, for WorldEdit to select the build by" }
                }
                div class="flex items-center gap-4" {
                    button type="submit" class="btn btn-primary" { "Download" }
                    button type="button" commandfor="download" command="close" class="btn btn-quiet" { "Cancel" }
                }
            }
        }
    }
}

/// A schematic's tags, download, and details, beside its viewer.
fn schematic_info(s: &Schematic) -> Markup {
    let file = s.filename();
    html! {
        div class="flex flex-col gap-8" {
            section aria-labelledby="tags-heading" {
                h2 id="tags-heading" class={ (SECTION_TITLE) " mb-3" } { "Tags" }
                @if s.tags.is_empty() {
                    p class="text-muted" { "No tags yet." }
                } @else {
                    ul class="flex flex-wrap gap-1.5" {
                        @for tag in &s.tags {
                            li { (badge(tag)) }
                        }
                    }
                }
            }
            section aria-labelledby="file-heading" class="flex flex-col gap-2" {
                h2 id="file-heading" class="sr-only" { "File" }
                button type="button" commandfor="download" command="show-modal" class="btn btn-primary" { "Download" }
                p class="text-meta break-all text-muted" { code { (file) } }
                (download_dialog(s))
            }
            section aria-labelledby="details-heading" {
                h2 id="details-heading" class={ (SECTION_TITLE) " mb-3" } { "Details" }
                dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-6 gap-y-2" {
                    (field("Function", &s.function_label()))
                    (field("Width", &s.width_label().unwrap_or_else(|| "unknown".to_owned())))
                    (field("Orientation", s.orientation_label()))
                    (field("Encoding", s.encoding_label()))
                    (field("Latency", &s.latency.map_or_else(|| "unknown".to_owned(), |t| format!("{t} ticks"))))
                    (field("Synchronized", s.synchronized_label()))
                    (field("Stacks", &stacking(s)))
                    (field("Size", &size(s)))
                    (field("Volume", &volume(s)))
                    (field(&plural("Builder", &s.builders), &if s.builders.is_empty() { "unknown".to_owned() } else { s.builders.join(", ") }))
                    @if !s.contributors.is_empty() {
                        (field(&plural("Contributor", &s.contributors), &s.contributors.join(", ")))
                    }
                    (field("Built", s.built_on.as_deref().unwrap_or("unknown")))
                    (field("Imported by", s.importer().unwrap_or("unknown")))
                    (field("Conventions", if s.conventional { "followed" } else { "not followed" }))
                }
                @if let Some(description) = &s.description {
                    p class="mt-4 leading-prose whitespace-pre-line" { (description) }
                }
            }
        }
    }
}

/// What the edit page shows.
pub struct EditSchematic<'a> {
    pub s: &'a Schematic,
    /// The values sent, when a save was refused; else the build's own.
    pub sent: Option<&'a details::Form>,
    pub choices: &'a Choices,
    /// Every tag, to suggest.
    pub tags: &'a [Tag],
    pub message: &'a str,
}

/// A mod's page for changing a build's details and tags: the form on the
/// left, the build on the right, as when uploading.
pub fn edit_schematic(user: &User, e: &EditSchematic) -> Markup {
    let s = e.s;
    let path = format!("/schematics/{}", s.id);
    let number = |n: Option<i64>| n.map(|n| n.to_string()).unwrap_or_default();
    let (width, latency) = (number(s.width), number(s.latency));
    let own_tags: Vec<String> =
        s.tags.iter().map(|tag| tag.name.clone()).collect();
    let (values, chosen_tags) = match e.sent {
        Some(form) => (form.fields(), form.tags.as_slice()),
        None => (
            Fields {
                function: &s.function,
                width: &width,
                orientation: &s.orientation,
                encoding: &s.encoding,
                latency: &latency,
                synchronized: s.synchronized,
                builders: &s.builders,
                contributors: &s.contributors,
                built_on: s.built_on.as_deref().unwrap_or(""),
                description: s.description.as_deref().unwrap_or(""),
                design: s.design.as_deref().unwrap_or(""),
                carry_in: s.carry_in == Some(true),
                carry_out: s.carry_out == Some(true),
            },
            own_tags.as_slice(),
        ),
    };
    base(
        Some(user),
        &format!("Edit {}", s.name()),
        &path,
        &html! {
            h1 class={ (PAGE_TITLE) " mb-6" } { "Edit " (s.name()) }
            (beside_viewer(&format!("{path}/download"), &html! {
                form action={ (path) "/edit" } method="post" class="flex flex-col gap-4" {
                    (fields(&values, e.choices))
                    (tag_picker(e.tags, chosen_tags))
                    @if !e.message.is_empty() {
                        p role="alert" class="text-danger" { (e.message) }
                    }
                    div class="flex items-center gap-4" {
                        button type="submit" class="btn btn-primary" { "Save" }
                        a href=(path) { "Cancel" }
                    }
                }
            }))
            script defer src="/form.js" {}
        },
    )
}

/// The values a details form starts with.
pub struct Fields<'a> {
    pub function: &'a str,
    pub width: &'a str,
    pub orientation: &'a str,
    pub encoding: &'a str,
    pub latency: &'a str,
    /// `None` when unknown, which shows the toggle as neither on nor off.
    pub synchronized: Option<bool>,
    pub builders: &'a [String],
    pub contributors: &'a [String],
    pub built_on: &'a str,
    pub description: &'a str,
    pub design: &'a str,
    pub carry_in: bool,
    pub carry_out: bool,
}

/// The lists a details form chooses from or suggests.
pub struct Choices {
    /// The standard functions.
    pub functions: Vec<Function>,
    pub designs: Vec<Design>,
    /// Everyone credited on a stored build, to suggest as builders and
    /// contributors.
    pub people: Vec<String>,
}

impl Choices {
    pub async fn load(db: &sqlx::SqlitePool) -> Result<Self, sqlx::Error> {
        Ok(Choices {
            functions: schematic::functions(db).await?,
            designs: schematic::designs(db).await?,
            people: schematic::all_people(db).await?,
        })
    }
}

/// The function, its name when other, its design, and its carry lines.
fn function_fields(f: &Fields, choices: &Choices) -> Markup {
    html! {
        // A function off the standard list shows as other, with its name.
        @let standard = f.function.is_empty()
            || choices.functions.iter().any(|function| function.name == f.function);
        @let carries = choices
            .functions
            .iter()
            .any(|function| function.name == f.function && function.carries);
        @let has_designs = choices
            .designs
            .iter()
            .any(|design| design.function == f.function);
        label class="field" {
            span { "Function" }
            select name="function" required class="input" data-function {
                @if f.function.is_empty() {
                    option value="" selected { "Choose a function" }
                }
                @for function in &choices.functions {
                    option value=(function.name) selected[f.function == function.name]
                        data-carries[function.carries] {
                        (function.label)
                    }
                }
                option value="other" selected[!standard] { "Other" }
            }
        }
        label class="field" data-function-name {
            span { "Function name, if other" }
            input name="function_name" value=(if standard { "" } else { f.function })
                placeholder="such as line_drawer" pattern="[A-Za-z][A-Za-z0-9_ ]*"
                title="Words of letters and digits, such as line drawer"
                class="input";
        }
        // Shown once the chosen function has designs to choose from.
        label class="field" data-design-field hidden[!has_designs] {
            span { "Design" }
            select name="design" class="input" data-design {
                option value="other" selected[f.design.is_empty() || f.design == "other"] { "Other" }
                @for function in &choices.functions {
                    @let designs: Vec<&Design> = choices.designs.iter().filter(|d| d.function == function.name).collect();
                    @if !designs.is_empty() {
                        optgroup label=(function.label) data-for=(function.name) {
                            @for design in designs {
                                option value=(design.name) selected[f.design == design.name] { (design.label()) }
                            }
                        }
                    }
                }
            }
        }
        div class="flex flex-wrap items-center gap-x-6 gap-y-2" data-carries-field
            hidden[!carries && !f.function.is_empty()] {
            label class="flex items-center gap-2" {
                input type="checkbox" name="carry_in" value="yes" checked[f.carry_in] class="size-4 appearance-auto";
                "Has a carry in"
            }
            label class="flex items-center gap-2" {
                input type="checkbox" name="carry_out" value="yes" checked[f.carry_out] class="size-4 appearance-auto";
                "Has a carry out"
            }
            span class="text-meta text-muted" { "Read from the file's signs." }
        }
    }
}

/// The inputs for a build's details, shared by editing and uploading.
fn fields(f: &Fields, choices: &Choices) -> Markup {
    html! {
        (function_fields(f, choices))
        label class="field" {
            span { "Width in bits" }
            input name="width" type="number" min="1" value=(f.width) class="input tabular-nums";
        }
        label class="field" {
            span { "Orientation" }
            select name="orientation" class="input" {
                @for (value, label) in [("horz", "Horizontal"), ("vert", "Vertical"), ("diag", "Diagonal")] {
                    option value=(value) selected[f.orientation == value] { (label) }
                }
            }
        }
        label class="field" {
            span { "Encoding" }
            select name="encoding" class="input" {
                @for (value, label) in [("bin", "binary"), ("hex", "hex")] {
                    option value=(value) selected[f.encoding == value] { (label) }
                }
            }
        }
        label class="field" {
            span { "Latency in ticks" }
            input name="latency" type="number" min="0" value=(f.latency) class="input tabular-nums";
        }
        div class="field" {
            span { "Synchronized" }
            // An unknown build's toggle stays unknown until it is clicked;
            // form.js shows it as neither on nor off and drops the marker.
            label class="flex items-center gap-2" {
                input type="checkbox" name="synchronized" value="yes"
                    checked[f.synchronized == Some(true)]
                    data-unknown[f.synchronized.is_none()]
                    class="size-4 appearance-auto";
                "Every output bit settles on the same tick"
            }
            @if f.synchronized.is_none() {
                input type="hidden" name="synchronized_unknown" value="yes";
            }
        }
        (person_field("Builders", "builder", "Add a builder's in-game name", f.builders, choices))
        (person_field("Contributors", "contributor", "Add someone who helped", f.contributors, choices))
        div class="field" {
            label for="built-on" class="text-meta font-semibold text-muted" { "Built on" }
            div class="flex items-center gap-4" data-date-field {
                input id="built-on" name="built_on" type="date" value=(f.built_on) class="input flex-1";
                label class="flex items-center gap-2 whitespace-nowrap" {
                    input type="checkbox" name="built_on_unknown" value="yes"
                        checked[f.built_on.is_empty()] class="size-4 appearance-auto";
                    "Don't know"
                }
            }
        }
        label class="field" {
            span { "Description" }
            textarea name="description" rows="3" class="input py-2 leading-prose" { (f.description) }
        }
    }
}

/// One choice in a pill field.
struct Pill {
    label: String,
    value: String,
    /// The badge colour, such as `badge-restriction`.
    class: String,
}

fn tag_pill(tag: &Tag) -> Pill {
    Pill {
        label: tag.label(),
        value: tag.name.clone(),
        class: format!("badge-{}", tag.category),
    }
}

/// A pill field of in-game names, suggesting everyone already credited.
fn person_field(
    label: &'static str,
    name: &'static str,
    placeholder: &'static str,
    chosen: &[String],
    choices: &Choices,
) -> Markup {
    pill_field(&PillField {
        label,
        name,
        placeholder,
        suggestions: choices
            .people
            .iter()
            .map(|name| person_pill(name))
            .collect(),
        chosen: chosen.iter().map(|name| person_pill(name)).collect(),
        new_class: "badge-builder",
        pattern: "^[A-Za-z0-9_]{1,16}$",
        hint: "is not a Minecraft name: letters, digits, and underscores, up to 16",
        keep_case: true,
    })
}

/// `Builder` for one name, else `Builders`.
fn plural(singular: &str, names: &[String]) -> String {
    if names.len() == 1 {
        singular.to_owned()
    } else {
        format!("{singular}s")
    }
}

fn person_pill(name: &str) -> Pill {
    Pill {
        label: name.to_owned(),
        value: name.to_owned(),
        class: "badge-builder".to_owned(),
    }
}

/// A field whose values show as pills inside one box, with the text cursor
/// after them and suggestions as you type. `form.js` turns a picked or
/// typed value into a pill; without it, the typed text is sent as
/// `<name>_entry`.
struct PillField<'a> {
    label: &'a str,
    /// The name each chosen value is sent under.
    name: &'a str,
    placeholder: &'a str,
    suggestions: Vec<Pill>,
    chosen: Vec<Pill>,
    /// The colour of a pill for a value not among the suggestions.
    new_class: &'a str,
    /// What a new value must match; the server checks it too.
    pattern: &'a str,
    /// Said after a typed value that does not match.
    hint: &'a str,
    /// Whether a new value keeps its capitals, as in-game names do; else
    /// it is lowercased, as tag names are.
    keep_case: bool,
}

fn pill_field(f: &PillField) -> Markup {
    let entry = format!("{}-entry", f.name);
    let list = format!("{}-suggestions", f.name);
    let message = format!("{}-message", f.name);
    html! {
        div class="flex flex-col gap-1.5" data-pill-field
            data-name=(f.name) data-new-class=(f.new_class)
            data-pattern=(f.pattern) data-hint=(f.hint) data-keep-case[f.keep_case] {
            label for=(entry) class="text-meta font-semibold text-muted" { (f.label) }
            div class="flex min-h-10 flex-wrap items-center gap-1.5 rounded-control border border-boundary bg-raised px-2 py-1.5 focus-within:border-brand focus-within:ring-1 focus-within:ring-brand" {
                ul class="contents" aria-label={ "Chosen: " (f.label) } data-chosen {
                    @for chosen in &f.chosen {
                        (pill(chosen, f.name))
                    }
                }
                input id=(entry) name={ (f.name) "_entry" } list=(list) autocomplete="off"
                    placeholder=(f.placeholder) aria-describedby=(message)
                    class="min-w-40 flex-1 bg-transparent px-1 py-0.5 text-ink outline-none placeholder:text-muted";
            }
            datalist id=(list) {
                @for suggestion in &f.suggestions {
                    option value=(suggestion.label) data-value=(suggestion.value) data-class=(suggestion.class) {}
                }
            }
            p id=(message) role="alert" class="message" data-message {}
            // A pill, cloned by form.js for each value added.
            template data-template {
                (pill(&Pill { label: String::new(), value: String::new(), class: String::new() }, f.name))
            }
        }
    }
}

/// A chosen value: a pill whose remove button shows on hover or focus, and
/// on touch screens always, with the hidden input that sends it.
fn pill(p: &Pill, name: &str) -> Markup {
    html! {
        li class="group" {
            span class={ "badge pr-1 " (p.class) } {
                span data-label { (p.label) }
                button type="button" data-remove
                    aria-label={ "Remove " (if p.label.is_empty() { "this" } else { &p.label }) }
                    class="flex size-4.5 cursor-pointer items-center justify-center rounded-full opacity-0 group-hover:opacity-70 hover:bg-current/15 hover:opacity-100 focus-visible:opacity-100 pointer-coarse:opacity-70" {
                    (cross())
                }
            }
            input type="hidden" name=(name) value=(p.value);
        }
    }
}

/// The tag field of the upload form.
fn tag_picker(tags: &[Tag], chosen: &[String]) -> Markup {
    pill_field(&PillField {
        label: "Tags",
        name: "tag",
        placeholder: "Add a tag, such as torchless",
        suggestions: tags.iter().map(tag_pill).collect(),
        chosen: tags
            .iter()
            .filter(|t| chosen.contains(&t.name))
            .map(tag_pill)
            .chain(
                // New tags the person named, not yet stored.
                chosen
                    .iter()
                    .filter(|name| tags.iter().all(|t| &t.name != *name))
                    .map(|name| Pill {
                        label: name.replace('_', " "),
                        value: name.clone(),
                        class: "badge-other".to_owned(),
                    }),
            )
            .collect(),
        new_class: "badge-other",
        pattern: "^[A-Za-z0-9 _]+$",
        hint: "cannot be a tag: use letters, digits, and spaces",
        keep_case: false,
    })
}

/// The × on a removable badge.
fn cross() -> Markup {
    html! {
        svg aria-hidden="true" viewBox="0 0 12 12" class="size-2.5" {
            path d="M3 3l6 6M9 3l-6 6" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" {}
        }
    }
}

fn badge(tag: &Tag) -> Markup {
    html! {
        span class={ "badge badge-" (tag.category) } title=(category_label(&tag.category)) {
            (tag.label())
        }
    }
}

fn category_label(category: &str) -> &str {
    match category {
        "restriction" => "Restriction",
        "timing" => "Timing",
        _ => "Other",
    }
}

fn field(name: &str, value: &str) -> Markup {
    html! {
        dt class="text-muted" { (name) }
        dd { (value) }
    }
}

/// Each direction a build stacks along, or `no`.
fn stacking(s: &Schematic) -> String {
    if s.stacking.is_empty() {
        return "no".to_owned();
    }
    s.stacking
        .iter()
        .map(StackDirection::describe)
        .collect::<Vec<_>>()
        .join("; ")
}

fn latency(s: &Schematic) -> String {
    s.latency.map(|t| format!("{t} ticks")).unwrap_or_default()
}

fn volume(s: &Schematic) -> String {
    format!("{} blocks", s.volume())
}

fn size(s: &Schematic) -> String {
    let (x, y, z) = s.size;
    format!("{x}×{y}×{z}")
}

/// The layout of a build's page and of an upload's preview: `side`, its
/// facts or form, on the left, and the 3D view of the file at `file` on
/// the right, staying in sight while the side scrolls. On a phone the view
/// comes first.
fn beside_viewer(file: &str, side: &Markup) -> Markup {
    html! {
        div class="grid items-start gap-8 desktop:grid-cols-[var(--container-form)_minmax(0,1fr)]" {
            div class="desktop:sticky desktop:top-4 desktop:order-last" {
                (viewer_3d(file))
            }
            div class="flex flex-col gap-8" { (side) }
        }
    }
}

/// A 3D view of the schematic at `file`, an address the browser can fetch,
/// drawn by schematic-renderer with the vanilla resource pack it publishes
/// alongside its own build.
fn viewer_3d(file: &str) -> Markup {
    html! {
        script type="importmap" { (PreEscaped(VIEWER_IMPORTS)) }
        // The renderer sizes its canvas to the parent, so the parent's height
        // must be fixed or the two grow each other without bound.
        div class="relative h-[60vh] overflow-hidden rounded-overlay border border-line bg-sidebar desktop:h-[calc(100vh-12rem)]" {
            canvas id="viewer" data-src=(file) aria-label="3D view of the schematic"
                class="absolute inset-0 size-full" {}
            p id="viewer-status" class="absolute inset-0 flex items-center justify-center text-muted" {
                "Loading viewer…"
            }
        }
        script type="module" { (PreEscaped(VIEWER_SCRIPT)) }
    }
}

const VIEWER_IMPORTS: &str = r#"{ "imports": {
  "three": "https://cdn.jsdelivr.net/npm/three@0.184.0/build/three.module.js",
  "nucleation": "https://cdn.jsdelivr.net/npm/nucleation@0.2.18/nucleation.js"
} }"#;

const VIEWER_SCRIPT: &str = r#"
const CDN = "https://cdn.jsdelivr.net/npm";
const { SchematicRenderer } = await import(`${CDN}/schematic-renderer@1.6.1/dist/schematic-renderer.es.js`);
const canvas = document.getElementById("viewer");
const status = document.getElementById("viewer-status");
const say = (text) => { status.textContent = text; };
new SchematicRenderer(canvas, {}, {
  vanilla: () => fetch(`${CDN}/schematic-renderer@1.6.1/dist/pack.zip`).then((r) => r.blob()),
}, {
  enableDragAndDrop: false,
  callbacks: {
    onRendererInitialized: async (renderer) => {
      try {
        await renderer.schematicManager.loadSchematicFromURL(canvas.dataset.src, "schematic");
        renderer.cameraManager.focusOnSchematics();
        status.hidden = true;
      } catch (error) {
        say(`Could not load the schematic: ${error.message}`);
      }
    },
  },
});
"#;

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
                h2 class={ (SECTION_TITLE) " mt-8 mb-3" } { "Tags" }
                p class="mb-3" { "A tag's colour gives its kind." }
                dl class="grid grid-cols-[auto_minmax(0,1fr)] items-center gap-x-4 gap-y-2" {
                    dt { span class="badge badge-restriction" { "Restriction" } }
                    dd { "Components the build does without." }
                    dt { span class="badge badge-timing" { "Timing" } }
                    dd { "How the build is clocked, when it differs from what the function implies." }
                    dt { span class="badge badge-other" { "Other" } }
                    dd { "Anything else worth searching by." }
                }
            }
        },
    )
}

/// The login page. `enabled` is false when the Discord settings are
/// missing; `message` explains a failed login.
pub fn login(enabled: bool, message: &str) -> Markup {
    base(
        None,
        "Log in",
        "/sessions/new",
        &html! {
            section class="mx-auto mt-8 flex max-w-sm flex-col gap-5 rounded-overlay border border-line bg-raised p-6 shadow-sm" {
                h1 class=(PAGE_TITLE) { "Log in" }
                p class="text-muted" {
                    "Log in with your Discord account to upload builds. Your account is made the first time you log in."
                }
                @if !message.is_empty() {
                    p role="alert" class="text-danger" { (message) }
                }
                @if enabled {
                    a href="/auth/discord" class="btn btn-primary" { "Log in with Discord" }
                } @else {
                    p role="alert" class="text-danger" {
                        "Discord login is not set up on this server: DISCORD_CLIENT_ID, DISCORD_CLIENT_SECRET, and DISCORD_REDIRECT_URI must be set."
                    }
                }
            }
        },
    )
}

/// The admin's list of accounts, each with a form to set its role.
pub fn users(admin: &User, users: &[User]) -> Markup {
    base(
        Some(admin),
        "Users",
        "/users",
        &html! {
            h1 class={ (PAGE_TITLE) " mb-2" } { "Users" }
            p class="mb-6 max-w-prose text-muted" {
                "A user uploads builds. A mod also edits any build's details and tags. An admin also sets roles."
            }
            div class="max-w-calm overflow-x-auto" {
                table class="w-full border-collapse" {
                    thead {
                        tr {
                            th scope="col" class=(TABLE_HEAD) { "Account" }
                            th scope="col" class=(TABLE_HEAD) { "Role" }
                        }
                    }
                    tbody {
                        @for user in users {
                            tr {
                                th scope="row" class={ (TABLE_CELL) " text-left font-normal" } {
                                    (account_link(user))
                                }
                                td class=(TABLE_CELL) {
                                    @if user.id == admin.id {
                                        span class="text-muted" { "admin (you)" }
                                    } @else {
                                        form action={ "/users/" (user.username) "/role" } method="post" class="flex gap-2" {
                                            label for={ "role-" (user.username) } class="sr-only" {
                                                "Role for " (user.name())
                                            }
                                            select id={ "role-" (user.username) } name="role" class="input min-h-9 w-auto" {
                                                @for role in Role::ALL {
                                                    option value=(role.as_str()) selected[role == user.role] { (role.as_str()) }
                                                }
                                            }
                                            button type="submit" class="btn btn-quiet" { "Set" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        },
    )
}

/// An account's avatar and name, linking to its page.
fn account_link(user: &User) -> Markup {
    html! {
        a href={ "/users/" (user.username) } class="inline-flex items-center gap-2" {
            @if let Some(avatar) = user.avatar_url() {
                img src=(avatar) alt="" width="24" height="24" class="size-6 rounded-full";
            }
            (user.name())
        }
        @if user.display_name.is_some() {
            span class="ml-2 text-meta text-muted" { "@" (user.username) }
        }
    }
}

/// An account's page: who they are and what they uploaded.
pub fn profile(
    viewer: Option<&User>,
    account: &User,
    uploads: &[Schematic],
) -> Markup {
    let path = format!("/users/{}", account.username);
    base(
        viewer,
        account.name(),
        &path,
        &html! {
            div class="mb-8 flex items-center gap-4" {
                @if let Some(avatar) = account.avatar_url() {
                    img src=(avatar) alt="" width="64" height="64" class="size-16 rounded-full";
                }
                div {
                    h1 class=(PAGE_TITLE) { (account.name()) }
                    p class="text-muted" {
                        "@" (account.username) " · " (account.role.as_str())
                    }
                }
            }
            h2 class={ (SECTION_TITLE) " mb-3" } { "Uploads" }
            @if uploads.is_empty() {
                p class="text-muted" { "No uploads yet." }
            } @else {
                (cards(uploads))
            }
        },
    )
}

/// The upload page's first step: choosing a file.
pub fn upload(user: &User, message: &str) -> Markup {
    base(
        Some(user),
        "Upload",
        "/upload",
        &html! {
            div class="max-w-calm" {
                h1 class={ (PAGE_TITLE) " mb-2" } { "Upload a build" }
                p class="mb-6 max-w-prose text-muted" {
                    "Upload a Sponge " code { ".schem" } " file, yours or someone else's. "
                    "Any glass corner markers are taken out and the box is fitted tightly around the build. "
                    "Next you will see what the site reads from it and fill in its details."
                }
                form action="/upload/preview" method="post" enctype="multipart/form-data"
                    class="flex flex-col gap-4 rounded-overlay border border-line bg-raised p-5 shadow-sm" {
                    label class="field" {
                        span { "Schematic file" }
                        input type="file" name="file" accept=".schem" required class="input py-1.5";
                    }
                    label class="flex items-start gap-2" {
                        input type="checkbox" name="standardize" value="yes" checked class="mt-1 size-4 appearance-auto";
                        span {
                            "Standardize it: turn every block but redstone parts, inputs, and outputs white, "
                            "and slabs and glass into white stained glass. "
                            "Inputs must already be levers on lime, light blue, or magenta blocks, and outputs yellow."
                        }
                    }
                    @if !message.is_empty() {
                        p role="alert" class="text-danger" { (message) }
                    }
                    div { button type="submit" class="btn btn-primary" { "Preview" } }
                }
            }
        },
    )
}

/// What the upload preview shows.
pub struct UploadPreview<'a> {
    /// The prepared file, base64, carried to publishing.
    pub data: &'a str,
    pub analysis: &'a Analysis,
    /// The build less its interface layers.
    pub size: [u16; 3],
    /// The id of a stored build with the same file, if any.
    pub duplicate: Option<&'a str>,
    pub fields: &'a Fields<'a>,
    pub choices: &'a Choices,
    pub tags: &'a [Tag],
    pub chosen_tags: &'a [String],
    pub unconventional: bool,
    pub message: &'a str,
}

/// The upload page's second step: checking what the site read and filling
/// in the details.
pub fn upload_preview(user: &User, p: &UploadPreview) -> Markup {
    let [x, y, z] = p.size;
    base(
        Some(user),
        "Upload",
        "/upload",
        &html! {
            h1 class={ (PAGE_TITLE) " mb-6" } { "Check and describe it" }
            // The viewer fetches the file from a data address, since nothing
            // is stored until publishing.
            (beside_viewer(&format!("data:application/octet-stream;base64,{}", p.data), &html! {
                div class="flex flex-col gap-3" {
                    p { "Size: " span class="tabular-nums" { (x) "×" (y) "×" (z) } }
                    @if let Some(id) = p.duplicate {
                        p role="alert" class="text-danger" {
                            "This exact file is already on the site: "
                            a href={ "/schematics/" (id) } { "see it here" } "."
                        }
                    }
                    a href="/upload" { "Choose a different file" }
                }
                (upload_form(p))
            }))
            (interface_section(p.analysis))
            script defer src="/form.js" {}
        },
    )
}

/// The upload page's details form, one field to a line.
fn upload_form(p: &UploadPreview) -> Markup {
    html! {
            form action="/upload" method="post" class="flex flex-col gap-4" {
                h2 class=(SECTION_TITLE) { "Details" }
                input type="hidden" name="data" value=(p.data);
                (fields(p.fields, p.choices))
                (tag_picker(p.tags, p.chosen_tags))
                @if !p.analysis.violations.is_empty() || p.unconventional {
                    label class="flex items-start gap-2" {
                        input type="checkbox" name="unconventional" value="yes"
                            checked[p.unconventional] class="mt-1 size-4 appearance-auto";
                        span {
                            "This build follows its own layout, such as a display, so the building conventions do not apply to it."
                        }
                    }
                }
                @if !p.message.is_empty() {
                    p role="alert" class="text-danger" { (p.message) }
                }
                div {
                    button type="submit" class="btn btn-primary" disabled[p.duplicate.is_some()] { "Publish" }
                }
            }
    }
}
