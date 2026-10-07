//! Full page templates.
//!
//! These return complete HTML documents wrapped in the base layout.
//! For fixi partial updates, use components directly.

use maud::{Markup, PreEscaped, html};

use crate::{
    APP_NAME,
    interface::{Bit, Interface, Port},
    lint::Violation,
    models::{
        schematic::{Schematic, StackDirection, Tag},
        user::{Role, User},
    },
    search::Term,
};

use super::components::base;

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
                    "Every word must match: a function, orientation, tag, width such as 8b, author, "
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
        s.author.clone(),
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
                                th scope="col" class=(TABLE_HEAD) { "Author" }
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
                                    td class=(TABLE_CELL) { (s.author.as_deref().unwrap_or("")) }
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

/// One schematic's page. `available` holds the tags that may still be
/// attached, and is `None` for a visitor who may not edit tags.
pub fn schematic(
    viewer: Option<&User>,
    s: &Schematic,
    available: Option<&[Tag]>,
    analysis: &Result<Analysis, String>,
) -> Markup {
    let name = s.name();
    let path = format!("/schematics/{}", s.id);
    base(
        viewer,
        &name,
        &path,
        &html! {
            h1 class={ (PAGE_TITLE) " mb-6" } { (name) }
            @if let Some(description) = &s.description {
                p class="-mt-3 mb-6 max-w-prose leading-prose whitespace-pre-line text-muted" { (description) }
            }
            div class="grid gap-8 desktop:grid-cols-[minmax(0,1fr)_var(--spacing-aside)]" {
                (viewer_3d(&format!("{path}/download")))
                (schematic_info(s, available, ""))
            }
            @if viewer.is_some_and(User::can_edit) {
                (details_form(s))
            }
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
                "Read from the file's levers, yellow output blocks, and signs. Positions count from the first block inside the corner markers."
            }
            div class="overflow-x-auto" {
                table class="w-full border-collapse" {
                    caption class="sr-only" { "Inputs" }
                    thead {
                        tr {
                            th scope="col" class=(TABLE_HEAD) { "Input" }
                            th scope="col" class=(TABLE_HEAD) { "Block" }
                            th scope="col" class=(TABLE_HEAD) { "Bits" }
                            th scope="col" class=(TABLE_HEAD) { "LSB at" }
                            th scope="col" class=(TABLE_HEAD) { "Spacing" }
                            th scope="col" class=(TABLE_HEAD) { "Enters from" }
                            th scope="col" class=(TABLE_HEAD) { "Drive" }
                            th scope="col" class=(TABLE_HEAD) { "Strength" }
                            th scope="col" class=(TABLE_HEAD) { "Inverted" }
                        }
                    }
                    tbody {
                        @for port in inputs {
                            tr {
                                th scope="row" class={ (TABLE_CELL) " text-left font-semibold" } { (port.name) }
                                td class=(TABLE_CELL) { (port.block) }
                                td class={ (TABLE_CELL) " tabular-nums" } { (port.bits.len()) }
                                td class={ (TABLE_CELL) " tabular-nums whitespace-nowrap" } { (lsb(port)) }
                                td class={ (TABLE_CELL) " whitespace-nowrap" } { (spacing(&port.bits)) }
                                td class=(TABLE_CELL) { (uniform(&port.bits, |b| b.entry.clone().unwrap_or_default())) }
                                td class=(TABLE_CELL) { (uniform(&port.bits, |b| (if b.hard { "hard" } else { "soft" }).to_owned())) }
                                td class={ (TABLE_CELL) " tabular-nums" } { (port.strength) " or more" }
                                td class={ (TABLE_CELL) " tabular-nums" } { (inverted(&port.bits)) }
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
                            th scope="col" class=(TABLE_HEAD) { "Block" }
                            th scope="col" class=(TABLE_HEAD) { "Bits" }
                            th scope="col" class=(TABLE_HEAD) { "LSB at" }
                            th scope="col" class=(TABLE_HEAD) { "Spacing" }
                        }
                    }
                    tbody {
                        @for port in outputs {
                            tr {
                                th scope="row" class={ (TABLE_CELL) " text-left font-semibold" } { (port.name) }
                                td class=(TABLE_CELL) { (port.block) }
                                td class={ (TABLE_CELL) " tabular-nums" } { (port.bits.len()) }
                                td class={ (TABLE_CELL) " tabular-nums whitespace-nowrap" } { (lsb(port)) }
                                td class={ (TABLE_CELL) " whitespace-nowrap" } { (spacing(&port.bits)) }
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

fn lsb(port: &Port) -> String {
    port.bits.first().map_or_else(String::new, |bit| {
        let [x, y, z] = bit.pos;
        format!("{x}, {y}, {z}")
    })
}

/// How far apart neighbouring bits are, such as `2 along −x`.
fn spacing(bits: &[Bit]) -> String {
    let steps: Vec<[i32; 3]> = bits
        .windows(2)
        .map(|pair| [0, 1, 2].map(|i| pair[1].pos[i] - pair[0].pos[i]))
        .collect();
    let Some(first) = steps.first() else {
        return String::new();
    };
    let axis = (0..3).max_by_key(|&i| first[i].abs()).unwrap_or(0);
    if steps.iter().any(|step| step[axis] != first[axis]) {
        return "irregular".to_owned();
    }
    let sign = if first[axis] < 0 { "−" } else { "+" };
    let staggered = steps.iter().any(|step| step != first);
    format!(
        "{} along {sign}{}{}",
        first[axis].abs(),
        ["x", "y", "z"][axis],
        if staggered { ", staggered" } else { "" }
    )
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

/// The bits saved with their lever on, counting from the LSB.
fn inverted(bits: &[Bit]) -> String {
    let indices: Vec<String> = bits
        .iter()
        .enumerate()
        .filter(|(_, bit)| bit.inverted)
        .map(|(i, _)| i.to_string())
        .collect();
    match indices.len() {
        0 => String::new(),
        n if n == bits.len() && n > 1 => "all".to_owned(),
        _ if bits.len() == 1 => "yes".to_owned(),
        _ => indices.join(", "),
    }
}

/// Everything on a schematic's page except its title and viewer. Tag edits
/// redraw it whole, since a tag can change the filename shown here too.
pub fn schematic_info(
    s: &Schematic,
    available: Option<&[Tag]>,
    message: &str,
) -> Markup {
    let path = format!("/schematics/{}", s.id);
    let file = s.filename();
    html! {
        div id="info" class="flex flex-col gap-8" {
            section aria-labelledby="tags-heading" {
                h2 id="tags-heading" tabindex="-1" class={ (SECTION_TITLE) " mb-3" } { "Tags" }
                @if s.tags.is_empty() {
                    p class="text-muted" { "No tags yet." }
                } @else {
                    ul class="flex flex-wrap gap-1.5" {
                        @for tag in &s.tags {
                            li {
                                @if available.is_some() {
                                    (removable_badge(&path, tag))
                                } @else {
                                    (badge(tag))
                                }
                            }
                        }
                    }
                }
                @match available {
                    Some(available) if !available.is_empty() => {
                        form action={ (path) "/tags" } method="post"
                            fx-action={ (path) "/tags" } fx-method="post" fx-target="#info"
                            class="mt-3 flex gap-2" {
                            label for="add-tag" class="sr-only" { "Tag to add" }
                            select id="add-tag" name="tag" class="input min-h-9 flex-1"
                                aria-invalid=[(!message.is_empty()).then_some("true")]
                                aria-describedby=[(!message.is_empty()).then_some("tag-message")] {
                                @for category in ["restriction", "timing", "other"] {
                                    @let tags: Vec<&Tag> = available.iter().filter(|t| t.category == category).collect();
                                    @if !tags.is_empty() {
                                        optgroup label=(category_label(category)) {
                                            @for tag in tags {
                                                option value=(tag.name) { (tag.label()) }
                                            }
                                        }
                                    }
                                }
                            }
                            button type="submit" class="btn btn-quiet" { "Add" }
                        }
                    }
                    _ => {}
                }
                @if !message.is_empty() {
                    p id="tag-message" role="alert" class="message mt-2" { (message) }
                }
            }
            section aria-labelledby="file-heading" class="flex flex-col gap-2" {
                h2 id="file-heading" class="sr-only" { "File" }
                a href={ (path) "/download" } download=(file) class="btn btn-primary" { "Download" }
                p class="text-meta break-all text-muted" { code { (file) } }
                @if !s.stacking.is_empty() {
                    form action={ (path) "/download" } method="get" class="mt-3 flex flex-col gap-3" {
                        div class="flex gap-2" {
                            @for direction in &s.stacking {
                                label class="field flex-1" {
                                    span { "Copies " (direction.name().replace('-', " and ")) }
                                    input type="number" name=(direction.name()) min="1"
                                        max=[direction.max_copies]
                                        value=(direction.max_copies.unwrap_or(8).min(8)) required
                                        class="input min-h-9 tabular-nums";
                                }
                            }
                        }
                        button type="submit" class="btn btn-quiet" { "Download stack" }
                    }
                }
            }
            section aria-labelledby="details-heading" {
                h2 id="details-heading" class={ (SECTION_TITLE) " mb-3" } { "Details" }
                dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-6 gap-y-2" {
                    (field("Function", &s.function.replace('_', " ")))
                    (field("Width", &s.width_label().unwrap_or_else(|| "unknown".to_owned())))
                    (field("Orientation", s.orientation_label()))
                    (field("Encoding", s.encoding_label()))
                    (field("Latency", &s.latency.map_or_else(|| "unknown".to_owned(), |t| format!("{t} ticks"))))
                    (field("Synchronized", s.synchronized_label()))
                    (field("Stacks", &stacking(s)))
                    (field("Size", &size(s)))
                    (field("Volume", &volume(s)))
                    (field("Author", s.author.as_deref().unwrap_or("unknown")))
                    (field("Built", s.built_on.as_deref().unwrap_or("unknown")))
                    (field("Imported by", s.importer().unwrap_or("unknown")))
                    (field("Conventions", if s.conventional { "followed" } else { "not followed" }))
                }
            }
        }
    }
}

/// The mod's form for a build's details, in a disclosure below its view.
fn details_form(s: &Schematic) -> Markup {
    let path = format!("/schematics/{}", s.id);
    let number = |n: Option<i64>| n.map(|n| n.to_string()).unwrap_or_default();
    html! {
        details class="mt-8 max-w-calm rounded-overlay border border-line bg-raised p-5 shadow-sm" {
            summary class={ (SECTION_TITLE) " cursor-pointer" } { "Edit details" }
            form action={ (path) "/details" } method="post" class="mt-4 grid gap-4 sm:grid-cols-2" {
                (fields(&Fields {
                    function: &s.function,
                    width: &number(s.width),
                    orientation: &s.orientation,
                    encoding: &s.encoding,
                    latency: &number(s.latency),
                    synchronized: s.synchronized,
                    author: s.author.as_deref().unwrap_or(""),
                    built_on: s.built_on.as_deref().unwrap_or(""),
                    description: s.description.as_deref().unwrap_or(""),
                }))
                div class="sm:col-span-2" {
                    button type="submit" class="btn btn-primary" { "Save details" }
                }
            }
        }
    }
}

/// The values a details form starts with.
pub struct Fields<'a> {
    pub function: &'a str,
    pub width: &'a str,
    pub orientation: &'a str,
    pub encoding: &'a str,
    pub latency: &'a str,
    pub synchronized: Option<bool>,
    pub author: &'a str,
    pub built_on: &'a str,
    pub description: &'a str,
}

/// The inputs for a build's details, shared by editing and uploading.
fn fields(f: &Fields) -> Markup {
    html! {
        label class="field" {
            span { "Function" }
            input name="function" value=(f.function) required
                pattern="[a-z][a-z0-9_]*" title="Lowercase words joined by underscores, such as line_drawer"
                class="input";
        }
        label class="field" {
            span { "Width in bits" }
            input name="width" type="number" min="1" value=(f.width) class="input tabular-nums";
        }
        label class="field" {
            span { "Orientation" }
            select name="orientation" class="input" {
                @for (value, label) in [("horz", "horizontal"), ("vert", "vertical"), ("diag", "diagonal")] {
                    option value=(value) selected[f.orientation == value] { (label) }
                }
            }
        }
        label class="field" {
            span { "Encoding" }
            select name="encoding" class="input" {
                @for (value, label) in [("bin", "binary"), ("hex", "signal strength")] {
                    option value=(value) selected[f.encoding == value] { (label) }
                }
            }
        }
        label class="field" {
            span { "Latency in ticks" }
            input name="latency" type="number" min="0" value=(f.latency) class="input tabular-nums";
        }
        label class="field" {
            span { "Synchronized" }
            select name="synchronized" class="input" {
                option value="" selected[f.synchronized.is_none()] { "unknown" }
                option value="yes" selected[f.synchronized == Some(true)] { "yes, every output bit settles on one tick" }
                option value="no" selected[f.synchronized == Some(false)] { "no" }
            }
        }
        label class="field" {
            span { "Built by" }
            input name="author" value=(f.author) placeholder="In-game name"
                pattern="[A-Za-z0-9_]+" title="A Minecraft name: letters, digits, and underscores"
                class="input";
        }
        label class="field" {
            span { "Built on" }
            input name="built_on" type="date" value=(f.built_on) class="input";
        }
        label class="field sm:col-span-2" {
            span { "Description" }
            textarea name="description" rows="3" class="input py-2 leading-prose" { (f.description) }
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

fn removable_badge(path: &str, tag: &Tag) -> Markup {
    html! {
        span class={ "badge badge-" (tag.category) " pr-1" } title=(category_label(&tag.category)) {
            (tag.label())
            button type="button" id={ "remove-tag-" (tag.name) }
                aria-label={ "Remove " (tag.label()) }
                fx-action={ (path) "/tags/" (tag.name) } fx-method="delete" fx-target="#info"
                class="flex size-4.5 cursor-pointer items-center justify-center rounded-full opacity-70 hover:bg-current/15 hover:opacity-100" {
                svg aria-hidden="true" viewBox="0 0 12 12" class="size-2.5" {
                    path d="M3 3l6 6M9 3l-6 6" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" {}
                }
            }
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

/// A 3D view of the schematic, drawn by schematic-renderer with the vanilla
/// resource pack it publishes alongside its own build.
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
                    dd { "Anything else worth searching by, such as the design." }
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
                    "Next you will see what the site reads from it and fill in its details."
                }
                form action="/upload/preview" method="post" enctype="multipart/form-data"
                    class="flex flex-col gap-4 rounded-overlay border border-line bg-raised p-5 shadow-sm" {
                    label class="field" {
                        span { "Schematic file" }
                        input type="file" name="file" accept=".schem" required class="input py-1.5";
                    }
                    label class="flex items-start gap-2" {
                        input type="checkbox" name="standardize" value="yes" class="mt-1 size-4 appearance-auto";
                        span {
                            "Standardize it: turn every block but redstone parts, inputs, and outputs white, "
                            "slabs and glass into white stained glass, and add the corner markers. "
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
    /// The thumbnail as a data address.
    pub thumbnail: Option<&'a str>,
    pub analysis: &'a Analysis,
    /// Inside the corner markers.
    pub size: [u16; 3],
    /// The id of a stored build with the same file, if any.
    pub duplicate: Option<&'a str>,
    pub fields: &'a Fields<'a>,
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
            div class="grid gap-8 desktop:grid-cols-[minmax(0,1fr)_var(--spacing-aside)]" {
                div class="overflow-hidden rounded-overlay border border-line bg-sidebar" {
                    @if let Some(thumbnail) = p.thumbnail {
                        img src=(thumbnail) alt="The build seen from the south-east" width="640" height="480"
                            class="aspect-[4/3] w-full object-contain";
                    }
                }
                div class="flex flex-col gap-3" {
                    p { "Size inside the corner markers: " span class="tabular-nums" { (x) "×" (y) "×" (z) } }
                    @if let Some(id) = p.duplicate {
                        p role="alert" class="text-danger" {
                            "This exact file is already on the site: "
                            a href={ "/schematics/" (id) } { "see it here" } "."
                        }
                    }
                    a href="/upload" { "Choose a different file" }
                }
            }
            (interface_section(p.analysis))
            form action="/upload" method="post" class="mt-12 grid max-w-calm gap-4 sm:grid-cols-2" {
                h2 class={ (SECTION_TITLE) " sm:col-span-2" } { "Details" }
                input type="hidden" name="data" value=(p.data);
                (fields(p.fields))
                fieldset class="flex flex-col gap-2 sm:col-span-2" {
                    legend class="mb-2 text-meta font-semibold text-muted" { "Tags" }
                    div class="flex flex-wrap gap-x-5 gap-y-2" {
                        @for tag in p.tags {
                            label class="flex items-center gap-2" {
                                input type="checkbox" name="tag" value=(tag.name)
                                    checked[p.chosen_tags.contains(&tag.name)]
                                    class="size-4 appearance-auto";
                                (badge(tag))
                            }
                        }
                    }
                }
                @if !p.analysis.violations.is_empty() || p.unconventional {
                    label class="flex items-start gap-2 sm:col-span-2" {
                        input type="checkbox" name="unconventional" value="yes"
                            checked[p.unconventional] class="mt-1 size-4 appearance-auto";
                        span {
                            "This build follows its own layout, such as a display, so the building conventions do not apply to it."
                        }
                    }
                }
                @if !p.message.is_empty() {
                    p role="alert" class="text-danger sm:col-span-2" { (p.message) }
                }
                div class="sm:col-span-2" {
                    button type="submit" class="btn btn-primary" disabled[p.duplicate.is_some()] { "Publish" }
                }
            }
        },
    )
}
