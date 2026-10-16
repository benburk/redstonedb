//! The details form's fields, shared by the upload, edit, and new-version
//! pages: the category menu, the fields that follow it, stacking, the build
//! date, and the pill fields for builders, contributors, and tags.

use maud::{Markup, html};

use crate::{
    category, import,
    models::{
        catalogue::{Category, Design, Flag, Measure},
        schematic::{Encoding, Logic, Orientation},
        tag::Tag,
    },
};

use super::{
    super::form::{Form, STACK_DIRECTIONS, StackRow},
    sections,
};

/// The lists a details form chooses from or suggests, loaded by
/// [`super::super::form::choices`].
pub struct Choices {
    /// The standard categories.
    pub categories: Vec<Category>,
    pub designs: Vec<Design>,
    /// Every category's flags.
    pub flags: Vec<Flag>,
    /// Everyone credited on a stored build, to suggest as builders and
    /// contributors.
    pub people: Vec<String>,
    /// Every tag, to suggest.
    pub tags: Vec<Tag>,
}

/// The category, its name when other, its design, and its carry lines.
fn category_fields(f: &Form, choices: &Choices) -> Markup {
    html! {
        @let chosen = Category::find(&choices.categories, &f.category);
        @let named = chosen.is_some_and(|c| c.named);
        @let has = |measure| chosen.is_some_and(|c: &Category| c.has(measure));
        @let chosen_name = chosen.map_or("", |c| c.name.as_str());
        @let has_designs = choices
            .designs
            .iter()
            .any(|design| design.category == chosen_name);
        // Typed, with a menu of the standard categories under their
        // sections, as in the sidebar, which form.js opens and narrows as
        // the box is typed in; a name of the builder's own makes a category
        // off the list. Without the script, the box takes typing alone.
        div class="field" {
            label for="category" class="text-meta font-semibold text-muted" { "Category" }
            div class="relative" {
                input id="category" name="category" required autocomplete="off"
                    role="combobox" aria-controls="category-menu" aria-expanded="false"
                    aria-autocomplete="list"
                    value=(chosen.map_or_else(|| f.category.replace('_', " "), |c| c.label.clone()))
                    placeholder="Type to search, such as adder" aria-describedby="category-help"
                    class="input" data-category;
                div id="category-menu" role="listbox" aria-label="Categories" hidden
                    class="absolute inset-x-0 top-full z-20 mt-1 max-h-80 overflow-y-auto rounded-control border border-line bg-raised p-1 shadow-overlay"
                    data-category-menu {
                    @for (key, heading, members) in sections(&choices.categories) {
                        // Headings stand apart from the options under them:
                        // bold, with the options indented and a line between
                        // sections.
                        div role="group" aria-labelledby={ "category-group-" (key) } data-group
                            class="menu-section" {
                            div id={ "category-group-" (key) } role="presentation"
                                class="px-2.5 pt-2 pb-1 text-meta font-bold text-ink" { (heading) }
                            @for category in members {
                                @let behaviour = category::behaviour(&category.name);
                                div id={ "category-option-" (category.name) } role="option"
                                    aria-selected="false" data-value=(category.label)
                                    data-name=(category.name) data-named[category.named]
                                    data-logic=(category.logic.map_or("unknown", Logic::name))
                                    data-reads-file[behaviour.reads_file]
                                    data-infers-stacking[behaviour.stacking == category::Stacking::Inferred]
                                    data-finds-stacking[behaviour.stacking == category::Stacking::Found]
                                    data-tested[behaviour.test.is_some()]
                                    data-measures=(category.measures.iter().map(|m| m.name()).collect::<Vec<_>>().join(" "))
                                    class="cursor-pointer rounded-control py-1.5 pr-2.5 pl-5 text-nav hover:bg-selection aria-selected:bg-selection" {
                                    (category.label)
                                }
                            }
                        }
                    }
                    p role="presentation" hidden class="px-2.5 py-1.5 text-meta text-muted" data-no-match {
                        "No category matches; this name makes a new one."
                    }
                }
            }
            span id="category-help" class="text-meta font-normal text-muted" {
                "Or type your own, such as piston door."
            }
        }
        // For a category whose builds each have a name, such as bussing.
        label class="field" data-named-field hidden[!named] {
            span { "Name" }
            input name="given_name" value=(f.given_name) maxlength="60"
                placeholder="such as Bus Crossing" class="input";
        }
        // For a memory, such as RAM.
        label class="field" data-measure-field="bytes" hidden[!has(Measure::Bytes)] {
            span { "Capacity (bytes)" }
            input type="number" name="bytes" value=(f.bytes) min="1"
                inputmode="numeric" placeholder="such as 32" class="input";
        }
        // For a threshold.
        label class="field" data-measure-field="at_least" hidden[!has(Measure::AtLeast)] {
            span { "Turns on with at least (inputs)" }
            input type="number" name="at_least" value=(f.at_least) min="1"
                inputmode="numeric" placeholder="such as 3" class="input";
        }
        // Shown once the chosen category has designs to choose from.
        label class="field" data-design-field hidden[!has_designs] {
            span { "Design" }
            select name="design" class="input" data-design {
                option value="other" selected[f.design.is_empty() || f.design == "other"] { "Other" }
                @for category in &choices.categories {
                    @let designs: Vec<&Design> = choices.designs.iter().filter(|d| d.category == category.name).collect();
                    @if !designs.is_empty() {
                        optgroup label=(category.label) data-for=(category.name) {
                            @for design in designs {
                                option value=(design.name) selected[f.design == design.name] { (design.label()) }
                            }
                        }
                    }
                }
            }
        }
        // Each category's flags, sent under its own name; form.js shows only
        // the chosen category's. A flag starts ticked in every group that has
        // it, as the carry lines a file's signs show do, so it stays ticked
        // whichever category is chosen.
        @for category in &choices.categories {
            @let flags: Vec<&Flag> = choices.flags.iter().filter(|flag| flag.category == category.name).collect();
            @if !flags.is_empty() {
                (flag_group(category, &flags, f, category.name == chosen_name))
            }
        }
    }
}

