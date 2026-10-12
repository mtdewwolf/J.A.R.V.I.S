//! Content-free durable events with a replay cursor.
//!
//! Events are metadata: IDs, types, ordering and references. Private bodies
//! never land here (architecture.md). Cursors are gap-free per session, which
//! is what lets a reconnecting client replay without gaps or duplicates.
//!
//! `created_at` is selected as text so the module needs no chrono/time
//! dependency; callers that need typed time can parse RFC 3339.

use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Event {
    pub id: i64,
    pub session_id: Option<String>,
    pub task_id: Option<String>,
    pub owner_id: String,
    #[sqlx(rename = "type")]
    pub event_type: String,
    pub payload: Value,
    /// RFC 3339 text, e.g. `2026-10-12 21:10:33.123+00`.
    pub created_at: String,
}

/// Append one event inside an existing transaction.
pub async fn append_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    owner_id: &str,
    event_type: &str,
    payload: Value,
) -> Result<Event, sqlx::Error> {
    append_scoped_in_tx(tx, owner_id, event_type, payload, None, None).await
}

/// Append one event bound to a session and/or task.
pub async fn append_scoped_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    owner_id: &str,
    event_type: &str,
    payload: Value,
    session_id: Option<&str>,
    task_id: Option<&str>,
) -> Result<Event, sqlx::Error> {
    sqlx::query_as::<_, Event>(
        r#"
        INSERT INTO events (session_id, task_id, owner_id, type, payload)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, session_id, task_id, owner_id, type, payload, created_at::text AS created_at
        "#,
    )
    .bind(session_id)
    .bind(task_id)
    .bind(owner_id)
    .bind(event_type)
    .bind(sqlx::types::Json(payload))
    .fetch_one(&mut **tx)
    .await
}

/// Replay events for one session strictly after `after_cursor`, oldest first.
///
/// Consumed by the authenticated client protocol in PR-4; exercised here by
/// the cursor tests.
#[allow(dead_code)]
pub async fn replay(
    pool: &PgPool,
    session_id: &str,
    after_cursor: i64,
    limit: i64,
) -> Result<Vec<Event>, sqlx::Error> {
    sqlx::query_as::<_, Event>(
        r#"
        SELECT id, session_id, task_id, owner_id, type, payload, created_at::text AS created_at
        FROM events
        WHERE session_id = $1 AND id > $2
        ORDER BY id
        LIMIT $3
        "#,
    )
    .bind(session_id)
    .bind(after_cursor)
    .bind(limit.clamp(1, 1000))
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[sqlx::test(migrations = "../../migrations")]
    async fn replay_returns_gap_free_ordered_cursor(pool: PgPool) -> Result<(), sqlx::Error> {
        let session = "s_test_replay";
        let mut last_seen = 0i64;
        for i in 0..5 {
            let mut tx = pool.begin().await?;
            let event = append_scoped_in_tx(
                &mut tx,
                "owner_a",
                "response.delta",
                json!({ "i": i }),
                Some(session),
                None,
            )
            .await?;
            assert!(event.id > last_seen, "cursors must be increasing");
            last_seen = event.id;
            tx.commit().await?;
        }

        let all = replay(&pool, session, 0, 100).await?;
        assert_eq!(all.len(), 5, "all session events replay");

        let after = replay(&pool, session, all[2].id, 100).await?;
        assert_eq!(after.len(), 2);
        assert_eq!(after[0].id, all[3].id, "cursor is exclusive and ordered");

        // Another session's events never bleed across.
        let mut tx = pool.begin().await?;
        append_scoped_in_tx(
            &mut tx,
            "owner_a",
            "response.delta",
            json!({}),
            Some("s_other"),
            None,
        )
        .await?;
        tx.commit().await?;
        assert_eq!(replay(&pool, session, 0, 100).await?.len(), 5);
        Ok(())
    }
}
