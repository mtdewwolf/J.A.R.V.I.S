//! Single-worker lease with monotonic fencing.
//!
//! One worker holds the core lease at a time. Takeover increments the fence
//! monotonically, and transitions guarded by a lease check refuse to run on a
//! fence that has been superseded — the mechanism that keeps a stale or
//! restarted worker from committing after takeover (architecture.md,
//! "Durable task state"; AC-17).

use sqlx::PgPool;

use thiserror::Error;

pub const CORE_LEASE: &str = "core-worker";

#[derive(Debug, Error)]
pub enum LeaseError {
    /// The lease is held by a live holder.
    #[error("lease {lease_name} is held by a live holder (fence {current_fence})")]
    Held {
        lease_name: String,
        current_fence: i64,
    },
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LeaseGrant {
    pub lease_name: String,
    pub holder: String,
    pub fence: i64,
    /// RFC 3339 text.
    pub expires_at: String,
}

/// Acquire (or take over an expired lease). Monotonic: takeover always
/// increments the fence.
pub async fn acquire(
    pool: &PgPool,
    lease_name: &str,
    holder: &str,
    ttl_seconds: f64,
) -> Result<LeaseGrant, LeaseError> {
    let mut tx = pool.begin().await?;
    let grant: Option<LeaseGrant> = sqlx::query_as::<_, LeaseGrant>(
        r#"
        WITH upserted AS (
            INSERT INTO leases (lease_name, holder, fence, expires_at)
            VALUES ($1, $2, 1, now() + make_interval(secs => $3::double precision))
            ON CONFLICT (lease_name) DO UPDATE
              SET fence = leases.fence + 1, holder = EXCLUDED.holder, expires_at = EXCLUDED.expires_at
              WHERE leases.expires_at < now()
            RETURNING lease_name, holder, fence, expires_at::text AS expires_at
        )
        SELECT * FROM upserted
        "#,
    )
    .bind(lease_name)
    .bind(holder)
    .bind(ttl_seconds)
    .fetch_optional(&mut *tx)
    .await?;

    match grant {
        Some(grant) => {
            tx.commit().await?;
            Ok(grant)
        }
        None => {
            let current: (i64,) = sqlx::query_as("SELECT fence FROM leases WHERE lease_name = $1")
                .bind(lease_name)
                .fetch_one(&mut *tx)
                .await?;
            Err(LeaseError::Held {
                lease_name: lease_name.to_string(),
                current_fence: current.0,
            })
        }
    }
}

/// Refresh only succeeds for a holder whose fence is still current; a stale
/// fence (superseded by takeover) refreshes nothing.
///
/// The startup lease-maintenance loop lands with the worker loop in a later
/// PR; exercised here by the takeover test.
#[allow(dead_code)]
pub async fn refresh(
    pool: &PgPool,
    lease_name: &str,
    holder: &str,
    fence: i64,
    ttl_seconds: f64,
) -> Result<bool, LeaseError> {
    let result = sqlx::query(
        r#"
        UPDATE leases
        SET expires_at = now() + make_interval(secs => $4::double precision)
        WHERE lease_name = $1 AND holder = $2 AND fence = $3
        "#,
    )
    .bind(lease_name)
    .bind(holder)
    .bind(fence)
    .bind(ttl_seconds)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// A fence is stale once the lease has moved past it.
pub async fn is_stale(pool: &PgPool, lease_name: &str, fence: i64) -> Result<bool, LeaseError> {
    let row: Option<(i64,)> = sqlx::query_as("SELECT fence FROM leases WHERE lease_name = $1")
        .bind(lease_name)
        .fetch_optional(pool)
        .await?;
    Ok(matches!(row, Some((current,)) if current > fence))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = "../../migrations")]
    async fn takeover_increments_fence_and_fences_out_stale_holder(
        pool: PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let first = acquire(&pool, CORE_LEASE, "worker-a", 60.0).await?;
        assert_eq!(first.fence, 1);

        // A second live holder cannot steal the lease.
        let denied = acquire(&pool, CORE_LEASE, "worker-b", 60.0).await;
        assert!(matches!(
            denied,
            Err(LeaseError::Held {
                current_fence: 1,
                ..
            })
        ));

        // Expire worker-a's lease; worker-b takes over with a higher fence.
        sqlx::query(
            "UPDATE leases SET expires_at = now() - interval '1 second' WHERE lease_name = $1",
        )
        .bind(CORE_LEASE)
        .execute(&pool)
        .await?;
        let second = acquire(&pool, CORE_LEASE, "worker-b", 60.0).await?;
        assert_eq!(second.fence, 2, "takeover increments the fence");

        // Stale worker-a can neither refresh nor pass the fence check.
        assert!(!refresh(&pool, CORE_LEASE, "worker-a", first.fence, 60.0).await?);
        assert!(is_stale(&pool, CORE_LEASE, first.fence).await?);

        // Current holder still can.
        assert!(refresh(&pool, CORE_LEASE, "worker-b", second.fence, 60.0).await?);
        assert!(!is_stale(&pool, CORE_LEASE, second.fence).await?);
        Ok(())
    }
}
