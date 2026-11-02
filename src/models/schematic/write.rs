//! The writes that change stored builds: storing a new one, its details,
//! credits, tags, flags, and stacking, what its simulation works out,
//! whether it is public, its downloads, and deleting it; and the builds
//! people favourite.

use sqlx::{SqliteExecutor, SqlitePool};

use super::{
    Encoding, Logic, Orientation, StackDirection, Widened, load::Credit,
};
use crate::{
    details::Ready,
    import::Prepared,
    models::{tag, version},
};

/// Makes build `id` public or private.
pub async fn set_public<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    public: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query!("UPDATE schematic SET public = ? WHERE id = ?", public, id)
        .execute(db)
        .await?;
    Ok(())
}

/// Whether `user` has favourited build `id`.
pub async fn favorited(
    db: &SqlitePool,
    user: uuid::Uuid,
    id: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM favorite
            WHERE user_id = ? AND schematic_id = ?) AS "found!: bool""#,
        user,
        id
    )
    .fetch_one(db)
    .await
}

/// Favourites build `id` for `user`, or takes it back when `on` is false.
pub async fn set_favorite(
    db: &SqlitePool,
    user: uuid::Uuid,
    id: &str,
    on: bool,
) -> Result<(), sqlx::Error> {
    if on {
        sqlx::query!(
            "INSERT OR IGNORE INTO favorite (user_id, schematic_id) VALUES (?, ?)",
            user,
            id
        )
        .execute(db)
        .await?;
    } else {
        sqlx::query!(
            "DELETE FROM favorite WHERE user_id = ? AND schematic_id = ?",
            user,
            id
        )
        .execute(db)
        .await?;
    }
    Ok(())
}

/// The ids of the builds `user` has favourited, newest first.
pub async fn favorites(
    db: &SqlitePool,
    user: uuid::Uuid,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT schematic_id FROM favorite WHERE user_id = ? ORDER BY added_at DESC",
        user
    )
    .fetch_all(db)
    .await
}
/// A new build: its details as checked, and its file as prepared.
#[derive(Clone, Copy)]
pub struct New<'a> {
    pub ready: &'a Ready,
    pub file: &'a Prepared,
    pub uploaded_by: Option<uuid::Uuid>,
    /// The IP address it was uploaded from.
    pub uploaded_from: Option<&'a str>,
    /// The in-game name of whoever imported it with `rsdb`.
    pub imported_by: Option<&'a str>,
    /// The id to store it under, as a build rebuilt from another copy of
    /// the database keeps its own; `None` makes one.
    pub id: Option<&'a str>,
}

/// Stores a new build, with its file as its first version, and returns its
/// id, the one given or a generated one.
pub async fn insert(
    db: &SqlitePool,
    new: &New<'_>,
) -> Result<String, sqlx::Error> {
    let (details, file) = (&new.ready.details, new.file);
    let (size, facts) = (&file.size, &file.facts);
    let (x, y, z) = (size.x, size.y, size.z);
    let mut tx = db.begin().await?;
    let id = sqlx::query_scalar!(
        r#"INSERT INTO schematic (id, category, width, orientation, encoding,
            latency, synchronized, size_x, size_y, size_z, built_on,
            imported_by, conventional, design, description, given_name,
            logic, throughput, at_least, pulse, bytes, torches, works_from,
            works_from_by, keeps_paste, uploaded_by, unverified)
         VALUES (coalesce(?, lower(hex(randomblob(16)))), ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
            ?, ?, ?, ?)
         RETURNING id as "id!""#,
        new.id,
        details.category,
        details.width,
        details.orientation,
        details.encoding,
        details.latency,
        details.synchronized,
        x,
        y,
        z,
        details.built_on,
        new.imported_by,
        file.conventional,
        details.design,
        details.description,
        details.given_name,
        details.logic,
        details.throughput,
        details.at_least,
        details.pulse,
        details.bytes,
        facts.torches,
        facts.works_from.release,
        facts.works_from.because,
        file.keeps_paste,
        new.uploaded_by,
        details.unverified,
    )
    .fetch_one(&mut *tx)
    .await?;
    version::first(&mut tx, &id, file, new.uploaded_by, new.uploaded_from)
        .await?;
    set_lists(&mut tx, &id, new.ready).await?;
    tx.commit().await?;
    Ok(id)
}

