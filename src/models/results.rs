//! A build's results carried from one database to another: its simulation
//! result and every detail the simulation sets, so builds can be refreshed
//! on a faster machine, against a copy of the live database, and their
//! results sent back. A build takes them only while its file and details
//! are as they were copied, so an upload or an edit since is never undone.

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

/// One build's results, with what they were worked out from.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Results {
    pub id: String,
    /// Its file's hash as the results were worked out.
    pub sha256: String,
    /// When its details were last edited then, if ever.
    pub edited_at: Option<String>,
    pub width: Option<i64>,
    pub orientation: String,
    pub encoding: Option<String>,
    pub latency: Option<i64>,
    pub synchronized: Option<i64>,
    pub throughput: Option<i64>,
    pub design: String,
    pub logic: Option<String>,
    pub pulse: Option<i64>,
    pub flags: Vec<String>,
    /// Each direction it stacks.
    pub stacking: Vec<Stacked>,
    pub simulation: Option<Simulation>,
}

/// A direction a build stacks, as stored: west, up, north, the most
/// copies, and the box its later copies repeat.
type Stacked = (i64, i64, i64, Option<i64>, Option<String>);

/// A build's stored simulation result.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Simulation {
    pub test: String,
    pub version: i64,
    pub sha256: String,
    pub failed: i64,
    pub report: String,
    pub ran_at: String,
    pub circuit: Option<String>,
}

/// What became of one build's results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Taken {
    Taken,
    /// Its file or details changed since, or it is gone, so they were not.
    Stale(String),
}

/// The results of each build in `ids`, as `db` holds them.
pub async fn export(
    db: &SqlitePool,
    ids: &[String],
) -> anyhow::Result<Vec<Results>> {
    let mut all = Vec::new();
    for id in ids {
        let row = sqlx::query(
            "SELECT s.width, s.orientation, s.encoding, s.latency,
                s.synchronized, s.throughput, s.design, s.logic, s.pulse,
                s.edited_at, f.sha256
             FROM schematic s JOIN current_file f ON f.schematic_id = s.id
             WHERE s.id = ?",
        )
        .bind(id)
        .fetch_one(db)
        .await?;
        let flags = sqlx::query_scalar(
            "SELECT name FROM schematic_flag WHERE schematic_id = ? ORDER BY name",
        )
        .bind(id)
        .fetch_all(db)
        .await?;
        let stacking = sqlx::query_as(
            "SELECT west, up, north, max_copies, faces FROM stack_direction
             WHERE schematic_id = ? ORDER BY west, up, north",
        )
        .bind(id)
        .fetch_all(db)
        .await?;
        let simulation = sqlx::query(
            "SELECT test, version, sha256, failed, report, ran_at, circuit
             FROM simulation WHERE schematic_id = ?",
        )
        .bind(id)
        .fetch_optional(db)
        .await?
        .map(|row| Simulation {
            test: row.get("test"),
            version: row.get("version"),
            sha256: row.get("sha256"),
            failed: row.get("failed"),
            report: row.get("report"),
            ran_at: row.get("ran_at"),
            circuit: row.get("circuit"),
        });
        all.push(Results {
            id: id.clone(),
            sha256: row.get("sha256"),
            edited_at: row.get("edited_at"),
            width: row.get("width"),
            orientation: row.get("orientation"),
            encoding: row.get("encoding"),
            latency: row.get("latency"),
            synchronized: row.get("synchronized"),
            throughput: row.get("throughput"),
            design: row.get("design"),
            logic: row.get("logic"),
            pulse: row.get("pulse"),
            flags,
            stacking,
            simulation,
        });
    }
    Ok(all)
}

/// Writes each of `results` over its build in `db`, each in a transaction
/// of its own, while its file and details are as they were when the
/// results were worked out; says what became of each.
pub async fn import(
    db: &SqlitePool,
    results: &[Results],
) -> anyhow::Result<Vec<(String, Taken)>> {
    let mut taken = Vec::new();
    for r in results {
        let mut tx = db.begin().await?;
        if let Some(why) = stale(&mut tx, r).await? {
            taken.push((r.id.clone(), Taken::Stale(why)));
            continue;
        }
        write(&mut tx, r).await?;
        tx.commit().await?;
        taken.push((r.id.clone(), Taken::Taken));
    }
    Ok(taken)
}

/// Why build `r.id`'s results no longer fit it, if they do not: it is
/// gone, or its file or details have changed since they were worked out.
async fn stale(
    tx: &mut sqlx::SqliteConnection,
    r: &Results,
) -> anyhow::Result<Option<String>> {
    let now: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT f.sha256, s.edited_at
         FROM schematic s JOIN current_file f ON f.schematic_id = s.id
         WHERE s.id = ?",
    )
    .bind(&r.id)
    .fetch_optional(&mut *tx)
    .await?;
    let stale = match now {
        None => Some("it is gone".to_owned()),
        Some((sha256, _)) if sha256 != r.sha256 => {
            Some("its file has changed".to_owned())
        }
        Some((_, edited_at)) if edited_at != r.edited_at => {
            Some("its details have been edited".to_owned())
        }
        Some(_) => None,
    };
    Ok(stale)
}

/// Writes `r` over its build.
async fn write(
    tx: &mut sqlx::SqliteConnection,
    r: &Results,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE schematic SET width = ?, orientation = ?, encoding = ?,
            latency = ?, synchronized = ?, throughput = ?, design = ?,
            logic = ?, pulse = ?
         WHERE id = ?",
    )
    .bind(r.width)
    .bind(&r.orientation)
    .bind(&r.encoding)
    .bind(r.latency)
    .bind(r.synchronized)
    .bind(r.throughput)
    .bind(&r.design)
    .bind(&r.logic)
    .bind(r.pulse)
    .bind(&r.id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM schematic_flag WHERE schematic_id = ?")
        .bind(&r.id)
        .execute(&mut *tx)
        .await?;
    for flag in &r.flags {
        sqlx::query(
            "INSERT INTO schematic_flag (schematic_id, name) VALUES (?, ?)",
        )
        .bind(&r.id)
        .bind(flag)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("DELETE FROM stack_direction WHERE schematic_id = ?")
        .bind(&r.id)
        .execute(&mut *tx)
        .await?;
    for (west, up, north, max_copies, faces) in &r.stacking {
        sqlx::query(
            "INSERT INTO stack_direction
                (schematic_id, west, up, north, max_copies, faces)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&r.id)
        .bind(west)
        .bind(up)
        .bind(north)
        .bind(max_copies)
        .bind(faces)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("DELETE FROM simulation WHERE schematic_id = ?")
        .bind(&r.id)
        .execute(&mut *tx)
        .await?;
    if let Some(sim) = &r.simulation {
        sqlx::query(
            "INSERT INTO simulation
                (schematic_id, test, version, sha256, failed, report,
                 ran_at, circuit)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&r.id)
        .bind(&sim.test)
        .bind(sim.version)
        .bind(&sim.sha256)
        .bind(sim.failed)
        .bind(&sim.report)
        .bind(&sim.ran_at)
        .bind(&sim.circuit)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}
