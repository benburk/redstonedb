//! A build's details form: what it starts with, and reading what it
//! sends, for editing and uploading.

use axum::response::{IntoResponse, Response};
use sqlx::SqlitePool;

use super::pages::{self, Choices, DetailsPage, File};
use crate::{
    details::Details,
    error::AppError,
    import::{self, Analysis, FileFacts},
    interface::{self, Interface},
    models::{
        catalogue::{self, Category},
        schematic::{self, Orientation, Schematic, StackDirection},
        tag,
        user::User,
    },
    schem::{Blocks, Size},
    simulate::Guess,
};

/// The ways the form can stack a build, as `StackDirection::name` gives
/// them, with their labels.
pub const STACK_DIRECTIONS: [(&str, &str); 4] = [
    ("west", "West"),
    ("up", "Up"),
    ("north", "North"),
    ("west-up", "West and up"),
];

/// One stacking direction as the form shows it, every field as text.
#[derive(Clone, Default)]
pub struct StackRow {
    /// One of [`STACK_DIRECTIONS`], such as `west`.
    pub direction: String,
    /// Blocks from each copy to the next.
    pub step: String,
    /// The most copies that work; empty for no limit.
    pub max: String,
}

impl StackRow {
    pub fn of(direction: &StackDirection) -> Self {
        StackRow {
            direction: direction.name(),
            step: direction
                .west
                .max(direction.up)
                .max(direction.north)
                .to_string(),
            max: direction
                .max_copies
                .map(|n| n.to_string())
                .unwrap_or_default(),
        }
    }

    /// A likely first direction for a build whose bits run along
    /// `orientation`, with core `size`: copies placed its size apart the
    /// way the bits run. A diagonal build's step is left to the person.
    pub fn suggested(orientation: Orientation, size: (i64, i64, i64)) -> Self {
        let (direction, step) = match orientation {
            Orientation::Horizontal => ("west", size.0.to_string()),
            Orientation::Vertical => ("up", size.1.to_string()),
            Orientation::Diagonal => ("west-up", String::new()),
        };
        StackRow {
            direction: direction.to_owned(),
            step,
            max: String::new(),
        }
    }

    /// The direction this row describes; `None` when its step is empty.
    fn parse(&self) -> Result<Option<StackDirection>, String> {
        let step = self.step.trim();
        if step.is_empty() {
            return Ok(None);
        }
        let step: i64 = step
            .parse()
            .ok()
            .filter(|n| *n > 0)
            .ok_or("A stacking step must be a whole number of blocks.")?;
        let (west, up, north) = match self.direction.as_str() {
            "west" => (step, 0, 0),
            "up" => (0, step, 0),
            "north" => (0, 0, step),
            "west-up" => (step, step, 0),
            _ => return Err("Choose a stacking direction.".to_owned()),
        };
        let max = self.max.trim();
        let max_copies = if max.is_empty() {
            None
        } else {
            Some(max.parse().ok().filter(|n| *n >= 2).ok_or(
                "The most copies must be a whole number, 2 or more; leave \
                 it empty for no limit.",
            )?)
        };
        Ok(Some(StackDirection {
            west,
            up,
            north,
            max_copies,
            faces: None,
        }))
    }
}

/// A details form's values, every field as text; empty means unknown.
#[derive(Clone, Default)]
pub struct Form {
    /// The category as typed: a standard one by its name or label, in any
    /// capitals, or else a name of the builder's own.
    pub category: String,
    /// An [`Orientation`]'s name.
    pub orientation: String,
    pub latency: String,
    /// Whether the toggle is on; `None` while an unknown build's toggle is
    /// untouched, so saving keeps it unknown.
    pub synchronized: Option<bool>,
    /// Such as `2024-02-02`; empty when unknown.
    pub built_on: String,
    pub description: String,
    /// A [`schematic::Logic`]'s name; empty follows the category.
    pub logic: String,
    /// Whether the stacking toggle is on; the rows count only when it is.
    pub stackable: bool,
    /// The stacking directions so far, or a suggested first one.
    pub stack_rows: Vec<StackRow>,
    /// In order, from the builder pills and anything typed beside them.
    pub builders: Vec<String>,
    /// Likewise, from the contributor pills.
    pub contributors: Vec<String>,
    /// From the tag pills and anything typed beside them.
    pub tags: Vec<String>,
    /// Whether anyone may see the build, or only its uploader.
    pub public: bool,
}