/// `category`'s flags, ticked as `ticked` has them, shown when it is the
/// chosen category. An adder's are read from its file, so it says so
/// instead.
fn flag_group(
    category: &Category,
    flags: &[&Flag],
    f: &Form,
    chosen: bool,
) -> Markup {
    html! {
        div class="flex flex-wrap items-center gap-x-6 gap-y-2" data-flags-for=(category.name) hidden[!chosen] {
            @if category::behaviour(&category.name).flags_from_file {
                p class="text-muted" {
                    "Carry in and carry out are read from the file."
                }
            } @else {
                @for flag in flags {
                    label class="flex items-center gap-2" {
                        input type="checkbox" name={ "flag:" (category.name) } value=(flag.name)
                            checked[f.ticks(&flag.name)] class="size-4 appearance-auto";
                        (flag.label)
                    }
                }
            }
        }
    }
}

/// The inputs for a build's details, shared by editing and uploading.
/// `derived` is the width and orientation the file gives a category that
/// reads them from the file, such as an adder's from its lime inputs,
/// shown in place of the typed ones once such a category is chosen.
pub(super) fn fields(
    f: &Form,
    choices: &Choices,
    derived: &import::Derived,
) -> Markup {
    // A category off the list, such as a piston door, has no width or encoding.
    let other = !f.category.is_empty()
        && Category::find(&choices.categories, &f.category).is_none();
    html! {
        (category_fields(f, choices))
        label class="field" data-standard-only data-typed-width hidden[other] {
            span { "Width in bits" }
            input name="width" type="number" min="1" value=(f.width) class="input tabular-nums";
        }
        @if let Some(width) = derived.width {
            div class="field" data-derived-width hidden {
                span { "Width in bits" }
                p class="tabular-nums" { (width) span class="text-muted" { ", counted from the lime inputs" } }
            }
        }
        @if let Some(orientation) = derived.orientation {
            div class="field" data-derived-orientation hidden {
                span { "Orientation" }
                p {
                    (orientation.label())
                    span class="text-muted" { ", from the way the lime inputs run" }
                }
            }
        }
        label class="field" data-typed-orientation {
            span { "Orientation" }
            select name="orientation" class="input" {
                @for orientation in Orientation::ALL {
                    option value=(orientation.name()) selected[f.orientation == orientation.name()] { (orientation.label()) }
                }
            }
        }
        @if let Some(encoding) = derived.encoding {
            div class="field" data-derived-encoding hidden {
                span { "Encoding" }
                p {
                    (encoding.label())
                    span class="text-muted" {
                        @if encoding == Encoding::Hex { ", as the lime inputs take a signal strength" } @else { ", as the lime inputs take levers" }
                    }
                }
            }
        }
        label class="field" data-standard-only data-typed-encoding hidden[other] {
            span { "Encoding" }
            select name="encoding" class="input" {
                @for encoding in Encoding::ALL {
                    option value=(encoding.name()) selected[f.encoding == encoding.name()] { (encoding.label()) }
                }
            }
        }
        // Hidden by form.js once a passing simulation measures them.
        p data-timing-note hidden class="text-muted" {
            "Latency and synchronized are measured by the simulation."
            // Sent in place of the hidden fields, so nothing typed is kept.
            input type="hidden" name="synchronized_unknown" value="yes" disabled data-measured-marker;
        }
        div class="contents" data-timing-fields {
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
        }
        label class="field" data-standard-only hidden[other] {
            span { "Throughput in ticks" }
            input type="number" name="throughput" value=(f.throughput) min="1" max="1000"
                inputmode="numeric" placeholder="such as 3" aria-describedby="throughput-help"
                class="input tabular-nums";
            span id="throughput-help" class="text-meta font-normal" {
                "The fewest ticks between inputs that still come out right."
            }
        }
        (logic_field(f, choices))
        (stacking_fields(f))
        (person_field("Builders", "builder", "Add a builder's in-game name", &f.builders, choices))
        (person_field("Contributors", "contributor", "Add someone who helped", &f.contributors, choices))
        (date_field(&f.built_on))
        label class="field" {
            span { "Description" }
            textarea name="description" rows="3" class="input py-2 leading-prose" { (f.description) }
        }
    }
}

