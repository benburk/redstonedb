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
    /// Inside the corner markers.
    pub size: (i64, i64, i64),
    pub author: Option<String>,
    pub built_on: Option<String>,
    pub source_name: String,
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
        "SELECT id, function, width, orientation, encoding, latency,
            size_x, size_y, size_z, author, built_on, source_name, added_at
         FROM schematic ORDER BY function, width, added_at"
    )
    .fetch_all(db)
    .await?;
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
            id: row.id,
            function: row.function,
            width: row.width,
            orientation: row.orientation,
            encoding: row.encoding,
            latency: row.latency,
            size: (row.size_x, row.size_y, row.size_z),
            author: row.author,
            built_on: row.built_on,
            source_name: row.source_name,
            added_at: row.added_at,
        })
        .collect())
}

pub async fn get(
    db: &SqlitePool,
    id: &str,
) -> Result<Option<Schematic>, sqlx::Error> {
    let Some(row) = sqlx::query!(
        "SELECT id, function, width, orientation, encoding, latency,
            size_x, size_y, size_z, author, built_on, source_name, added_at
         FROM schematic WHERE id = ?",
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
        size: (row.size_x, row.size_y, row.size_z),
        author: row.author,
        built_on: row.built_on,
        source_name: row.source_name,
        added_at: row.added_at,
        tags,
    }))
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
    pub size: (i64, i64, i64),
    pub author: Option<&'a str>,
    pub built_on: Option<&'a str>,
    pub source_name: &'a str,
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
            latency, size_x, size_y, size_z, author, built_on, source_name,
            sha256, data)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         RETURNING id as "id!""#,
        new.function,
        new.width,
        new.orientation,
        new.encoding,
        new.latency,
        x,
        y,
        z,
        new.author,
        new.built_on,
        new.source_name,
        new.sha256,
        new.data,
    )
    .fetch_one(db)
    .await
}

/// Fields to change; `None` keeps the stored value.
#[derive(Default)]
pub struct Changes<'a> {
    pub function: Option<&'a str>,
    pub width: Option<i64>,
    pub orientation: Option<&'a str>,
    pub encoding: Option<&'a str>,
    pub latency: Option<i64>,
    pub author: Option<&'a str>,
    pub built_on: Option<&'a str>,
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
            author = coalesce(?, author),
            built_on = coalesce(?, built_on)
         WHERE id = ?",
        changes.function,
        changes.width,
        changes.orientation,
        changes.encoding,
        changes.latency,
        changes.author,
        changes.built_on,
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
    Author,
    BuiltOn,
}

/// Makes `fields` unknown.
pub async fn forget<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    fields: &[Optional],
) -> Result<(), sqlx::Error> {
    let (width, latency, author, built_on) = (
        fields.contains(&Optional::Width),
        fields.contains(&Optional::Latency),
        fields.contains(&Optional::Author),
        fields.contains(&Optional::BuiltOn),
    );
    sqlx::query!(
        "UPDATE schematic SET
            width = iif(?, NULL, width),
            latency = iif(?, NULL, latency),
            author = iif(?, NULL, author),
            built_on = iif(?, NULL, built_on)
         WHERE id = ?",
        width,
        latency,
        author,
        built_on,
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
            size: (19, 11, 18),
            author: Some("embizone".to_owned()),
            built_on: Some("2014-09-06".to_owned()),
            source_name: String::new(),
            added_at: String::new(),
            tags: vec![
                tag("carry_lookahead", None, "other"),
                tag("alpha", None, "restriction"),
                tag("unsynchronized", Some("unsync"), "timing"),
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
            author: None,
            built_on: None,
            tags: Vec::new(),
            ..cla()
        };
        assert_eq!(s.title(), "adder");
        assert_eq!(s.filename(), "adder-horz-bin-19x11x18.schem");
    }
}
