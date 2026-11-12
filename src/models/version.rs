//! Each file a build has had: its versions, numbered from 1, the highest
//! being the one it has now. This is the only place files are kept.

use sqlx::{SqliteExecutor, SqlitePool};
use uuid::Uuid;

use crate::{
    diff::Counts, import::Prepared, interface::Kept, models::user::REDSTONEDB,
};

/// One version, without its file.
pub struct Version {
    pub version: i64,
    pub added_at: String,
    /// The uploader as people know them: Minecraft name, else username.
    pub added_by: Option<String>,
    /// The uploader's username, which their profile's address takes.
    pub added_by_username: Option<String>,
    /// Whether `RedstoneDB` itself made it.
    pub by_site: bool,
    /// What it changed; `None` for the first.
    pub note: Option<String>,
    /// The version it restores, whose file it copies, if it does.
    pub restores: Option<i64>,
    /// What it measured, once tested; `None` before it was kept.
    pub measures: Option<Measures>,
    /// What it changed from the version before, once counted.
    pub changes: Option<Counts>,
}

/// What a version measured, kept with it so its list can say what each
/// version changed in its results, as those of a build are kept only for
/// its current file.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Measures {
    /// `passes`, `wrong`, `broken`, `skipped`, or `untested`.
    pub verdict: String,
    pub latency: Option<i64>,
    pub throughput: Option<i64>,
    /// Its size, as x, y, and z.
    pub size: [i64; 3],
    /// Its tightest stacking pitch, if it stacks at one.
    pub pitch: Option<i64>,
}

/// Build `id`'s versions, newest first.
pub async fn list<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
) -> Result<Vec<Version>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT version, build_version.added_at,
            coalesce(user.minecraft_name, user.username) as "added_by?: String",
            user.username as "added_by_username?: String",
            added_by as "added_by_id?: Uuid", note, restores, measures, changes
         FROM build_version LEFT JOIN user ON user.id = added_by
         WHERE build_id = ? ORDER BY version DESC"#,
        id
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| Version {
            version: row.version,
            added_at: row.added_at,
            added_by: row.added_by,
            added_by_username: row.added_by_username,
            by_site: row.added_by_id == Some(REDSTONEDB),
            note: row.note,
            restores: row.restores,
            measures: json(row.measures),
            changes: json(row.changes),
        })
        .collect())
}

/// The value JSON `text` holds, if it holds one.
fn json<T: serde::de::DeserializeOwned>(text: Option<String>) -> Option<T> {
    text.and_then(|text| serde_json::from_str(&text).ok())
}

/// Keeps `measures` as what build `id`'s current version measured.
pub async fn set_measures<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    measures: &Measures,
) -> Result<(), sqlx::Error> {
    let json = serde_json::to_string(measures).expect("measures are JSON");
    sqlx::query!(
        "UPDATE build_version SET measures = ?
         WHERE build_id = ? AND version = (
             SELECT max(version) FROM build_version AS newest
             WHERE newest.build_id = ?
         )",
        json,
        id,
        id
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Keeps `changes` as what build `id`'s version `version` changed from
/// the one before.
pub async fn set_changes<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    version: i64,
    changes: Counts,
) -> Result<(), sqlx::Error> {
    let json = serde_json::to_string(&changes).expect("counts are JSON");
    sqlx::query!(
        "UPDATE build_version SET changes = ?
         WHERE build_id = ? AND version = ?",
        json,
        id,
        version
    )
    .execute(db)
    .await?;
    Ok(())
}

/// The file of build `id`'s version `version`, carrying the roles kept
/// with it, as a restore copies it.
pub async fn file<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    version: i64,
) -> Result<Option<Vec<u8>>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT data, roles FROM build_version
         WHERE build_id = ? AND version = ?",
        id,
        version
    )
    .fetch_optional(db)
    .await?;
    row.map(|row| carrying(row.data, row.roles.as_deref()))
        .transpose()
}

/// The colours the author of build `id`'s version `version` gave its
/// building blocks, if any are kept with it.
pub async fn colours_of<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    version: i64,
) -> Result<Option<crate::recognize::colouring::Colouring>, sqlx::Error> {
    let json = sqlx::query_scalar!(
        "SELECT colours FROM build_version
         WHERE build_id = ? AND version = ?",
        id,
        version
    )
    .fetch_optional(db)
    .await?
    .flatten();
    Ok(json.and_then(|json| serde_json::from_str(&json).ok()))
}

