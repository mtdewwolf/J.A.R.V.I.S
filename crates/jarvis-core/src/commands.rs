//! Idempotent command ingestion (AC-05).
//!
//! A client command carries an owner/request pair. Re-sending the same pair
//! must never create a duplicate command, generation or later execution: the
//! first insert wins and replays are reported as replays, carrying the
//! original `command_id` so callers can bind to the original record.

use serde_json::Value;
use sqlx::PgPool;

use crate::events;

#[derive(Debug, PartialEq, Eq)]
pub enum IngestOutcome {
    Ingested { command_id: String },
    Replayed { command_id: String },
}

pub async fn ingest(
    pool: &PgPool,
    owner_id: &str,
    request_id: &str,
    command_type: &str,
    payload: Value,
) -> Result<IngestOutcome, sqlx::Error> {
    let command_id = crate::ids::new_id("cmd");
    let mut tx = pool.begin().await?;

    let inserted = sqlx::query(
        r#"
        INSERT INTO commands (command_id, owner_id, request_id, command_type, payload, state)
        VALUES ($1, $2, $3, $4, $5, 'accepted')
        ON CONFLICT (owner_id, request_id) DO NOTHING
        "#,
    )
    .bind(&command_id)
    .bind(owner_id)
    .bind(request_id)
    .bind(command_type)
    .bind(sqlx::types::Json(payload))
    .execute(&mut *tx)
    .await?
    .rows_affected();

    let outcome = if inserted == 1 {
        events::append_in_tx(
            &mut tx,
            owner_id,
            "command.ingested",
            serde_json::json!({ "command_id": command_id, "command_type": command_type }),
        )
        .await?;
        IngestOutcome::Ingested { command_id }
    } else {
        let existing: (String,) = sqlx::query_as(
            "SELECT command_id FROM commands WHERE owner_id = $1 AND request_id = $2",
        )
        .bind(owner_id)
        .bind(request_id)
        .fetch_one(&mut *tx)
        .await?;
        IngestOutcome::Replayed {
            command_id: existing.0,
        }
    };

    tx.commit().await?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sqlx::Row;

    #[sqlx::test(migrations = "../../migrations")]
    async fn replayed_request_reuses_original_command(pool: PgPool) -> Result<(), sqlx::Error> {
        let first = ingest(
            &pool,
            "owner_a",
            "req_001",
            "input.send",
            json!({ "text": "hi" }),
        )
        .await?;
        let second = ingest(
            &pool,
            "owner_a",
            "req_001",
            "input.send",
            json!({ "text": "hi" }),
        )
        .await?;
        let third = ingest(
            &pool,
            "owner_a",
            "req_002",
            "input.send",
            json!({ "text": "again" }),
        )
        .await?;

        assert!(matches!(first, IngestOutcome::Ingested { .. }));
        assert_eq!(
            second,
            IngestOutcome::Replayed {
                command_id: first_command_id(&first)
            }
        );
        assert!(matches!(third, IngestOutcome::Ingested { .. }));

        let count: i64 = sqlx::query("SELECT count(*) FROM commands WHERE owner_id = 'owner_a'")
            .fetch_one(&pool)
            .await?
            .get(0);
        assert_eq!(count, 2, "replay creates no duplicate command");

        let events: i64 =
            sqlx::query("SELECT count(*) FROM events WHERE type = 'command.ingested'")
                .fetch_one(&pool)
                .await?
                .get(0);
        assert_eq!(events, 2, "replay records no extra event");
        Ok(())
    }

    fn first_command_id(outcome: &IngestOutcome) -> String {
        match outcome {
            IngestOutcome::Ingested { command_id } | IngestOutcome::Replayed { command_id } => {
                command_id.clone()
            }
        }
    }
}