/// Writes checked details over build `id`'s, with its credits, tags, and
/// flags, and its stacking unless the simulation decides it. A build whose
/// every measured detail the simulation works out keeps those as it set
/// them, as its result on the same file stands and is not run again. The
/// build counts as edited.
pub async fn set_details(
    db: &SqlitePool,
    id: &str,
    ready: &Ready,
) -> Result<(), sqlx::Error> {
    let details = &ready.details;
    let mut tx = db.begin().await?;
    if ready.evaluated {
        sqlx::query!(
            "UPDATE schematic SET category = ?, built_on = ?,
                description = ?, given_name = ?, at_least = ?, bytes = ?,
                unverified = NULL,
                edited_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
             WHERE id = ?",
            details.category,
            details.built_on,
            details.description,
            details.given_name,
            details.at_least,
            details.bytes,
            id,
        )
        .execute(&mut *tx)
        .await?;
        set_people(&mut tx, id, ready).await?;
        return tx.commit().await;
    }
    sqlx::query!(
        "UPDATE schematic SET category = ?, width = ?, orientation = ?,
            encoding = ?, latency = ?, synchronized = ?, design = ?,
            built_on = ?, description = ?, given_name = ?, logic = ?,
            throughput = ?, at_least = ?, pulse = ?, bytes = ?, unverified = ?,
            edited_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE id = ?",
        details.category,
        details.width,
        details.orientation,
        details.encoding,
        details.latency,
        details.synchronized,
        details.design,
        details.built_on,
        details.description,
        details.given_name,
        details.logic,
        details.throughput,
        details.at_least,
        details.pulse,
        details.bytes,
        details.unverified,
        id,
    )
    .execute(&mut *tx)
    .await?;
    set_lists(&mut tx, id, ready).await?;
    tx.commit().await
}

/// Replaces build `id`'s credits, tags, and flags with `ready`'s, and its
/// stacking unless the simulation decides it.
async fn set_lists(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
    ready: &Ready,
) -> Result<(), sqlx::Error> {
    let details = &ready.details;
    if !ready.stacking_inferred {
        set_stacking(tx, id, &details.stacking).await?;
    }
    set_flags(tx, id, &details.flags).await?;
    set_people(tx, id, ready).await
}

/// Replaces build `id`'s credits and tags with `ready`'s.
async fn set_people(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
    ready: &Ready,
) -> Result<(), sqlx::Error> {
    let details = &ready.details;
    let credits = [
        (Credit::Builder, &details.builders),
        (Credit::Contributor, &details.contributors),
    ];
    for (credit, names) in credits {
        set_credits(tx, id, credit, names).await?;
    }
    tag::set(tx, id, &details.tags).await
}

