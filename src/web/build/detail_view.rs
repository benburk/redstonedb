//! A build's page: a verdict under its title, its key facts and specs
//! beside its viewer, each marked where the simulator or the file checked
//! it, then its simulation, inputs and outputs, stacking, and versions.

use maud::{Markup, html};

use crate::{
    account::User,
    build::{Build, Status},
    ingest::read::Analysis,
    interface::problem::Problem,
    simulate::Report,
    store::versions::Version,
    web::{
        back,
        build::{
            actions_view::{actions_menu, download_dialog, favorite_button},
            checks_view::{Known, Subject, checks_section},
            download::Colours,
            info_view::build_info,
            simulation_view::cases_pending,
            sources::stacking_found,
            stack_guide_view, versions_view,
        },
        href,
        ui::{
            self, PAGE_TITLE, SECTION_TITLE, SWITCH, SWITCHES, Tone, cards,
            hover::hover_note,
            icons::{self, icon},
            layout::{self, base_previewed},
            summary::{self, size},
            text::{self, grouped, percent},
            viewer::beside_viewer_under,
        },
    },
};

/// What a stored build's file says about it.
pub struct FileRead {
    pub analysis: Analysis,
    /// How many of each kind of block it has, most first.
    pub block_counts: Vec<(&'static str, usize)>,
    /// For a build not verified as its category, one the check by
    /// function recognizes, each fault that keeps it from being one.
    pub unrecognized: Vec<Problem>,
    /// Whether it takes a signal strength as an input, which keeps it
    /// from being recognized.
    pub strength_inputs: bool,
    /// How its stacking looks, each way it stacks.
    pub stacking: Vec<crate::stack::Review>,
    /// What uploading it takes out or swaps, each a sentence, such as its
    /// signs; empty for a stored build, whose upload is done.
    pub kept: Vec<String>,
}

/// What a build's page shows: the build, the builds like it, its file as read,
/// its simulation, its versions, and whether the viewer has favourited it.
pub struct BuildPage<'a> {
    pub s: &'a Build,
    pub similar: &'a [(Build, u32)],
    pub file: &'a Result<FileRead, String>,
    pub simulation: &'a Option<Report>,
    /// Whether storing it is still simulating it in the background, so
    /// what is stored is yet to come.
    pub simulating: bool,
    pub versions: &'a [Version],
    pub favorited: bool,
    pub colours: ColoursShown,
    /// The listing the viewer came to it from, such as a search, which
    /// its breadcrumbs link back to and deleting it returns to.
    pub from: Option<&'a str>,
}

/// The colours a build's page shows it in, and which others it offers:
/// its author's, when kept, and the viewer's theme, when they have one
/// and the build can take it.
#[derive(Clone, Copy)]
pub struct ColoursShown {
    pub chosen: Colours,
    pub original: bool,
    pub theme: bool,
    /// Whether the build can take a theme at all, as one the site knows
    /// the needs of: else it is shown and downloaded only in its own.
    pub themeable: bool,
}

/// One schematic's page, with an edit button for a mod.
pub fn schematic_page(viewer: Option<&User>, page: &BuildPage) -> Markup {
    let BuildPage {
        s,
        similar,
        file,
        simulation,
        simulating,
        versions,
        favorited,
        colours,
        from,
    } = *page;
    let report = simulation.as_ref();
    let name = s.name();
    let path = href::build(&s.id);
    base_previewed(
        viewer,
        &name,
        &path,
        &html! {
            (crumbs(s, from))
            div class="mb-6 flex flex-wrap items-start justify-between gap-4" {
                div class="flex flex-wrap items-center gap-x-4 gap-y-2" {
                    h1 class=(PAGE_TITLE) { (name) }
                    @if !s.entry.public {
                        (summary::private_badge())
                    }
                    (build_banner(s, simulating))
                }
                // On a phone, under the title, the download as wide as the
                // page with the menu at its end; beside the title once
                // there is room.
                div class="ml-auto flex w-full items-center gap-2 sm:w-auto" {
                    @if viewer.is_some() {
                        button type="button" commandfor="download" command="show-modal" class="btn btn-primary flex-1 sm:flex-none" { "Download" }
                        (download_dialog(s, colours))
                    } @else {
                        (ui::locked_download("Download", "btn btn-primary flex-1 sm:flex-none"))
                    }
                    @if viewer.is_some() {
                        (favorite_button(&path, favorited, s.entry.favorites))
                    } @else if s.entry.favorites > 0 {
                        @let label = text::favourites_label(s.entry.favorites);
                        span class="flex h-10 items-center gap-1.5 px-2 text-muted tabular-nums" title=(label) aria-label=(label) {
                            (icon(icons::STAR, "size-5")) (s.entry.favorites)
                        }
                    }
                    (actions_menu(s, viewer, &path, from))
                }
            }
            (beside_viewer_under(
                &format!("{path}/download?preview=yes&colours={}", colours.chosen.name()),
                &colours_switch(&path, colours, from),
                &build_info(s, similar, file, report),
                false,
            ))
            // Its checks as stages, each opening onto what it found; while
            // storing it is still simulating, the cases to come.
            @if simulating {
                (cases_pending(&format!("{path}/simulation")))
            } @else {
                (checks_section(&Known {
                    s: Subject::of(s), file, report,
                    trace: Some(&format!("{path}/trace")),
                    timing: None,
                }))
            }
            @if let Ok(file) = file {
                (stack_guide_view::stacking_section(s, &file.stacking, stacking_found(s), viewer.is_some()))
            }
            (versions_view::versions_section(s, versions, viewer, viewer.is_some_and(|user| user.may_restore(s))))
        },
        Some(&preview(s)),
    )
}

