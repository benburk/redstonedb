//! A build's details form: what it starts with, and reading what it
//! sends, for editing and uploading.

use axum::response::{IntoResponse, Response};
use sqlx::SqlitePool;

use super::pages::{self, Choices, DetailsPage, File, Reply};
use crate::{
    category::Category,
    details::Details,
    import::{self, Analysis, FileFacts},
    models::{
        schematic::{self, Orientation, Schematic, StackDirection},
        tag,
        user::User,
    },
    recognize::interface::{self, Interface},
    schem::{Blocks, Size},
    simulate::Guess,
    web::error::AppError,
};

/// The ways the form can stack a build, as `StackDirection::name` gives
/// them, with their labels.
pub const STACK_DIRECTIONS: [(&str, &str); 3] =
    [("west", "West"), ("up", "Up"), ("west-up", "West and up")];

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
            step: direction.west.max(direction.up).to_string(),
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
        let (west, up) = match self.direction.as_str() {
            "west" => (step, 0),
            "up" => (0, step),
            "west-up" => (step, step),
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
            max_copies,
            faces: None,
            pitch: None,
        }))
    }
}

/// A details form's values, every field as text; empty means unknown.
#[derive(Clone, Default)]
pub struct Form {
    /// The category as typed: a standard one by its name or label, in any
    /// capitals, or else a name of the author's own.
    pub category: String,
    /// An [`Orientation`]'s name.
    pub orientation: String,
    /// In bits, typed for a build unverified as its category, whose file
    /// may not show it.
    pub width: String,
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
    /// In order, from the author pills and anything typed beside them.
    pub authors: Vec<String>,
    /// Likewise, from the contributor pills.
    pub contributors: Vec<String>,
    /// From the tag pills and anything typed beside them.
    pub tags: Vec<String>,
    /// Whether anyone may see the build, or only its uploader.
    pub public: bool,
}

/// The date the date field's boxes give, such as `2024-02-02`, a single
/// digit month or day taken as if it led with a zero; empty when all are.
fn date(year: &str, month: &str, day: &str) -> String {
    let parts = [year, month, day].map(str::trim);
    if parts.iter().all(|part| part.is_empty()) {
        return String::new();
    }
    let pad = |part: &str| {
        if part.len() == 1 {
            format!("0{part}")
        } else {
            part.to_owned()
        }
    };
    format!("{}-{}-{}", parts[0], pad(parts[1]), pad(parts[2]))
}

impl Form {
    /// Reads a urlencoded body by hand, since the pill fields repeat one
    /// name per value. `author_entry`, `contributor_entry`, and `tag_entry`
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
            width: get("width"),
            latency: get("latency"),
            synchronized: get("synchronized_unknown")
                .is_empty()
                .then(|| !get("synchronized").is_empty()),
            // In three boxes from the date field, or whole from elsewhere.
            built_on: if pairs.iter().any(|(k, _)| k == "built_on_year") {
                date(
                    &get("built_on_year"),
                    &get("built_on_month"),
                    &get("built_on_day"),
                )
            } else {
                get("built_on")
            },
            description: get("description"),
            logic: get("logic"),
            stackable: !get("stackable").is_empty(),
            stack_rows,
            authors: all("author", "author_entry"),
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
            width: number(s.width),
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
            authors: s.authors.clone(),
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
            width: or(self.width, suggested.width),
            latency: or(self.latency, suggested.latency),
            synchronized: self.synchronized.or(suggested.synchronized),
            built_on: or(self.built_on, suggested.built_on),
            description: or(self.description, suggested.description),
            logic: or(self.logic, suggested.logic),
            stackable: self.stackable,
            stack_rows,
            authors: self.authors,
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
        let number = |text: &str, what: &str| {
            let text = text.trim();
            (!text.is_empty())
                .then(|| {
                    text.parse()
                        .map_err(|_| format!("{what} must be a whole number."))
                })
                .transpose()
        };
        let latency = number(&self.latency, "Latency")?;
        let width = number(&self.width, "Width")?;
        let text = |value: &str| Some(value.to_owned());
        Ok(Details {
            category: self.category.clone(),
            orientation: self.orientation.parse()?,
            width,
            latency,
            synchronized: self.synchronized,
            built_on: text(&self.built_on),
            description: text(&self.description),
            logic: chosen(&self.logic)?,
            authors: self.authors.clone(),
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
        categories: Category::all(),
        people: schematic::all_people(db).await?,
        tags: tag::all(db).await?,
    })
}

/// What a details page shows beside its form: the file, and what it says
/// back to the last thing sent.
pub struct Shown<'a> {
    pub file: File<'a>,
    /// The file's blocks, when they can be read.
    pub blocks: Option<&'a Blocks>,
    pub reply: Reply<'a>,
}

/// The details page for `user`, its form as `form` has it. What every
/// details page shows beside the file is worked out here: the lists the
/// form chooses from, how the viewer's Schemati session stands, for the
/// slot their files load into, the file's inputs and outputs read for the
/// category shown, typed for an upload and stored for a stored build, and
/// what a category that reads the file, such as an adder, would take from
/// it.
pub async fn page(
    db: &SqlitePool,
    user: &User,
    form: &Form,
    shown: Shown<'_>,
) -> Result<Response, AppError> {
    let choices = choices(db).await?;
    let link = User::schemati_link(db, user.id).await?;
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
            reply: shown.reply,
            schemati: link.as_ref(),
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
            authors: vec!["Sam".to_owned()],
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
        assert_eq!(form.authors, ["Sam"]);
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
            &[("west", "2", "7"), ("up", "4", ""), ("west", "", "")],
        );
        let directions = on.stacking().unwrap();
        assert_eq!(directions.len(), 2);
        assert_eq!(
            (directions[0].west, directions[0].max_copies),
            (2, Some(7))
        );
        assert_eq!((directions[1].up, directions[1].max_copies), (4, None));
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
            vec![("north", "2", "")],
        ] {
            assert!(form(true, &rows).stacking().is_err(), "{rows:?}");
        }
    }

    #[test]
    fn a_stored_direction_shows_as_its_row() {
        let direction: StackDirection = "up=4,max=3".parse().unwrap();
        let row = StackRow::of(&direction);
        assert_eq!(
            (row.direction.as_str(), row.step.as_str(), row.max.as_str()),
            ("up", "4", "3")
        );
        assert!("north=4".parse::<StackDirection>().is_err());
    }

    /// The date field's three boxes make one date, a single-digit month
    /// or day gaining its zero, and empty boxes no date; a form without
    /// them still takes the date whole.
    #[test]
    fn the_date_boxes_make_one_date() {
        let pairs = |list: &[(&str, &str)]| -> Vec<(String, String)> {
            list.iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect()
        };
        let boxes = |y, m, d| {
            Form::read(&pairs(&[
                ("built_on_year", y),
                ("built_on_month", m),
                ("built_on_day", d),
            ]))
            .built_on
        };
        assert_eq!(boxes("2021", "5", "7"), "2021-05-07");
        assert_eq!(boxes("2024", "02", "29"), "2024-02-29");
        assert_eq!(boxes("", "", ""), "");
        assert_eq!(
            Form::read(&pairs(&[("built_on", "2013-09-11")])).built_on,
            "2013-09-11"
        );
    }
}
