use sqlx::SqliteExecutor;
use uuid::Uuid;

/// A login: a random id kept in the browser's cookie, the account it logs
/// in, and when it stops working.
#[derive(Debug, Clone)]
pub struct Session {
    pub id: Uuid,
    pub user_id: Uuid,
    pub expires_at: i64,
}

impl Session {
    pub async fn insert<'e, E: SqliteExecutor<'e>>(
        db: E,
        session: &Self,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "INSERT INTO session (id, user_id, expires_at) VALUES (?, ?, ?)",
            session.id,
            session.user_id,
            session.expires_at,
        )
        .execute(db)
        .await?;
        Ok(())
    }

    pub async fn get_by_id<'e, E: SqliteExecutor<'e>>(
        db: E,
        id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            Session,
            r#"SELECT id as "id: Uuid", user_id as "user_id: Uuid", expires_at
             FROM session WHERE id = ?"#,
            id
        )
        .fetch_optional(db)
        .await
    }

    pub async fn delete_by_id<'e, E: SqliteExecutor<'e>>(
        db: E,
        id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!("DELETE FROM session WHERE id = ?", id)
            .execute(db)
            .await?;
        Ok(())
    }

    /// Deletes every session that has expired.
    pub async fn delete_expired<'e, E: SqliteExecutor<'e>>(
        db: E,
        now: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!("DELETE FROM session WHERE expires_at <= ?", now)
            .execute(db)
            .await?;
        Ok(())
    }
}