/// The file of build `id`'s version `version`.
pub async fn data<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    version: i64,
) -> Result<Option<Vec<u8>>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT data FROM build_version
         WHERE build_id = ? AND version = ?",
        id,
        version
    )
    .fetch_optional(db)
    .await
}

/// Keeps `colours` as the colours the author of build `id`'s version
/// `version` gave its building blocks.
pub async fn set_colours<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    version: i64,
    colours: &crate::recognize::colouring::Colouring,
) -> Result<(), sqlx::Error> {
    let json =
        serde_json::to_string(colours).expect("positions and names are JSON");
    sqlx::query!(
        "UPDATE build_version SET colours = ? WHERE build_id = ? AND version = ?",
        json,
        id,
        version
    )
    .execute(db)
    .await?;
    Ok(())
}

/// The original colours of build `id`'s current file: those kept with the
/// newest version up to it that has any, while that version's file is the
/// current one's size, so a version the site makes from it, such as one
/// with its unused blocks taken out, keeps them.
pub async fn colours(
    db: &sqlx::SqlitePool,
    id: &str,
) -> Result<Option<crate::recognize::colouring::Colouring>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT data, colours FROM build_version
         WHERE build_id = ? ORDER BY version DESC",
        id
    )
    .fetch_all(db)
    .await?;
    let size =
        |data: &[u8]| crate::schem::Blocks::read(data).ok().map(|b| b.size);
    let Some(current) = rows.first().and_then(|row| size(&row.data)) else {
        return Ok(None);
    };
    Ok(rows
        .iter()
        .find(|row| row.colours.is_some())
        .filter(|row| size(&row.data) == Some(current))
        .and_then(|row| serde_json::from_str(row.colours.as_deref()?).ok()))
}

/// Build `id`'s current file, its highest version's, carrying the roles
/// kept with it, as every reading of its lines takes them; see
/// [`crate::schem::lines`]. A version whose roles are not yet found
/// carries none, and `simulation::refresh` finds them.
pub async fn current_file<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
) -> Result<Option<Vec<u8>>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT data as "data!", roles FROM current_file
         WHERE build_id = ?"#,
        id
    )
    .fetch_optional(db)
    .await?;
    row.map(|row| carrying(row.data, row.roles.as_deref()))
        .transpose()
}

/// The file `data` carrying the roles kept with it, `roles`, as JSON
/// [`Kept`] writes; as it is with none.
pub fn carrying(
    data: Vec<u8>,
    roles: Option<&str>,
) -> Result<Vec<u8>, sqlx::Error> {
    let Some(json) = roles else {
        return Ok(data);
    };
    let decode = |err: anyhow::Error| sqlx::Error::Decode(err.into());
    let kept: Kept = serde_json::from_str(json)
        .map_err(|err| decode(anyhow::anyhow!("its roles: {err}")))?;
    crate::schem::lines::with(&data, Some(&kept.roles())).map_err(decode)
}

/// The roles of `file`'s lines as its version keeps them, JSON of
/// [`Kept`], when it carries any.
fn roles_json(file: &Prepared) -> Option<String> {
    let kept = Kept::read(&file.blocks)?;
    Some(serde_json::to_string(&kept).expect("lines are JSON"))
}

/// The file as its version keeps it, without the roles it carries.
fn stored(file: &Prepared) -> Result<Vec<u8>, sqlx::Error> {
    file.stored()
        .map_err(|err| sqlx::Error::Encode(err.into_boxed_dyn_error()))
}

/// Keeps the roles the file `data`, build `id`'s current one, carries as
/// those of its version, as `simulation::refresh` does for one whose
/// roles were not yet found.
pub async fn set_current_roles<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    data: &[u8],
) -> Result<(), sqlx::Error> {
    let blocks = crate::schem::Blocks::read(data)
        .map_err(|err| sqlx::Error::Decode(err.into()))?;
    let Some(kept) = Kept::read(&blocks) else {
        return Ok(());
    };
    let json = serde_json::to_string(&kept).expect("lines are JSON");
    sqlx::query!(
        "UPDATE build_version SET roles = ?
         WHERE build_id = ? AND version = (
             SELECT max(version) FROM build_version AS newest
             WHERE newest.build_id = ?
         )",
        json,
        id,
        id
    )
    .execute(db)
    .await?;
    Ok(())
}

