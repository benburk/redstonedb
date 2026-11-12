//! What a viewer can do with a build from its page: favourite it,
//! download it as they choose, and, for those who may, edit, merge, or
//! delete it.

use maud::{Markup, html};

use crate::{
    account::User,
    build::Build,
    web::{
        build::{detail_view::ColoursShown, download::Colours},
        href,
        ui::{
            CHOICE, SECTION_TITLE, SWITCHES, dialog,
            icons::{self, icon},
            text,
        },
    },
};

/// A star that favourites the build at `path`, filled once it is, and
/// pressed again takes it back, beside how many people have favourited it,
/// `count`, once anyone has. Through fixi the answer is the star alone, so
/// the rest of the page, its viewer too, stays as it is.
pub fn favorite_button(path: &str, favorited: bool, count: i64) -> Markup {
    let label = if favorited {
        "Remove from favourites"
    } else {
        "Add to favourites"
    };
    let title = if count > 0 {
        format!("{label} · {}", text::favourites_label(count))
    } else {
        label.to_owned()
    };
    html! {
        form action={ (path) "/favorite" } method="post" class="contents"
            fx-action={ (path) "/favorite" } fx-method="post" {
            @if !favorited {
                input type="hidden" name="on" value="yes";
            }
            button id="favorite" type="submit" aria-label=(title) title=(title) aria-pressed=(favorited)
                class={ "btn btn-quiet flex h-10 min-w-10 cursor-pointer items-center justify-center gap-1.5 tabular-nums "
                    (if count > 0 { "px-2.5" } else { "p-0" }) } {
                (icon(icons::STAR, if favorited { "size-5 fill-current text-brand" } else { "size-5" }))
                @if count > 0 { span { (count) } }
            }
        }
    }
}

/// What else the viewer may do with build `s`, at `path`, under one button:
/// edit it, upload a new version, or delete it, going back to `from`, the
/// listing the viewer came from. Nothing for a viewer who may do none of
/// them.
pub(crate) fn actions_menu(
    s: &Build,
    viewer: Option<&User>,
    path: &str,
    from: Option<&str>,
) -> Markup {
    let edits = viewer.is_some_and(|v| v.may_edit(s));
    let merges = viewer.is_some_and(User::can_delete);
    let deletes = viewer.is_some_and(|v| v.may_delete(s));
    let item = "flex min-h-9 w-full items-center gap-2.5 rounded-control px-2.5 text-left hover:bg-selection hover:no-underline";
    html! {
        @if edits || merges || deletes {
            details class="relative" data-menu {
                summary aria-label="More actions" title="More actions"
                    class="btn btn-quiet flex size-10 cursor-pointer list-none items-center justify-center p-0 [&::-webkit-details-marker]:hidden" {
                    (icon(icons::MORE, "size-5"))
                }
                div class="absolute top-full right-0 z-20 mt-1 flex w-max min-w-44 flex-col rounded-overlay border border-line bg-raised p-1 shadow-overlay" {
                    @if edits {
                        a href={ (path) "/edit" } class={ (item) " text-ink" } { (icon(icons::PENCIL, "size-4 text-muted")) "Edit details" }
                        a href={ (path) "/versions/new" } class={ (item) " text-ink" } { (icon(icons::UPLOAD, "size-4 text-muted")) "Upload a new version" }
                    }
                    @if merges {
                        button type="button" commandfor="merge" command="show-modal"
                            class={ (item) " cursor-pointer text-ink" } {
                            (icon(icons::MERGE, "size-4 text-muted")) "Merge into another build"
                        }
                    }
                    @if deletes {
                        button type="button" commandfor="delete" command="show-modal"
                            class={ (item) " cursor-pointer text-danger" } {
                            (icon(icons::TRASH, "size-4")) "Delete"
                        }
                    }
                }
            }
            @if merges {
                (merge_dialog(s))
            }
            @if deletes {
                (delete_dialog(s, from))
            }
        }
    }
}

/// A mod's form for merging a build into another, as a fix uploaded as a
/// build of its own is: its file becomes that build's next version, if it
/// may, and it is deleted.
fn merge_dialog(s: &Build) -> Markup {
    html! {
        dialog id="merge" aria-labelledby="merge-heading"
            class="dialog w-full max-w-form" {
            form action={ (href::build(&s.id)) "/merge" } method="post" data-step class="flex flex-col gap-4" {
                h2 id="merge-heading" class=(SECTION_TITLE) { "Merge " (s.name()) " into another build" }
                p { "Its file becomes the other build's next version, and this build is deleted." }
                label class="flex flex-col gap-1" {
                    span { "Build to merge into" }
                    input type="text" name="into" required placeholder="Its link or id" class="input";
                }
                label class="flex flex-col gap-1" {
                    span { "Note " span class="text-muted" { "(optional)" } }
                    input type="text" name="note" placeholder={ "Merged from " (s.name()) } class="input";
                }
                div class="flex items-center gap-4" {
                    button type="submit" class="btn btn-primary" { "Merge" }
                    (dialog::close("merge", "Cancel"))
                }
            }
        }
    }
}