impl Form {
    /// Reads a urlencoded body by hand, since the pill fields repeat one
    /// name per value. `builder_entry`, `contributor_entry`, and `tag_entry`
    /// are their text boxes, sent as typed when scripting is off.
    pub fn read(pairs: &[(String, String)]) -> Self {
        let get = |key: &str| first(pairs, key).to_owned();
        let each = |key: &str| -> Vec<String> {
            pairs
                .iter()
                .filter(|(k, _)| k == key)
                .map(|(_, v)| v.trim().to_owned())
                .collect()
        };
        let stack_rows = each("stack_direction")
            .into_iter()
            .zip(each("stack_step"))
            .zip(each("stack_max"))
            .map(|((direction, step), max)| StackRow {
                direction,
                step,
                max,
            })
            .collect();
        let all = |key: &str, entry: &str| -> Vec<String> {
            pairs
                .iter()
                .filter(|(k, v)| {
                    k == key || (k == entry && !v.trim().is_empty())
                })
                .map(|(_, v)| v.trim().to_owned())
                .collect()
        };
        Form {
            category: get("category"),
            orientation: get("orientation"),
            latency: get("latency"),
            synchronized: get("synchronized_unknown")
                .is_empty()
                .then(|| !get("synchronized").is_empty()),
            built_on: get("built_on"),
            description: get("description"),
            logic: get("logic"),
            stackable: !get("stackable").is_empty(),
            stack_rows,
            builders: all("builder", "builder_entry"),
            contributors: all("contributor", "contributor_entry"),
            tags: all("tag", "tag_entry")
                .iter()
                .map(|name| tag::name(name))
                .collect(),
            public: !get("public").is_empty(),
        }
    }

    /// Stored build `s`'s details, for its edit page.
    pub fn of(s: &Schematic) -> Self {
        let number =
            |n: Option<i64>| n.map(|n| n.to_string()).unwrap_or_default();
        let text = |t: &Option<String>| t.clone().unwrap_or_default();
        let name = |name: Option<&str>| name.unwrap_or_default().to_owned();
        Form {
            category: s.category.clone(),
            orientation: s.orientation.name().to_owned(),
            latency: number(s.latency),
            synchronized: s.synchronized,
            built_on: text(&s.built_on),
            description: text(&s.description),
            logic: name(s.own_logic.map(schematic::Logic::name)),
            stackable: !s.stacking.is_empty(),
            // A build that does not stack yet starts with a likely direction.
            stack_rows: if s.stacking.is_empty() {
                vec![StackRow::suggested(s.orientation, s.size)]
            } else {
                s.stacking.iter().map(StackRow::of).collect()
            },
            builders: s.builders.clone(),
            contributors: s.contributors.clone(),
            tags: s
                .tags
                .iter()
                .filter(|tag| !tag.is_derived())
                .map(|tag| tag.name.clone())
                .collect(),
            public: s.public,
        }
    }

    /// What a new upload's form starts with: the category the file seems
    /// to be, by `guess`, and the orientation its `interface` suggests,
    /// with a first stacking direction its `size` apart.
    pub fn suggested(
        guess: Option<&Guess>,
        interface: &Interface,
        size: &Size,
    ) -> Self {
        let category = guess.map_or("", |guess| guess.category).to_owned();
        let orientation = import::guess(interface).1.unwrap_or_default();
        let size = (size.x.into(), size.y.into(), size.z.into());
        Form {
            orientation: orientation.name().to_owned(),
            // Most builds are; the uploader says when one is not.
            synchronized: Some(true),
            stack_rows: vec![StackRow::suggested(orientation, size)],
            category,
            public: true,
            ..Form::default()
        }
    }