/// The build and version whose file has the hash `sha256`, if any is
/// stored: each file is stored once, but in a restored version, which
/// shares its file with the version it restores.
pub async fn holder<'e, E: SqliteExecutor<'e>>(
    db: E,
    sha256: &str,
) -> Result<Option<(String, i64)>, sqlx::Error> {
    Ok(sqlx::query!(
        "SELECT build_id, version FROM build_version
         WHERE sha256 = ? ORDER BY restores IS NOT NULL, version",
        sha256
    )
    .fetch_optional(db)
    .await?
    .map(|row| (row.build_id, row.version)))
}

/// Records a new build's file as its first version.
pub(crate) async fn first(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
    file: &Prepared,
    added_by: Option<Uuid>,
    added_from: Option<&str>,
) -> Result<(), sqlx::Error> {
    let (data, roles) = (stored(file)?, roles_json(file));
    sqlx::query!(
        "INSERT INTO build_version
            (build_id, version, data, sha256, added_by, added_from,
            roles)
         VALUES (?, 1, ?, ?, ?, ?, ?)",
        id,
        data,
        file.sha256,
        added_by,
        added_from,
        roles
    )
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// How a new file takes the place of a build's current one.
#[derive(Clone, Copy)]
pub enum Replacing<'a> {
    /// As its next version, uploaded by `added_by` from IP address
    /// `added_from` with a `note` saying what it changed, restoring the
    /// version `restores` if it copies one's file. The build counts as
    /// edited.
    NextVersion {
        added_by: Option<Uuid>,
        added_from: Option<&'a str>,
        note: Option<&'a str>,
        restores: Option<i64>,
    },
    /// In its current version's place, as `rsdb refresh` and
    /// `rsdb new-version --in-place` fix a file: the same build, tidied,
    /// with no version added.
    InPlace,
}

/// Makes `file` build `id`'s current file, as `how` says, together with
/// what is measured and read from it, and returns the number of the
/// version that holds it.
pub async fn replace(
    db: &SqlitePool,
    id: &str,
    file: &Prepared,
    how: Replacing<'_>,
) -> Result<i64, sqlx::Error> {
    let (data, roles) = (stored(file)?, roles_json(file));
    let mut tx = db.begin().await?;
    let current = sqlx::query_scalar!(
        r#"SELECT coalesce(max(version), 0) as "version!: i64"
         FROM build_version WHERE build_id = ?"#,
        id
    )
    .fetch_one(&mut *tx)
    .await?;
    let (version, edited) = match how {
        Replacing::NextVersion {
            added_by,
            added_from,
            note,
            restores,
        } => {
            let version = current + 1;
            sqlx::query!(
                "INSERT INTO build_version
                    (build_id, version, data, sha256, added_by,
                    added_from, note, roles, restores)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                id,
                version,
                data,
                file.sha256,
                added_by,
                added_from,
                note,
                roles,
                restores
            )
            .execute(&mut *tx)
            .await?;
            (version, true)
        }
        // A file changed in place may have moved its blocks, so what each
        // must be is read again rather than kept for the old file.
        Replacing::InPlace => {
            sqlx::query!(
                "UPDATE build_version SET data = ?,
                    needs = iif(sha256 = ?, needs, NULL), sha256 = ?,
                    roles = coalesce(?, iif(sha256 = ?, roles, NULL))
                 WHERE build_id = ? AND version = ?",
                data,
                file.sha256,
                file.sha256,
                roles,
                file.sha256,
                id,
                current
            )
            .execute(&mut *tx)
            .await?;
            (current, false)
        }
    };
    let (size, facts) = (&file.size, &file.facts);
    let (x, y, z) = (size.x, size.y, size.z);
    sqlx::query!(
        "UPDATE build SET size_x = ?, size_y = ?, size_z = ?,
            conventional = ?, torches = ?, works_from = ?, works_from_by = ?,
            edited_at = iif(?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                edited_at)
         WHERE id = ?",
        x,
        y,
        z,
        file.conventional,
        facts.torches,
        facts.works_from.release,
        facts.works_from.because,
        edited,
        id
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(version)
}
