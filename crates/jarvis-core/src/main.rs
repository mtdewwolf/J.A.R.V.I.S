//! J.A.R.V.I.S trusted core — PR-2 durable slice.
//!
//! Startup applies migrations, acquires the single-worker lease with a
//! monotonic fence (refusing to run a second live worker), and runs the
//! recovery sweep once. No model calls, tools or client surface yet: those
//! arrive in PR-3/PR-4 per docs/phase-0/pull-requests.md.

// `input.send` and the client-facing replay events are PR-4 surface; the
// ingestion path is complete and tested, and starts flowing from the
// authenticated CLI endpoint when that lands.
#[allow(dead_code)]
mod commands;
// Durable events feed the authenticated client protocol; the Event row is
// returned by the append path and consumed from PR-4 onward.
#[allow(dead_code)]
mod events;
mod ids;
mod leases;
#[allow(dead_code)]
mod limits;
mod recovery;
mod store;
// The task state machine is exercised by the recovery sweep and its tests
// today; PR-3/PR-4 wire the remaining API (steps, dispatch, completion).
#[allow(dead_code)]
mod tasks;

use std::env;

fn holder_id() -> String {
    let host = env::var("HOSTNAME").unwrap_or_else(|_| "local".to_string());
    format!("jarvis-core/{}@{}", std::process::id(), host)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
        eprintln!(
            "DATABASE_URL is not set; the core requires PostgreSQL (docs/phase-0/technology.md)"
        );
        std::process::exit(2);
    });

    let pool = store::connect(&database_url).await?;
    store::migrate(&pool).await?;
    tracing::info!(target: "jarvis_core", "migrations applied; durable slice ready");

    let lease = match leases::acquire(&pool, leases::CORE_LEASE, &holder_id(), 30.0).await {
        Ok(lease) => lease,
        Err(leases::LeaseError::Held {
            lease_name,
            current_fence,
        }) => {
            eprintln!(
                "lease {lease_name} is held by a live holder (fence {current_fence}); \
                 refusing to start a second core worker"
            );
            std::process::exit(5);
        }
        Err(e) => return Err(e.into()),
    };
    tracing::info!(
        target: "jarvis_core",
        lease = %lease.lease_name,
        fence = lease.fence,
        holder = %lease.holder,
        expires_at = %lease.expires_at,
        "core lease acquired"
    );

    let report = recovery::recover_once(&pool).await?;
    tracing::info!(target: "jarvis_core", ?report, "recovery sweep complete");

    // Fail closed rather than proceed if takeover happened during startup.
    if leases::is_stale(&pool, leases::CORE_LEASE, lease.fence).await? {
        eprintln!("lease was taken over during startup; refusing to act as a stale worker");
        std::process::exit(5);
    }

    tracing::info!(
        target: "jarvis_core",
        "PR-2 durable slice initialized: commands, tasks, steps, events, leases, recovery"
    );
    Ok(())
}
