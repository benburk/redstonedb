//! Each build's latest test result, kept with the hash of the file it ran
//! on and the key of the circuit that file holds.

use anyhow::Result;
use sqlx::SqliteExecutor;

use crate::{category::Test, simulate::Report};

/// Build `id`'s stored result, when it ran on the file whose hash is
/// `sha256` under `test`; a result of another file or test, none. A change
/// to a test leaves its stored results as they are until a build is
/// simulated again, as `rsdb refresh` does; one a change to what a report
/// holds left unreadable is none until then.
pub async fn stored<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    sha256: &str,
    test: Test,
) -> Result<Option<Report>> {
    let name = test.name();
    let report = sqlx::query_scalar!(
        "SELECT report FROM simulation
         WHERE build_id = ? AND sha256 = ? AND test = ?",
        id,
        sha256,
        name
    )
    .fetch_optional(db)
    .await?;
    Ok(report.and_then(|report| {
        serde_json::from_str(&report)
            .inspect_err(|err| {
                tracing::warn!(id, "stored report is unreadable: {err}");
            })
            .ok()
    }))
}

/// Keeps `report` as build `id`'s result of `test` on the file whose hash
/// is `sha256`, holding the circuit whose key is `circuit`, in place of any
/// before.
pub async fn set<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    test: Test,
    (sha256, circuit): (&str, Option<&str>),
    report: &Report,
) -> Result<()> {
    let (name, failed) = (test.name(), report.failed());
    let report = serde_json::to_string(report)?;
    sqlx::query!(
        "INSERT INTO simulation
            (build_id, test, sha256, circuit, failed, report)
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT (build_id) DO UPDATE SET test = excluded.test,
            sha256 = excluded.sha256, circuit = excluded.circuit,
            failed = excluded.failed, report = excluded.report,
            ran_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        id,
        name,
        sha256,
        circuit,
        failed,
        report
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Drops build `id`'s result, as when its category has no test.
pub async fn clear<'e, E: SqliteExecutor<'e>>(db: E, id: &str) -> Result<()> {
    sqlx::query!("DELETE FROM simulation WHERE build_id = ?", id)
        .execute(db)
        .await?;
    Ok(())
}

/// Moves build `id`'s result to the file whose hash is `sha256`, when it
/// ran on another file holding the circuit whose key is `circuit`.
pub async fn adopt<'e, E: SqliteExecutor<'e>>(
    db: E,
    id: &str,
    circuit: &str,
    sha256: &str,
) -> Result<()> {
    sqlx::query!(
        "UPDATE simulation SET sha256 = ?
         WHERE build_id = ? AND circuit = ? AND sha256 <> ?",
        sha256,
        id,
        circuit,
        sha256
    )
    .execute(db)
    .await?;
    Ok(())
}