/// Beside a build's title, while storing it is still simulating it, as
/// just after it is stored, a badge saying so; once it is done, whether it
/// works is among its tags, see [`verdict_tags`]. A live panel, redrawn
/// when its simulation comes in.
pub fn build_banner(s: &Build, simulating: bool) -> Markup {
    html! {
        div id="build-banner" fx-action={ (href::build(&s.id)) "?panel=banner" } fx-trigger="redraw"
            class="flex flex-wrap items-center gap-1.5" {
            @if simulating {
                @let pill = html! {
                    span class={ "inline-block rounded-full px-2.5 text-meta leading-6 font-semibold " (Tone::Plain) } { "Simulating…" }
                };
                (hover_note("verdict-note", &pill, "Its cases and timing are running; this updates when they finish."))
            }
        }
    }
}

/// The tag a tag-like badge is drawn as: its label, its tone, and what it
/// says more on hover or focus.
type Verdict = (&'static str, Tone, String);

/// Whether build `s` works, as the first of its tags: passing, failing,
/// not tested, or of a category with no test; and when it is unranked
/// though it works, why. Each links to its checks.
fn verdicts(s: &Build, report: Option<&Report>) -> Vec<Verdict> {
    let status = {
        let note = match (s.status, report) {
            (Status::Passing, Some(Report::Ran(run))) => format!(
                "All {} cases of {} right.",
                grouped(run.cases),
                run.pattern
            ),
            (Status::Failing, Some(Report::Ran(run))) => format!(
                "{} of {} cases of {} wrong; see which in its checks.",
                grouped(run.wrong),
                grouped(run.cases),
                run.pattern
            ),
            (
                _,
                Some(Report::Broken { reason } | Report::Skipped { reason }),
            ) => reason.clone(),
            (Status::NoTest, _) => {
                "The site has no test for its category and width yet."
                    .to_owned()
            }
            _ => s.unverified.as_deref().unwrap_or_default().to_owned(),
        };
        let (label, tone) = match s.status {
            Status::Passing => ("passing", Tone::Good),
            Status::Failing => ("failing", Tone::Bad),
            Status::NotTested => ("not tested", Tone::Plain),
            Status::NoTest => ("no test", Tone::Plain),
        };
        (label, tone, note)
    };
    let mut verdicts = vec![status];
    // A working build that ranks says nothing more, ranking being the
    // rule; one that doesn't says why.
    if let Some(why) = s.unranked().filter(|_| s.status != Status::NoTest) {
        verdicts.push((
            "unranked",
            Tone::Warn,
            format!("Listed, but on no leaderboard: {why}."),
        ));
    }
    verdicts
}

/// Build `s`'s verdicts as tags, see [`verdicts`], each linking to its
/// checks.
pub(crate) fn verdict_tags(s: &Build, report: Option<&Report>) -> Markup {
    html! {
        @for (i, (label, tone, note)) in verdicts(s, report).into_iter().enumerate() {
            li {
                @let pill = html! {
                    a href="#checks" class={ "badge hover:no-underline " (tone) } { (label) }
                };
                (hover_note(&format!("verdict-note-{i}"), &pill, &note))
            }
        }
    }
}

/// Where build `s` sits, a trail from home through its category's builds,
/// after a link back to `from`, the listing the viewer came from, when it
/// is somewhere else, and a search of every build, so a build found leads
/// back to the rest.
fn crumbs(s: &Build, from: Option<&str>) -> Markup {
    // A category off the list is among the other builds.
    let (category, plural) = match s.kind() {
        Some(kind) if s.kind().is_some() => {
            (href::category(&s.category), kind.plural())
        }
        _ => (href::category("other"), "Other builds".to_owned()),
    };
    let back = from.filter(|&from| from != "/" && from != category);
    html! {
        div class="mb-4 flex flex-wrap items-center justify-between gap-x-6 gap-y-3" {
            // The way back first, pointing back, apart from the trail.
            nav aria-label="Breadcrumb" class="flex min-w-0 items-center gap-3 text-meta text-muted" {
                @if let Some(back) = back {
                    a href=(back) data-back class="flex min-w-0 items-center gap-1.5 text-muted hover:text-ink" {
                        (icon(icons::ARROW_LEFT, "size-3.5 shrink-0"))
                        span class="truncate" { (back::label(back)) }
                    }
                    span aria-hidden="true" class="h-4 shrink-0 border-l border-boundary" {}
                }
                ol class="flex shrink-0 items-center gap-1" {
                    li { a href="/" class="text-muted hover:text-ink" { "Home" } }
                    li aria-hidden="true" class="flex" { (icon(icons::CHEVRON_RIGHT, "size-3.5")) }
                    li { a href=(category) class="text-muted hover:text-ink" { (plural) } }
                }
            }
            form action="/" method="get" role="search" class="w-full sm:w-72" {
                label for="search" class="sr-only" { "Search builds" }
                input id="search" type="search" name="q" placeholder="Search builds"
                    autocomplete="off" class="input";
            }
        }
    }
}

/// A choice of the colours the build at `path` shows in: the site's own,
/// its author's when kept, and the viewer's theme when they have one;
/// nothing when the site's are the only ones. Each keeps `from`, where the
/// viewer came from, as the page it links to has the build's as its
/// referrer.
fn colours_switch(
    path: &str,
    shown: ColoursShown,
    from: Option<&str>,
) -> Markup {
    let from = from.map_or(String::new(), |from| {
        format!(
            "&from={}",
            form_urlencoded::byte_serialize(from.as_bytes())
                .collect::<String>()
        )
    });
    let options = [
        (Colours::Default, "Default", true),
        (Colours::Original, "Original", shown.original),
        (Colours::Theme, "Your theme", shown.theme),
    ];
    html! {
        @if shown.original || shown.theme {
            nav aria-label="Colours" class="mb-3 flex flex-wrap items-center gap-x-3 gap-y-1" {
                span class="text-meta font-semibold text-muted" aria-hidden="true" { "Colours" }
                div class=(SWITCHES) {
                    @for (colours, label, offered) in options {
                        @if offered {
                            a href={ (path) "?colours=" (colours.name()) (from) } class=(SWITCH)
                                aria-current=[(colours == shown.chosen).then_some("page")] { (label) }
                        }
                    }
                }
            }
        }
    }
}

/// The builds of its category a build is similar to, the most alike first.
pub(crate) fn similar_section(similar: &[(Build, u32)]) -> Markup {
    html! {
        section aria-labelledby="similar-heading" {
            h2 id="similar-heading" class={ (SECTION_TITLE) " mb-1" } { "Similar builds" }
            p class="mb-3 text-meta text-muted" {
                "Share of their redstone that lines up, turned or mirrored as need be."
            }
            ul class="flex flex-col gap-1.5" {
                @for (other, permille) in similar {
                    li class="flex items-baseline justify-between gap-4" {
                        span {
                            a href=(href::build(&other.id)) { (other.title()) }
                            span class="text-muted" {
                                @if !other.credits.authors.is_empty() { " by " (other.credits.authors.join(", ")) }
                                ", " (size(other))
                            }
                        }
                        span class="tabular-nums text-muted whitespace-nowrap" { (percent(*permille)) }
                    }
                }
            }
        }
    }
}

/// How a link to build `s`'s page shows where it is posted, as in Discord:
/// its name, authors, size, and blocks as the title; its latency, how it
/// stacks, the oldest release it works in, and whether it is verified as
/// the line under it; and its picture.
pub(crate) fn preview(s: &Build) -> layout::Preview {
    let by = if s.credits.authors.is_empty() {
        String::new()
    } else {
        format!(" by {}", s.credits.authors.join(", "))
    };
    let title = format!(
        "{}{by} · {} H×W×D, {} blocks",
        s.name(),
        summary::size(s),
        s.volume()
    );
    let mut facts = vec![s.category_label()];
    if let Some(t) = s.specs.latency {
        facts.push(text::ticks(t));
    }
    if let Some(pitch) = s.stacking.iter().filter_map(|d| d.pitch).min() {
        facts.push(format!("{pitch}-wide stackable"));
    }
    if let Some(from) = &s.file.works_from {
        facts.push(format!("works from {from}"));
    }
    facts.push(
        if s.verified() {
            "verified"
        } else if s.untested() {
            "untested"
        } else {
            "failing"
        }
        .to_owned(),
    );
    layout::Preview {
        title,
        description: facts.join(" · "),
        image: Some(cards::thumbnail_src(s, None)),
    }
}
