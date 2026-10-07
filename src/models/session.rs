use sqlx::{FromRow, SqliteExecutor};
use uuid::Uuid;

use crate::util::current_time_micros;

#[derive(Debug, Clone, FromRow)]
pub struct Session {
    pub id: Uuid,
    pub user_id: Uuid,
    pub ip_address: String,
    pub user_agent: String,
    pub created_at: i64,
    pub expires_at: i64,
}

impl Session {
    pub async fn insert<'e, E: SqliteExecutor<'e>>(
        db: E,
        session: &Self,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query!(
            "INSERT INTO session (id, user_id, ip_address, user_agent, created_at, expires_at)
        VALUES (?, ?, ?, ?, ?, ?)",
            session.id,
            session.user_id,
            session.ip_address,
            session.user_agent,
            session.created_at,
            session.expires_at,
        )
        .execute(db)
        .await
        .map(|row| row.last_insert_rowid())
    }

    pub async fn get_by_id<'e, E: SqliteExecutor<'e>>(
        db: E,
        id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            Session,
            r#"SELECT
            id as "id: uuid::Uuid",
            user_id as "user_id: uuid::Uuid",
            ip_address,
            user_agent,
            created_at,
            expires_at
            FROM 'session' WHERE id = ?"#,
            id
        )
        .fetch_optional(db)
        .await
    }

    pub async fn delete_by_id<'e, E: SqliteExecutor<'e>>(
        db: E,
        id: Uuid,
    ) -> Result<u64, sqlx::Error> {
        sqlx::query!("DELETE FROM session WHERE id = ?", id)
            .execute(db)
            .await
            .map(|row| row.rows_affected())
    }

    pub async fn get_by_user_id<'e, E: SqliteExecutor<'e>>(
        db: E,
        user_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let now = current_time_micros();
        sqlx::query_as!(
            Session,
            r#"SELECT
            id as "id: uuid::Uuid",
            user_id as "user_id: uuid::Uuid",
            ip_address,
            user_agent,
            created_at,
            expires_at
            FROM 'session' WHERE user_id = ? AND expires_at > ?
            ORDER BY created_at DESC"#,
            user_id,
            now
        )
        .fetch_all(db)
        .await
    }
}