    /// The details for a newly chosen file: this form as sent, every
    /// field filled in kept, and only those left empty taken from
    /// `suggested`, what the new file suggests.
    pub fn keeping_sent(self, suggested: Form) -> Form {
        let or = |sent: String, suggested: String| {
            if sent.trim().is_empty() {
                suggested
            } else {
                sent
            }
        };
        let stack_rows = if self.stack_rows.iter().all(|r| r.step.is_empty()) {
            suggested.stack_rows
        } else {
            self.stack_rows
        };
        Form {
            category: or(self.category, suggested.category),
            orientation: or(self.orientation, suggested.orientation),
            latency: or(self.latency, suggested.latency),
            synchronized: self.synchronized.or(suggested.synchronized),
            built_on: or(self.built_on, suggested.built_on),
            description: or(self.description, suggested.description),
            logic: or(self.logic, suggested.logic),
            stackable: self.stackable,
            stack_rows,
            builders: self.builders,
            contributors: self.contributors,
            tags: self.tags,
            public: self.public,
        }
    }

    /// The stacking directions, checked: none unless the toggle is on, and
    /// then at least one, each way once.
    fn stacking(&self) -> Result<Vec<StackDirection>, String> {
        if !self.stackable {
            return Ok(Vec::new());
        }
        let mut directions: Vec<StackDirection> = Vec::new();
        for row in &self.stack_rows {
            if let Some(direction) = row.parse()? {
                if directions.iter().any(|d| d.name() == direction.name()) {
                    return Err("Give each stacking direction once.".to_owned());
                }
                directions.push(direction);
            }
        }
        if directions.is_empty() {
            return Err("Give a stacking step, or untick Stackable.".to_owned());
        }
        Ok(directions)
    }

    /// The details the text gives, or a sentence saying which field is not
    /// a number, one of its choices, or a stacking step. The rules they
    /// must keep are [`crate::details::check`]'s. What the form doesn't
    /// take, a listed build's probing and simulation work out, and a build
    /// off the list has none of.
    pub fn parse(&self) -> Result<Details, String> {
        let latency = self.latency.trim();
        let latency = (!latency.is_empty())
            .then(|| {
                latency
                    .parse()
                    .map_err(|_| "Latency must be a whole number.".to_owned())
            })
            .transpose()?;
        let text = |value: &str| Some(value.to_owned());
        Ok(Details {
            category: self.category.clone(),
            orientation: self.orientation.parse()?,
            latency,
            synchronized: self.synchronized,
            built_on: text(&self.built_on),
            description: text(&self.description),
            logic: chosen(&self.logic)?,
            builders: self.builders.clone(),
            contributors: self.contributors.clone(),
            stacking: self.stacking()?,
            tags: self.tags.clone(),
            ..Details::default()
        })
    }
}

/// The lists a details form chooses from or suggests.
pub async fn choices(db: &SqlitePool) -> Result<Choices, sqlx::Error> {
    Ok(Choices {
        categories: catalogue::categories(db).await?,
        people: schematic::all_people(db).await?,
        tags: tag::all(db).await?,
    })
}

/// What a details page shows beside its form: the file, and why the last
/// try was refused, if it was.
pub struct Shown<'a> {
    pub file: File<'a>,
    /// The file's blocks, when they can be read.
    pub blocks: Option<&'a Blocks>,
    /// Why the last attempt to publish or save was refused, if it was.
    pub message: &'a str,
    /// Whether the fields for choosing another file start open, with why
    /// the last file chosen could not be read, if it could not.
    pub choosing: Option<&'a str>,
}

/// The details page for `user`, its form as `form` has it. What every
/// details page shows beside the file is worked out here: the lists the
/// form chooses from, the viewer's Schemati folder, the file's inputs and
/// outputs read for the category shown, typed for an upload and stored for
/// a stored build, and what a category that reads the file, such as an
/// adder, would take from it.
pub async fn page(
    db: &SqlitePool,
    user: &User,
    form: &Form,
    shown: Shown<'_>,
) -> Result<Response, AppError> {
    let choices = choices(db).await?;
    let folder = super::schemati::own_folder_quickly(db, user).await?;
    let (category, conventional) = match &shown.file {
        File::Upload { unconventional, .. } => (
            Category::name_of(&choices.categories, &form.category),
            !unconventional,
        ),
        File::Stored { s, .. } | File::Version { s, .. } => {
            (s.category.clone(), s.conventional)
        }
    };
    let analysis = shown.blocks.map_or_else(Analysis::default, |blocks| {
        Analysis::of(blocks, &category)
    });
    // Whatever the category is now, in case it becomes one that reads
    // the file.
    let derived = shown
        .blocks
        .filter(|_| conventional)
        .map(|blocks| import::read_from_file(&interface::read(blocks)))
        .unwrap_or_default();
    // The tags the file gives the build, which no one may add or remove.
    let fixed = match (shown.blocks, &shown.file) {
        (Some(blocks), _) => tag::from_file(&FileFacts::of(blocks)),
        (None, File::Stored { s, .. } | File::Version { s, .. }) => s
            .tags
            .iter()
            .filter(|tag| {
                tag.is_derived() && tag.kind == tag::Kind::Restriction
            })
            .cloned()
            .collect(),
        (None, File::Upload { .. }) => Vec::new(),
    };
    Ok(pages::details_page(
        user,
        &DetailsPage {
            file: shown.file,
            analysis: &analysis,
            form,
            choices: &choices,
            derived,
            fixed: &fixed,
            message: shown.message,
            choosing: shown.choosing,
            schemati: &super::schemati::connection(folder.as_ref()),
        },
    )
    .into_response())
}

