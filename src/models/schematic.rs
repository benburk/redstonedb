use std::collections::HashMap;

use sqlx::{SqliteExecutor, SqlitePool};

/// A tag's name is written in full words joined by underscores, such as
/// `carry_lookahead`.
#[derive(Debug, Clone)]
pub struct Tag {
    pub name: String,
    /// The filename abbreviation, when the name is too long for one.
    pub token: Option<String>,
    pub category: String,
}

impl Tag {
    /// The name as words, such as `carry lookahead`.
    pub fn label(&self) -> String {
        self.name.replace('_', " ")
    }

    /// How a filename writes this tag.
    pub fn token(&self) -> &str {
        self.token.as_deref().unwrap_or(&self.name)
    }
}

/// The form of a tag name a person types, with spaces for underscores.
pub fn tag_name(input: &str) -> String {
    input.trim().to_lowercase().replace(' ', "_")
}

/// One direction a stackable build repeats along. Each copy sits `west`,
/// `up`, and `north` blocks from the one before, since copies count up from
/// the south-east-bottom corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackDirection {
    pub west: i64,
    pub up: i64,
    pub north: i64,
    /// The most copies that work; `None` for no limit.
    pub max_copies: Option<i64>,
}

impl StackDirection {
    fn parts(&self) -> impl Iterator<Item = (i64, &'static str)> {
        [(self.west, "west"), (self.up, "up"), (self.north, "north")]
            .into_iter()
            .filter(|(n, _)| *n > 0)
    }

    /// The way copies go, such as `west` or `west-up`. It names the
    /// direction in a download link.
    pub fn name(&self) -> String {
        self.parts()
            .map(|(_, way)| way)
            .collect::<Vec<_>>()
            .join("-")
    }

    /// Such as `2 west per copy, up to 7 copies`.
    pub fn describe(&self) -> String {
        let steps: Vec<String> =
            self.parts().map(|(n, way)| format!("{n} {way}")).collect();
        let limit = match self.max_copies {
            Some(n) => format!("up to {n} copies"),
            None => "any number of copies".to_owned(),
        };
        format!("{} per copy, {limit}", steps.join(" and "))
    }

    /// Whether copies this way add bits, as along the way the build's bits
    /// already run, rather than add parallel copies such as more registers.
    pub fn adds_bits(&self, orientation: &str) -> bool {
        match orientation {
            "horz" => self.west > 0 && self.up == 0 && self.north == 0,
            "vert" => self.up > 0 && self.west == 0 && self.north == 0,
            "diag" => self.west > 0 && self.up > 0 && self.north == 0,
            _ => false,
        }
    }
}

impl std::str::FromStr for StackDirection {
    type Err = String;

    /// Parses `west=2`, `north=4,max=3`, or `west=1,up=1`.
    fn from_str(text: &str) -> Result<Self, String> {
        let mut direction = StackDirection {
            west: 0,
            up: 0,
            north: 0,
            max_copies: None,
        };
        for pair in text.split(',') {
            let (key, value) = pair
                .split_once('=')
                .ok_or_else(|| format!("expected key=value, got `{pair}`"))?;
            let value: i64 = value
                .trim()
                .parse()
                .map_err(|_| format!("`{value}` is not a whole number"))?;
            match key.trim() {
                "west" => direction.west = value,
                "up" => direction.up = value,
                "north" => direction.north = value,
                "max" => direction.max_copies = Some(value),
                other => {
                    return Err(format!(
                        "unknown key `{other}`; use west, up, north, or max"
                    ));
                }
            }
        }
        if direction.west + direction.up + direction.north <= 0 {
            return Err("give a step west, up, or north".to_owned());
        }
        Ok(direction)
    }
}

