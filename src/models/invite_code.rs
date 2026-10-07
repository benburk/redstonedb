use sqlx::{FromRow, SqliteExecutor};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct AvailableInvite {
    pub code: String,
    pub issuer_username: String,
}

pub struct InviteCode;

impl InviteCode {
    /// Return an unused invite and the username of the user who issued it.
    pub async fn get_available<'e, E: SqliteExecutor<'e>>(
        db: E,
        code: &str,
    ) -> Result<Option<AvailableInvite>, sqlx::Error> {
        sqlx::query_as::<_, AvailableInvite>(
            r"SELECT invite_code.code, user.username AS issuer_username
               FROM invite_code
               JOIN user ON user.id = invite_code.issued_by
               WHERE invite_code.code = ? AND invite_code.redeemed_by IS NULL",
        )
        .bind(code)
        .fetch_optional(db)
        .await
    }

    /// Redeem an unused invite for a user, returning false if it is no longer available.
    pub async fn redeem<'e, E: SqliteExecutor<'e>>(
        db: E,
        code: &str,
        user_id: Uuid,
        redeemed_at: i64,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r"UPDATE invite_code
               SET redeemed_by = ?, redeemed_at = ?
               WHERE code = ? AND redeemed_by IS NULL",
        )
        .bind(user_id)
        .bind(redeemed_at)
        .bind(code)
        .execute(db)
        .await?;
        Ok(result.rows_affected() == 1)
    }
}

/// Canonicalize a Crockford Base32 invite entered by a person.
pub fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|character| {
            *character != '-' && !character.is_ascii_whitespace()
        })
        .map(|character| match character.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            character => character,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn normalizes_human_entered_codes() {
        assert_eq!(normalize(" sxnh-wa69 53jc-o34x "), "SXNHWA6953JC034X");
        assert_eq!(normalize("il"), "11");
    }
}