/// The confirmation before deleting a build, which then goes back
/// to `from`, the listing the viewer came from, else home.
fn delete_dialog(s: &Build, from: Option<&str>) -> Markup {
    html! {
        dialog id="delete" aria-labelledby="delete-heading"
            class="dialog w-full max-w-form" {
            form action={ (href::build(&s.id)) "/delete" } method="post" data-step class="flex flex-col gap-4" {
                @if let Some(from) = from {
                    input type="hidden" name="next" value=(from);
                }
                h2 id="delete-heading" class=(SECTION_TITLE) { "Delete " (s.name()) "?" }
                p { "It is taken off the site. An admin can restore it." }
                div class="flex items-center gap-4" {
                    button type="submit" class="btn btn-danger" { "Delete" }
                    (dialog::close("delete", "Cancel"))
                }
            }
        }
    }
}

/// The choices for downloading a build: the file's name; its colours, the
/// page's own first, or a palette made for this download alone, in an
/// editor custom asks the server for, with the fields so far, so choosing
/// it again keeps them; copies of a stackable one, or one copy with glass
/// at two corners of the box `//rstack` stacks, for `WorldEdit` to select
/// it by; and a button that sends the file so made to the viewer's
/// Schemati instead. Only someone signed in is shown it.
pub(crate) fn download_dialog(s: &Build, colours: ColoursShown) -> Markup {
    let path = href::build(&s.id);
    let filename = s.filename();
    let stem = filename.trim_end_matches(".schem");
    let options = [
        (Colours::Default, "Default", true),
        (Colours::Original, "Original", colours.original),
        (Colours::Theme, "Your theme", colours.theme),
        (Colours::Custom, "Custom", colours.themeable),
    ];
    html! {
        dialog id="download" aria-labelledby="download-heading"
            class="dialog w-full max-w-form" {
            form action={ (path) "/download" } method="get" class="flex flex-col gap-4" {
                h2 id="download-heading" class=(SECTION_TITLE) { "Download" }
                label class="field" {
                    span { "File name" }
                    div class="flex items-center gap-2" {
                        input name="name" value=(stem) required maxlength="100"
                            autocomplete="off" spellcheck="false" data-caret-end class="input flex-1";
                        span aria-hidden="true" class="text-muted" { ".schem" }
                    }
                }
                // Default alone is no choice, as for a build that takes no
                // theme and keeps no other colours.
                @if colours.original || colours.themeable {
                fieldset class="field" {
                    legend class="mb-1.5 text-meta font-semibold text-muted" { "Colours" }
                    // The page's colours first; custom asks for its editor.
                    div class={ (SWITCHES) " self-start" } {
                        @for (choice, label, offered) in options {
                            @if offered {
                                label class=(CHOICE) {
                                    @if choice == Colours::Custom {
                                        input type="radio" name="colours" value=(choice.name()) class="sr-only"
                                            fx-action="/palette/custom" fx-trigger="click" fx-target="#custom-colours";
                                    } @else {
                                        input type="radio" name="colours" value=(choice.name())
                                            checked[choice == colours.chosen] class="sr-only";
                                    }
                                    (label)
                                }
                            }
                        }
                    }
                }
                }
                // Where custom colours' editor comes in.
                div id="custom-colours" hidden {}
                @if !s.stacking.is_empty() {
                    div class="flex gap-2" {
                        @for direction in &s.stacking {
                            label class="field flex-1" {
                                span { "Copies " (direction.words()) }
                                input type="number" name=(direction.name()) min="1"
                                    max=[direction.max_copies] value="1" required
                                    class="input tabular-nums";
                            }
                        }
                    }
                }
                // One box marked, on one copy, as the server takes them.
                @if !s.stacking.is_empty() {
                    fieldset class="flex flex-col gap-1.5" {
                        legend class="mb-1.5 text-meta font-semibold text-muted" {
                            "Glass at the corners of the box to //rstack, to select and then break; one copy only"
                        }
                        label class="flex items-center gap-2" {
                            input type="radio" name="markers" value="" checked class="size-4 appearance-auto";
                            span { "None" }
                        }
                        @for direction in &s.stacking {
                            label class="flex items-center gap-2" {
                                input type="radio" name="markers" value=(direction.name()) class="size-4 appearance-auto";
                                span { "To stack " (direction.words()) }
                            }
                        }
                    }
                }
                div class="flex flex-wrap items-center gap-4" {
                    button type="submit" class="btn btn-primary" { "Download" }
                    button type="submit" formaction={ (path) "/schemati" } formmethod="post"
                        fx-action={ (path) "/schemati" } fx-method="post" fx-target="#sent" fx-swap="innerHTML"
                        class="btn btn-quiet" {
                        "Send to Schemati"
                    }
                    (dialog::close("download", "Cancel"))
                }
                div id="sent" aria-live="polite" class="text-meta empty:hidden" {}
            }
        }
    }
}