/// A stored schematic's metadata. The `.schem` bytes stay in the database
/// until [`data`] fetches them.
#[derive(Debug, Clone)]
pub struct Schematic {
    pub id: String,
    pub function: String,
    pub width: Option<i64>,
    pub orientation: String,
    pub encoding: String,
    pub latency: Option<i64>,
    /// Whether every output bit settles on the same tick.
    pub synchronized: Option<bool>,
    /// Inside the corner markers.
    pub size: (i64, i64, i64),
    /// Empty for a build that does not stack.
    pub stacking: Vec<StackDirection>,
    /// Whether the build follows the building conventions, so the lint and
    /// the interface reader apply to it.
    pub conventional: bool,
    pub author: Option<String>,
    pub built_on: Option<String>,
    /// The in-game name of whoever imported the build.
    pub imported_by: Option<String>,
    /// Of the stored file, so a URL derived from the file can name its
    /// version.
    pub sha256: String,
    pub added_at: String,
    /// Sorted by category in glossary order (restriction, timing, other),
    /// then name.
    pub tags: Vec<Tag>,
}

impl Schematic {
    pub fn tags_in<'a>(
        &'a self,
        category: &'a str,
    ) -> impl Iterator<Item = &'a Tag> + 'a {
        self.tags.iter().filter(move |tag| tag.category == category)
    }

    /// The width in words, such as `8 bits` or `1 bit`.
    pub fn width_label(&self) -> Option<String> {
        self.width
            .map(|w| format!("{w} {}", if w == 1 { "bit" } else { "bits" }))
    }

    /// The width and function, such as `8-bit adder`, for views that show
    /// the tags beside it.
    pub fn name(&self) -> String {
        let function = self.function.replace('_', " ");
        match self.width {
            Some(w) => format!("{w}-bit {function}"),
            None => function,
        }
    }

    /// A readable name from the most identifying facts, such as
    /// `8-bit alpha carry lookahead adder`.
    pub fn title(&self) -> String {
        self.width
            .map(|w| format!("{w}-bit"))
            .into_iter()
            .chain(self.tags_in("restriction").map(Tag::label))
            .chain(self.tags_in("other").map(Tag::label))
            .chain([self.function.replace('_', " ")])
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The download filename, following `naming.md`. Unknown parts are left
    /// out.
    pub fn filename(&self) -> String {
        let (x, y, z) = self.size;
        let parts: Vec<String> = std::iter::once(self.function.clone())
            .chain(self.width.map(|w| format!("{w:02}b")))
            .chain(self.tags_in("restriction").map(|t| t.token().to_owned()))
            .chain([self.orientation.clone(), self.encoding.clone()])
            .chain(self.tags_in("timing").map(|t| t.token().to_owned()))
            .chain(
                self.synchronized
                    .map(|s| (if s { "sync" } else { "unsync" }).to_owned()),
            )
            .chain(self.latency.map(|t| format!("{t}t")))
            .chain([format!("{x}x{y}x{z}")])
            .chain(self.author.clone())
            .chain(self.built_on.as_ref().map(|d| d.replace('-', "")))
            .collect();
        format!("{}.schem", parts.join("-"))
    }

    /// The number of blocks inside the build's bounding box.
    pub fn volume(&self) -> i64 {
        let (x, y, z) = self.size;
        x * y * z
    }

    /// This build stacked into a box of inner `size`, with `copies` along
    /// each direction. Copies that add bits widen it. A stack's latency and
    /// timing are not its slice's, so they are unknown.
    #[must_use]
    pub fn stacked(
        &self,
        copies: &[(StackDirection, usize)],
        size: [i32; 3],
    ) -> Schematic {
        let factor: i64 = copies
            .iter()
            .filter(|(direction, _)| direction.adds_bits(&self.orientation))
            .map(|(_, n)| i64::try_from(*n).unwrap_or(i64::MAX))
            .product();
        Schematic {
            width: self.width.map(|w| w * factor),
            latency: None,
            synchronized: None,
            size: (size[0].into(), size[1].into(), size[2].into()),
            ..self.clone()
        }
    }

    /// `yes`, `no`, or `unknown`.
    pub fn synchronized_label(&self) -> &str {
        match self.synchronized {
            Some(true) => "yes",
            Some(false) => "no",
            None => "unknown",
        }
    }

    pub fn orientation_label(&self) -> &str {
        match self.orientation.as_str() {
            "horz" => "horizontal",
            "vert" => "vertical",
            "diag" => "diagonal",
            other => other,
        }
    }

    pub fn encoding_label(&self) -> &str {
        match self.encoding.as_str() {
            "bin" => "binary",
            "hex" => "signal strength",
            other => other,
        }
    }
}