/// Stateless or stateful, or as the category usually is, which `form.js`
/// names as the category changes.
fn logic_field(f: &Form, choices: &Choices) -> Markup {
    let usual = Category::find(&choices.categories, &f.category)
        .and_then(|c| c.logic)
        .map_or("unknown", Logic::name);
    html! {
        label class="field" {
            span { "Stateless" }
            select name="logic" class="input" aria-describedby="logic-help" {
                option value="" selected[f.logic.is_empty()] data-usual-logic {
                    "As the category usually is (" (usual) ")"
                }
                @for logic in Logic::ALL {
                    option value=(logic.name()) selected[f.logic == logic.name()] { (logic.label()) }
                }
            }
            span id="logic-help" class="text-meta font-normal text-muted" {
                "Stateful builds remember something, as a register does."
            }
        }
    }
}

/// The build date, the browser's own date picker; left empty when it is
/// not known.
fn date_field(built_on: &str) -> Markup {
    html! {
        label class="field" {
            span { "Built on " span class="font-normal" { "(empty if unknown)" } }
            input type="date" name="built_on" value=(built_on)
                max=(crate::details::latest_build_date()) class="input w-auto tabular-nums";
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
        class: format!("badge-{}", tag.kind.name()),
    }
}

/// Whether the build stacks, and how: two rows, each a direction with its
/// step and most copies. The second row is for a build that also stacks
/// another way, and is left empty otherwise. The rows hide while the
/// toggle is off.
fn stacking_fields(f: &Form) -> Markup {
    let blank = StackRow::default();
    let rows = f
        .stack_rows
        .iter()
        .chain(std::iter::repeat(&blank))
        .take(f.stack_rows.len().max(2));
    html! {
        // For a category such as adder, the simulation stacks copies the way
        // the bits run and decides, so the rows give way to a note; for a
        // register, once it finds copies that work.
        div class="field" data-derived-stacking hidden {
            span { "Stacking" }
            p class="text-muted" {
                "Found by the simulation."
            }
        }
        fieldset class="group flex flex-col gap-3" data-typed-stacking {
            legend class="mb-1.5 text-meta font-semibold text-muted" { "Stacking" }
            label class="flex items-start gap-2" {
                input type="checkbox" name="stackable" value="yes" checked[f.stackable]
                    class="mt-1 size-4 appearance-auto";
                span { "Stackable: copies placed one after another make a bigger build" }
            }
            div class="flex flex-col gap-3 group-has-[[name=stackable]:not(:checked)]:hidden" {
                @for (i, row) in rows.enumerate() {
                    @let n = i + 1;
                    // Wraps between phrases, never inside one.
                    div class="flex flex-wrap items-center gap-x-2 gap-y-2" {
                        span class="flex items-center gap-2" {
                            "Each copy is"
                            input type="number" name="stack_step" min="1" value=(row.step)
                                aria-label={ "Blocks per copy, direction " (n) }
                                class="input w-18 tabular-nums";
                            "blocks"
                        }
                        select name="stack_direction" aria-label={ "Direction " (n) } class="input w-auto" {
                            @for (value, label) in STACK_DIRECTIONS {
                                option value=(value) selected[row.direction == value] { (label) }
                            }
                        }
                        span class="flex items-center gap-2" {
                            "up to"
                            input type="number" name="stack_max" min="2" value=(row.max) placeholder="any"
                                aria-label={ "Most copies, direction " (n) }
                                class="input w-18 tabular-nums";
                            "copies"
                        }
                    }
                }
                p class="text-meta text-muted" {
                    "Fill in the second row only for a build that also stacks another way, such as north for "
                    "more registers side by side. Leave the most copies empty for no limit. Saving stacks two "
                    "copies each way to check the step."
                }
            }
        }
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
        fixed: Vec::new(),
        chosen: chosen.iter().map(|name| person_pill(name)).collect(),
        new_class: "badge-builder",
        pattern: "^[A-Za-z0-9_]{1,16}$",
        hint: "is not a Minecraft name: letters, digits, and underscores, up to 16",
        keep_case: true,
    })
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
    /// Values that are the build's whatever is chosen, shown before the
    /// chosen ones without a remove button, and not sent.
    fixed: Vec<Pill>,
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
                @if !f.fixed.is_empty() {
                    ul class="contents" aria-label={ "Read from the file: " (f.label) } {
                        @for fixed in &f.fixed {
                            li {
                                span class={ "inline-flex items-center rounded-full px-2 py-1 text-meta font-medium whitespace-nowrap " (fixed.class) }
                                    title="Read from the file, so it cannot be removed" {
                                    (fixed.label)
                                }
                            }
                        }
                    }
                }
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
        li {
            // Fits its name and its ×, with even space all round: the left
            // is wider by what the × keeps inside its own box. Names have
            // capitals, so the text is centred as it is, without the lift
            // a lowercase badge takes.
            span data-pill class={ "inline-flex items-center gap-1 rounded-full py-1 pr-1 pl-2 text-meta font-medium whitespace-nowrap " (p.class) } {
                span data-label { (p.label) }
                button type="button" data-remove
                    aria-label={ "Remove " (if p.label.is_empty() { "this" } else { &p.label }) }
                    class="flex size-4 cursor-pointer items-center justify-center rounded-full opacity-60 hover:bg-current/15 hover:opacity-100 focus-visible:opacity-100" {
                    (cross())
                }
            }
            input type="hidden" name=(name) value=(p.value);
        }
    }
}

