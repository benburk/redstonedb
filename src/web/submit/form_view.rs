//! The details form's fields, shared by the upload, edit, and new-version
//! pages: the category menu, the fields that follow it, stacking, the build
//! date, and the pill fields for authors, contributors, and tags.

use maud::{Markup, html};

use crate::{
    build::tag::Tag,
    category::{self, Category, Logic},
    ingest::read::FromFile,
    interface::Orientation,
    web::{
        href,
        submit::form::{Form, STACK_DIRECTIONS, StackRow},
        ui::sections,
    },
};

/// The lists a details form chooses from or suggests, loaded by
/// [`crate::web::submit::form::choices`].
pub struct Choices {
    /// The standard categories.
    pub categories: Vec<Category>,
    /// Everyone credited on a stored build, to suggest as authors and
    /// contributors.
    pub people: Vec<String>,
    /// Every tag, to suggest.
    pub tags: Vec<Tag>,
}

/// The category: one of the list's, or a name of the author's own.
fn category_fields(f: &Form, choices: &Choices) -> Markup {
    html! {
        @let chosen = Category::find(&choices.categories, &f.category);
        // Typed, with a menu of the standard categories under their
        // sections, as in the sidebar, which form.js opens and narrows as
        // the box is typed in; a name of the author's own makes a category
        // off the list. Without the script, the box takes typing alone.
        div class="field" {
            // Whether the build is verified as this category, which
            // form.js says once the simulation has run.
            div class="flex items-center gap-2" {
                label for="category" class="text-meta font-semibold text-muted" { "Category" }
                span class="badge" data-verdict-badge hidden {}
            }
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
                                @let listed = category::listed(&category.name);
                                div id={ "category-option-" (category.name) } role="option"
                                    aria-selected="false" data-value=(category.label)
                                    data-name=(category.name)
                                    data-logic=(category.logic.map_or("unknown", Logic::name))
                                    data-listed[listed]
                                    data-never-stacks[category::behaviour(&category.name).stacking == category::Stacking::Never]
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
    }
}

/// The mark on a field's label that its value was found, not typed, such
/// as `measured`; `form.js` shows the latency's and synchronized's while
/// the simulation fills them, saying `measuring…` until it has.
fn found_mark(how: &str, hidden: bool) -> Markup {
    html! {
        span class="font-normal text-muted" data-found-mark hidden[hidden] { " · " (how) }
    }
}

/// The inputs for a build's details, shared by editing and uploading.
/// `derived` is the width and orientation the file gives a category that
/// reads them from the file, such as an adder's from its input A,
/// shown in place of the typed ones once such a category is chosen.
pub(crate) fn fields(
    f: &Form,
    choices: &Choices,
    derived: &FromFile,
) -> Markup {
    html! {
        (category_fields(f, choices))
        // A standard category's, found in the file, shown as the field it
        // is typed in otherwise, but locked.
        @if let Some(width) = derived.width {
            div class="field" data-derived-width hidden {
                span { "Width in bits" (found_mark("counted from input A", false)) }
                input type="number" value=(width) disabled class="input tabular-nums";
            }
        }
        // Only an unverified build's width is typed; form.js shows it.
        label class="field" data-typed-width hidden {
            span { "Width in bits" }
            input name="width" type="number" min="1" value=(f.width)
                placeholder=[derived.width] class="input tabular-nums";
        }
        @if let Some(orientation) = derived.orientation {
            div class="field" data-derived-orientation hidden {
                span { "Orientation" (found_mark("from the way input A runs", false)) }
                select disabled class="input" { option selected { (orientation.label()) } }
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
        // Enabled by form.js while the simulation measures them, sent in
        // place of the locked fields, so nothing typed is kept.
        input type="hidden" name="synchronized_unknown" value="yes" disabled data-measured-marker;
        // Locked by form.js, and filled, while the simulation measures them.
        div class="contents" data-timing-fields {
        label class="field" {
            span { "Latency in ticks" (found_mark("measured", true)) }
            input name="latency" type="number" min="0" value=(f.latency) class="input tabular-nums";
        }
        div class="field" {
            span { "Synchronized" (found_mark("measured", true)) }
            // An unknown build's toggle stays unknown until it is clicked;
            // form.js shows it as neither on nor off and drops the marker.
            label class="flex items-center gap-2" {
                input type="checkbox" name="synchronized" value="yes"
                    checked[f.synchronized == Some(true)]
                    data-unknown[f.synchronized.is_none()]
                    class="size-4 appearance-auto";
                "Every data output bit settles on the same tick, without glitches"
            }
            @if f.synchronized.is_none() {
                input type="hidden" name="synchronized_unknown" value="yes";
            }
        }
        }
        (logic_field(f, choices))
        (stacking_fields(f))
        (person_field("Authors", "author", "Add an author's in-game name", &f.authors, choices))
        (person_field("Contributors", "contributor", "Add someone who helped", &f.contributors, choices))
        (date_field(&f.built_on))
        // Typing @ offers the players the site credits, in a menu at the
        // text cursor, each an option here that form.js shows as it fits.
        label class="field" {
            span { "Description" }
            div class="relative" data-mentions {
                textarea name="description" rows="3" aria-describedby="description-help"
                    aria-autocomplete="list" aria-controls="mention-menu" aria-expanded="false"
                    class="input py-2 leading-prose" { (f.description) }
                div id="mention-menu" role="listbox" aria-label="Players" hidden
                    class="absolute z-20 max-h-60 w-max min-w-40 max-w-[min(16rem,100%)] overflow-y-auto rounded-control border border-line bg-raised p-1 shadow-overlay" {
                    @for (i, name) in choices.people.iter().enumerate() {
                        div id={ "mention-option-" (i) } role="option" aria-selected="false" data-name=(name) hidden
                            class="flex cursor-pointer items-center gap-2 rounded-control px-2.5 py-1.5 hover:bg-selection aria-selected:bg-selection" {
                            img src=(href::head(name)) alt="" width="16" height="16" loading="lazy"
                                class="size-4 rounded-[3px] bg-line [image-rendering:pixelated]";
                            (name)
                        }
                    }
                }
            }
            span id="description-help" class="message" { "Write @name to link a player by their in-game name." }
        }
        div class="field" {
            span { "Visibility" }
            label class="flex items-center gap-2" {
                input type="checkbox" name="public" value="yes" checked[f.public]
                    class="size-4 appearance-auto";
                "Public: anyone can find it. Untick to keep it off the listings, for only people you give its link."
            }
        }
    }
}

/// Stateless or stateful, or as the category usually is, which `form.js`
/// names as the category changes.
fn logic_field(f: &Form, choices: &Choices) -> Markup {
    let usual = Category::find(&choices.categories, &f.category)
        .and_then(|c| c.logic)
        .map_or("unknown", Logic::name);
    // A standard category's builds are found to hold state or not.
    let evaluated = category::listed(&f.category);
    html! {
        label class="field" data-logic-field hidden[evaluated] {
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

/// The build date, typed as `yyyy-mm-dd`, which the server checks is a
/// real day not still to come; left empty when it is not known. A text box
/// rather than the browser's date picker, which shows the date in the
/// reader's own regional order instead.
/// The date a build was built, in three boxes, year, month, and day,
/// each filled from `built_on` when it has one; `form.js` moves on to the
/// next box as each fills, and takes a whole date pasted into any.
fn date_field(built_on: &str) -> Markup {
    let mut parts = built_on.splitn(3, '-');
    let [year, month, day] = [parts.next(), parts.next(), parts.next()]
        .map(Option::unwrap_or_default);
    let part = |name: &str,
                label: &str,
                value: &str,
                digits: u8,
                width: &str| {
        html! {
            input type="text" name={ "built_on_" (name) } value=(value)
                inputmode="numeric" pattern={ "[0-9]{" (digits) "}" } maxlength=(digits)
                placeholder=(label) aria-label=(label) autocomplete="off"
                class={ "input tabular-nums text-center " (width) };
        }
    };
    html! {
        fieldset class="field" data-date {
            legend class="mb-1.5 text-meta font-semibold text-muted" { "Built on " span class="font-normal" { "(empty if unknown)" } }
            div class="flex items-center gap-1.5" {
                (part("year", "yyyy", year, 4, "w-18"))
                span class="text-muted" aria-hidden="true" { "-" }
                (part("month", "mm", month, 2, "w-12"))
                span class="text-muted" aria-hidden="true" { "-" }
                (part("day", "dd", day, 2, "w-12"))
            }
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
        class: tag.class().to_owned(),
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
        // For a standard category, the simulation stacks copies and decides,
        // so the rows give way to a note.
        div class="field" data-derived-stacking hidden {
            span { "Stacking" (found_mark("measured", false)) }
            // The stack the simulation found, once it has run.
            p class="text-muted" data-stack-note {
                "Found by the simulation once it has run."
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
                    "Fill in the second row only for a build that also stacks another way, such as up for "
                    "more registers on a shared bus. Leave the most copies empty for no limit. Saving stacks two "
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
        new_class: "badge-author",
        pattern: "^[A-Za-z0-9_]{1,16}$",
        hint: "is not a Minecraft name: letters, digits, and underscores, up to 16",
        keep_case: true,
    })
}

fn person_pill(name: &str) -> Pill {
    Pill {
        label: name.to_owned(),
        value: name.to_owned(),
        class: "badge-author".to_owned(),
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
pub(crate) fn tag_picker(
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
    use crate::build::tag::Kind;

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