/// Every schematic, ordered by function, width, then insertion.
pub async fn all(db: &SqlitePool) -> Result<Vec<Schematic>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, function, width, orientation, encoding, latency,
            synchronized as "synchronized: bool", size_x, size_y, size_z, author, built_on, imported_by, sha256, added_at,
            conventional as "conventional: bool"
         FROM schematic ORDER BY function, width, added_at"#
    )
    .fetch_all(db)
    .await?;
    let mut stacking: HashMap<String, Vec<StackDirection>> = HashMap::new();
    for row in sqlx::query!(
        "SELECT schematic_id, west, up, north, max_copies
         FROM stack_direction ORDER BY west DESC, up DESC, north DESC"
    )
    .fetch_all(db)
    .await?
    {
        stacking
            .entry(row.schematic_id)
            .or_default()
            .push(StackDirection {
                west: row.west,
                up: row.up,
                north: row.north,
                max_copies: row.max_copies,
            });
    }
    let mut tags: HashMap<String, Vec<Tag>> = HashMap::new();
    for row in sqlx::query!(
        "SELECT schematic_tag.schematic_id, tag.name, tag.token, tag.category
         FROM schematic_tag JOIN tag ON tag.id = schematic_tag.tag_id
         ORDER BY tag.category = 'other', tag.category, tag.name"
    )
    .fetch_all(db)
    .await?
    {
        tags.entry(row.schematic_id).or_default().push(Tag {
            name: row.name,
            token: row.token,
            category: row.category,
        });
    }
    Ok(rows
        .into_iter()
        .map(|row| Schematic {
            tags: tags.remove(&row.id).unwrap_or_default(),
            stacking: stacking.remove(&row.id).unwrap_or_default(),
            id: row.id,
            function: row.function,
            width: row.width,
            orientation: row.orientation,
            encoding: row.encoding,
            latency: row.latency,
            synchronized: row.synchronized,
            size: (row.size_x, row.size_y, row.size_z),
            author: row.author,
            built_on: row.built_on,
            imported_by: row.imported_by,
            conventional: row.conventional,
            sha256: row.sha256,
            added_at: row.added_at,
        })
        .collect())
}

pub async fn get(
    db: &SqlitePool,
    id: &str,
) -> Result<Option<Schematic>, sqlx::Error> {
    let Some(row) = sqlx::query!(
        r#"SELECT id, function, width, orientation, encoding, latency,
            synchronized as "synchronized: bool", size_x, size_y, size_z, author, built_on, imported_by, sha256, added_at,
            conventional as "conventional: bool"
         FROM schematic WHERE id = ?"#,
        id
    )
    .fetch_optional(db)
    .await?
    else {
        return Ok(None);
    };
    let tags = sqlx::query_as!(
        Tag,
        "SELECT tag.name, tag.token, tag.category
         FROM schematic_tag JOIN tag ON tag.id = schematic_tag.tag_id
         WHERE schematic_tag.schematic_id = ?
         ORDER BY tag.category = 'other', tag.category, tag.name",
        id
    )
    .fetch_all(db)
    .await?;
    Ok(Some(Schematic {
        id: row.id,
        function: row.function,
        width: row.width,
        orientation: row.orientation,
        encoding: row.encoding,
        latency: row.latency,
        synchronized: row.synchronized,
        size: (row.size_x, row.size_y, row.size_z),
        stacking: stacking(db, id).await?,
        author: row.author,
        built_on: row.built_on,
        imported_by: row.imported_by,
        conventional: row.conventional,
        sha256: row.sha256,
        added_at: row.added_at,
        tags,
    }))
}

/// The directions a build stacks along: west first, then up, then north.
pub async fn stacking<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
) -> Result<Vec<StackDirection>, sqlx::Error> {
    sqlx::query_as!(
        StackDirection,
        "SELECT west, up, north, max_copies FROM stack_direction
         WHERE schematic_id = ? ORDER BY west DESC, up DESC, north DESC",
        id
    )
    .fetch_all(db)
    .await
}