/// The tag field of the details forms. Only an admin, `may_create`, may
/// name a tag not yet on the list. The tags the build's file gives it,
/// `fixed`, such as torchless, show first and cannot be removed, and no
/// tag worked out from a build is suggested.
pub(super) fn tag_picker(
    tags: &[Tag],
    chosen: &[String],
    fixed: &[Tag],
    may_create: bool,
) -> Markup {
    pill_field(&PillField {
        label: "Tags",
        name: "tag",
        placeholder: "Add a tag, such as double-density",
        suggestions: tags
            .iter()
            .filter(|tag| !tag.is_derived())
            .map(tag_pill)
            .collect(),
        fixed: fixed.iter().map(tag_pill).collect(),
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
        // A pattern nothing matches, so only listed tags can be added.
        pattern: if may_create {
            "^[A-Za-z0-9 _]([A-Za-z0-9 _-]*[A-Za-z0-9 _])?$"
        } else {
            "(?!)"
        },
        hint: if may_create {
            "cannot be a tag: use letters, digits, spaces, and dashes, with \
             no dash at either end"
        } else {
            "is not a tag; only an admin can make a new one"
        },
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::tag::Kind;

    fn tag(name: &str, kind: Kind) -> Tag {
        Tag {
            name: name.to_owned(),
            token: None,
            kind,
        }
    }

    /// The tags the file gives a build show without a remove button and
    /// are sent by no input, and no derived tag is suggested.
    #[test]
    fn tags_read_from_the_file_show_but_cannot_be_removed_or_added() {
        let tags = [
            tag("alpha", Kind::Restriction),
            tag("torchless", Kind::Restriction),
            tag("double-density", Kind::Other),
        ];
        let fixed = [tag("torchless", Kind::Restriction)];
        let html =
            tag_picker(&tags, &["double-density".to_owned()], &fixed, false)
                .into_string();
        let (read, rest) = html.split_once("data-chosen").unwrap();
        assert!(read.contains(">torchless<"));
        assert!(!read.contains("data-remove"));
        assert!(!html.contains(r#"name="tag" value="torchless""#));
        assert!(rest.contains(r#"name="tag" value="double-density""#));
        let suggested = html.split_once("<datalist").unwrap().1;
        assert!(
            !suggested.contains("alpha") && !suggested.contains("torchless")
        );
    }
}