/// Replaces the people a build credits in one role with `names`, in that
/// order. A name given twice, in any case, is credited once.
async fn set_credits(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
    credit: Credit,
    names: &[String],
) -> Result<(), sqlx::Error> {
    let role = credit.as_str();
    sqlx::query!(
        "DELETE FROM credit WHERE schematic_id = ? AND role = ?",
        id,
        role
    )
    .execute(&mut *tx)
    .await?;
    for (position, name) in (0_i64..).zip(names) {
        // A name an admin has confirmed for an account is stored as that
        // account spells it.
        sqlx::query!(
            "INSERT OR IGNORE INTO credit (schematic_id, role, name, position)
             VALUES (?, ?, coalesce(
                (SELECT minecraft_name FROM user WHERE minecraft_verified
                    AND minecraft_name = ? COLLATE NOCASE),
                ?), ?)",
            id,
            role,
            name,
            name,
            position
        )
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
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
        let faces = d
            .faces
            .map(|faces| serde_json::to_string(&faces).unwrap_or_default());
        sqlx::query!(
            "INSERT INTO stack_direction
                (schematic_id, west, up, north, max_copies, faces)
             VALUES (?, ?, ?, ?, ?, ?)",
            id,
            d.west,
            d.up,
            d.north,
            d.max_copies,
            faces,
        )
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

/// Replaces the wider builds a build's copies make with `widened`.
pub async fn set_widened(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
    widened: &[Widened],
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM stacked_width WHERE schematic_id = ?", id)
        .execute(&mut *tx)
        .await?;
    for w in widened {
        sqlx::query!(
            "INSERT INTO stacked_width
                (schematic_id, width, copies, latency, synchronized,
                 throughput)
             VALUES (?, ?, ?, ?, ?, ?)",
            id,
            w.width,
            w.copies,
            w.latency,
            w.synchronized,
            w.throughput,
        )
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

/// Replaces the flags a build has with those of `names` its category has,
/// as a subtractor has no subtract flag, its category saying so.
pub async fn set_flags(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
    names: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM schematic_flag WHERE schematic_id = ?", id)
        .execute(&mut *tx)
        .await?;
    for name in names {
        sqlx::query!(
            "INSERT OR IGNORE INTO schematic_flag (schematic_id, name)
             SELECT schematic.id, flag.name FROM schematic
             JOIN flag ON flag.category = schematic.category AND flag.name = ?
             WHERE schematic.id = ?",
            name,
            id
        )
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

/// Sets the details worked out from a build's file and its simulation
/// rather than typed: its width, orientation, and encoding as its file
/// shows them, and its latency and whether it is synchronized as the
/// simulation measured them. None of it is a person's edit.
pub async fn set_derived<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    width: Option<i64>,
    orientation: Orientation,
    encoding: Option<Encoding>,
    latency: Option<i64>,
    synchronized: Option<bool>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE schematic SET width = ?, orientation = ?, encoding = ?,
            latency = ?, synchronized = ?
         WHERE id = ?",
        width,
        orientation,
        encoding,
        latency,
        synchronized,
        id
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Sets the details `evaluate` gives a build of a category whose details
/// are all evaluated, beside those `set_derived` sets.
pub async fn set_evaluated<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    throughput: Option<i64>,
    design: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE schematic SET throughput = ?, design = ? WHERE id = ?",
        throughput,
        design,
        id
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Sets whether build `id` holds state, where it differs from its
/// category's; `None` follows the category.
pub async fn set_logic<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    logic: Option<Logic>,
) -> Result<(), sqlx::Error> {
    let logic = logic.map(Logic::name);
    sqlx::query!("UPDATE schematic SET logic = ? WHERE id = ?", logic, id)
        .execute(db)
        .await?;
    Ok(())
}

/// Sets a monostable's pulse as the simulation measured it, which is no
/// person's edit either.
pub async fn set_pulse<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    pulse: Option<i64>,
) -> Result<(), sqlx::Error> {
    sqlx::query!("UPDATE schematic SET pulse = ? WHERE id = ?", pulse, id)
        .execute(db)
        .await?;
    Ok(())
}

/// Sets a threshold's count of its inputs on that turns it on, as its
/// simulation finds it.
pub async fn set_at_least<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    at_least: Option<i64>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE schematic SET at_least = ? WHERE id = ?",
        at_least,
        id
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Sets the fields only `rsdb set` changes once a build is stored: who
/// imported it, and whether it follows the building conventions.
pub async fn set_import_details<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    imported_by: Option<&str>,
    conventional: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE schematic SET imported_by = ?, conventional = ? WHERE id = ?",
        imported_by,
        conventional,
        id
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Counts one more download of a schematic.
pub async fn count_download<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE schematic SET downloads = downloads + 1 WHERE id = ?",
        id
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Deletes a schematic, with its versions and everything else of it.
pub async fn delete<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM schematic WHERE id = ?", id)
        .execute(db)
        .await?;
    Ok(())
}