/// The value a menu sent, such as an encoding; `None` when it sent none.
fn chosen<T: std::str::FromStr<Err = String>>(
    value: &str,
) -> Result<Option<T>, String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.parse()).transpose()
}

/// The pairs of a urlencoded body.
pub fn pairs(body: &[u8]) -> Vec<(String, String)> {
    form_urlencoded::parse(body)
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

/// The first value a form sent under `key`, or empty.
pub fn first<'a>(pairs: &'a [(String, String)], key: &str) -> &'a str {
    pairs
        .iter()
        .find(|(k, _)| k == key)
        .map_or("", |(_, v)| v.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(stackable: bool, rows: &[(&str, &str, &str)]) -> Form {
        Form {
            stackable,
            stack_rows: rows
                .iter()
                .map(|&(direction, step, max)| StackRow {
                    direction: direction.to_owned(),
                    step: step.to_owned(),
                    max: max.to_owned(),
                })
                .collect(),
            ..Form::default()
        }
    }

    /// A new file keeps every detail filled in, and fills only the empty
    /// ones from what it suggests.
    #[test]
    fn a_new_file_keeps_the_details_sent() {
        let sent = Form {
            category: "adder".to_owned(),
            latency: "6".to_owned(),
            builders: vec!["Sam".to_owned()],
            description: "Fast.".to_owned(),
            tags: vec!["pistonless".to_owned()],
            ..Form::default()
        };
        let suggested = Form {
            category: "monostable".to_owned(),
            orientation: "vert".to_owned(),
            ..Form::default()
        };
        let form = sent.keeping_sent(suggested);
        assert_eq!(form.category, "adder");
        assert_eq!(form.latency, "6");
        assert_eq!(form.orientation, "vert");
        assert_eq!(form.builders, ["Sam"]);
        assert_eq!(form.description, "Fast.");
        assert_eq!(form.tags, ["pistonless"]);
    }

    #[test]
    fn stacking_counts_only_while_ticked() {
        let off = form(false, &[("west", "2", "")]);
        assert!(off.stacking().unwrap().is_empty());
    }

    #[test]
    fn stacking_reads_each_filled_row() {
        let on = form(
            true,
            &[("west", "2", "7"), ("north", "4", ""), ("west", "", "")],
        );
        let directions = on.stacking().unwrap();
        assert_eq!(directions.len(), 2);
        assert_eq!(
            (directions[0].west, directions[0].max_copies),
            (2, Some(7))
        );
        assert_eq!((directions[1].north, directions[1].max_copies), (4, None));
        let diagonal = form(true, &[("west-up", "1", "")]).stacking().unwrap();
        assert_eq!((diagonal[0].west, diagonal[0].up), (1, 1));
    }

    #[test]
    fn stacking_refuses_what_cannot_stack() {
        for rows in [
            vec![("west", "", "")],
            vec![("west", "0", "")],
            vec![("west", "2", "1")],
            vec![("west", "2", ""), ("west", "3", "")],
            vec![("sideways", "2", "")],
        ] {
            assert!(form(true, &rows).stacking().is_err(), "{rows:?}");
        }
    }

    #[test]
    fn a_stored_direction_shows_as_its_row() {
        let direction: StackDirection = "north=4,max=3".parse().unwrap();
        let row = StackRow::of(&direction);
        assert_eq!(
            (row.direction.as_str(), row.step.as_str(), row.max.as_str()),
            ("north", "4", "3")
        );
    }
}