/// Replaces the directions a build stacks along; none makes it unstackable.
pub async fn set_stacking(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
    directions: &[StackDirection],
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM stack_direction WHERE schematic_id = ?", id)
        .execute(&mut *tx)
        .await?;
    for d in directions {
        sqlx::query!(
            "INSERT INTO stack_direction (schematic_id, west, up, north, max_copies)
             VALUES (?, ?, ?, ?, ?)",
            id,
            d.west,
            d.up,
            d.north,
            d.max_copies,
        )
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

/// The stored `.schem` bytes.
pub async fn data<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
) -> Result<Option<Vec<u8>>, sqlx::Error> {
    sqlx::query_scalar!("SELECT data FROM schematic WHERE id = ?", id)
        .fetch_optional(db)
        .await
}

/// Ids starting with `prefix`; at most two, enough to detect ambiguity.
pub async fn ids_with_prefix<'e, E: SqliteExecutor<'e>>(
    db: E,
    prefix: &str,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM schematic WHERE id GLOB ? || '*' LIMIT 2",
        prefix
    )
    .fetch_all(db)
    .await
}

pub async fn id_by_sha256<'e, E: SqliteExecutor<'e>>(
    db: E,
    sha256: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!("SELECT id FROM schematic WHERE sha256 = ?", sha256)
        .fetch_optional(db)
        .await
}

pub struct New<'a> {
    pub function: &'a str,
    pub width: Option<i64>,
    pub orientation: &'a str,
    pub encoding: &'a str,
    pub latency: Option<i64>,
    pub synchronized: Option<bool>,
    pub size: (i64, i64, i64),
    pub author: Option<&'a str>,
    pub built_on: Option<&'a str>,
    pub imported_by: Option<&'a str>,
    pub conventional: bool,
    pub sha256: &'a str,
    pub data: &'a [u8],
}

/// Inserts a schematic and returns its generated id.
pub async fn insert<'e, E: SqliteExecutor<'e>>(
    db: E,
    new: &New<'_>,
) -> Result<String, sqlx::Error> {
    let (x, y, z) = new.size;
    sqlx::query_scalar!(
        r#"INSERT INTO schematic (function, width, orientation, encoding,
            latency, synchronized, size_x, size_y, size_z, author, built_on,
            imported_by, conventional, sha256, data)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         RETURNING id as "id!""#,
        new.function,
        new.width,
        new.orientation,
        new.encoding,
        new.latency,
        new.synchronized,
        x,
        y,
        z,
        new.author,
        new.built_on,
        new.imported_by,
        new.conventional,
        new.sha256,
        new.data,
    )
    .fetch_one(db)
    .await
}

/// Deletes a schematic and its tag links.
pub async fn delete<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM schematic WHERE id = ?", id)
        .execute(db)
        .await?;
    Ok(())
}

/// Fields to change; `None` keeps the stored value.
#[derive(Default)]
pub struct Changes<'a> {
    pub function: Option<&'a str>,
    pub width: Option<i64>,
    pub orientation: Option<&'a str>,
    pub encoding: Option<&'a str>,
    pub latency: Option<i64>,
    pub synchronized: Option<bool>,
    pub author: Option<&'a str>,
    pub built_on: Option<&'a str>,
    pub imported_by: Option<&'a str>,
    pub conventional: Option<bool>,
}

pub async fn update<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    changes: &Changes<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE schematic SET
            function = coalesce(?, function),
            width = coalesce(?, width),
            orientation = coalesce(?, orientation),
            encoding = coalesce(?, encoding),
            latency = coalesce(?, latency),
            synchronized = coalesce(?, synchronized),
            author = coalesce(?, author),
            built_on = coalesce(?, built_on),
            imported_by = coalesce(?, imported_by),
            conventional = coalesce(?, conventional)
         WHERE id = ?",
        changes.function,
        changes.width,
        changes.orientation,
        changes.encoding,
        changes.latency,
        changes.synchronized,
        changes.author,
        changes.built_on,
        changes.imported_by,
        changes.conventional,
        id,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// The tag named `name`, which may also be its filename token.
/// A field that may be unknown.
#[derive(PartialEq, Eq)]
pub enum Optional {
    Width,
    Latency,
    Synchronized,
    Author,
    BuiltOn,
    ImportedBy,
}

/// Makes `fields` unknown.
pub async fn forget<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    fields: &[Optional],
) -> Result<(), sqlx::Error> {
    let has = |field| fields.contains(&field);
    let (width, latency, synchronized, author, built_on) = (
        has(Optional::Width),
        has(Optional::Latency),
        has(Optional::Synchronized),
        has(Optional::Author),
        has(Optional::BuiltOn),
    );
    let imported_by = has(Optional::ImportedBy);
    sqlx::query!(
        "UPDATE schematic SET
            width = iif(?, NULL, width),
            latency = iif(?, NULL, latency),
            synchronized = iif(?, NULL, synchronized),
            author = iif(?, NULL, author),
            built_on = iif(?, NULL, built_on),
            imported_by = iif(?, NULL, imported_by)
         WHERE id = ?",
        width,
        latency,
        synchronized,
        author,
        built_on,
        imported_by,
        id,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// The tag named `name`, which may also be its filename token.
pub async fn tag_id<'e, E: SqliteExecutor<'e>>(
    db: E,
    name: &str,
) -> Result<Option<String>, sqlx::Error> {
    let name = tag_name(name);
    sqlx::query_scalar!(
        "SELECT id FROM tag WHERE name = ? OR token = ?",
        name,
        name
    )
    .fetch_optional(db)
    .await
}

pub async fn attach<'e, E: SqliteExecutor<'e>>(
    db: E,
    schematic_id: &str,
    tag_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT OR IGNORE INTO schematic_tag (schematic_id, tag_id)
         VALUES (?, ?)",
        schematic_id,
        tag_id
    )
    .execute(db)
    .await?;
    Ok(())
}

pub async fn detach<'e, E: SqliteExecutor<'e>>(
    db: E,
    schematic_id: &str,
    tag_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM schematic_tag WHERE schematic_id = ? AND tag_id = ?",
        schematic_id,
        tag_id
    )
    .execute(db)
    .await?;
    Ok(())
}

pub async fn all_tags<'e, E: SqliteExecutor<'e>>(
    db: E,
) -> Result<Vec<Tag>, sqlx::Error> {
    sqlx::query_as!(
        Tag,
        "SELECT name, token, category FROM tag
         ORDER BY category = 'other', category, name"
    )
    .fetch_all(db)
    .await
}

pub async fn define_tag<'e, E: SqliteExecutor<'e>>(
    db: E,
    name: &str,
    token: Option<&str>,
    category: &str,
) -> Result<(), sqlx::Error> {
    let name = tag_name(name);
    sqlx::query!(
        "INSERT INTO tag (name, token, category) VALUES (?, ?, ?)",
        name,
        token,
        category
    )
    .execute(db)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(name: &str, token: Option<&str>, category: &str) -> Tag {
        Tag {
            name: name.to_owned(),
            token: token.map(str::to_owned),
            category: category.to_owned(),
        }
    }

    fn cla() -> Schematic {
        Schematic {
            id: String::new(),
            function: "adder".to_owned(),
            width: Some(8),
            orientation: "horz".to_owned(),
            encoding: "bin".to_owned(),
            latency: Some(8),
            synchronized: Some(false),
            size: (19, 11, 18),
            stacking: Vec::new(),
            author: Some("embizone".to_owned()),
            built_on: Some("2014-09-06".to_owned()),
            imported_by: None,
            conventional: true,
            sha256: String::new(),
            added_at: String::new(),
            tags: vec![
                tag("carry_lookahead", None, "other"),
                tag("alpha", None, "restriction"),
            ],
        }
    }

    #[test]
    fn title_reads_width_restrictions_other_tags_then_function() {
        assert_eq!(cla().title(), "8-bit alpha carry lookahead adder");
    }

    #[test]
    fn filename_writes_tag_tokens() {
        assert_eq!(
            cla().filename(),
            "adder-08b-alpha-horz-bin-unsync-8t-19x11x18-embizone-20140906.schem"
        );
    }

    #[test]
    fn unknown_parts_are_left_out() {
        let s = Schematic {
            width: None,
            latency: None,
            synchronized: None,
            author: None,
            built_on: None,
            tags: Vec::new(),
            ..cla()
        };
        assert_eq!(s.title(), "adder");
        assert_eq!(s.filename(), "adder-horz-bin-19x11x18.schem");
    }
}
