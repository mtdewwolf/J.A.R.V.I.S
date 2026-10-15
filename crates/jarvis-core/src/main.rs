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

mod api;
mod connections;
mod crypto;
mod inference;
mod jcs;
mod strict_json;
mod tools;

use std::{env, path::PathBuf, sync::Arc};

fn private_token(path: &std::path::Path) -> Result<String, Box<dyn std::error::Error>> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.mode() & 0o077 != 0 || meta.uid() != unsafe { libc::geteuid() } {
        return Err("token file must be owned by this user and mode 0600".into());
    }
    let token = std::fs::read_to_string(path)?.trim().to_string();
    if token.len() < 32 {
        return Err("token must contain at least 32 characters".into());
    }
    Ok(token)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let home = env::var("HOME")?;
    let config =
        PathBuf::from(env::var("JARVIS_CONFIG_DIR").unwrap_or(format!("{home}/.config/jarvis")));
    let vault = crypto::Vault::load(&config.join("key")).map_err(std::io::Error::other)?;
    let token = private_token(&config.join("token"))?;
    let gateway_token = private_token(&config.join("gateway-token"))?;
    let database_url = env::var("DATABASE_URL").map_err(|_| "DATABASE_URL is required")?;
    let provider = env::var("JARVIS_PROVIDER")
        .map_err(|_| "JARVIS_PROVIDER is required (anthropic, ollama, or explicit fake demo)")?;
    let model = env::var("JARVIS_MODEL").map_err(|_| "JARVIS_MODEL is required")?;
    let export = PathBuf::from(
        env::var("JARVIS_EXPORT_DIR").unwrap_or(format!("{home}/.local/share/jarvis/export")),
    )
    .canonicalize()?;
    let port: u16 = env::var("JARVIS_CORE_PORT")
        .unwrap_or("8790".into())
        .parse()?;
    let gateway = env::var("JARVIS_GATEWAY_URL").unwrap_or("http://127.0.0.1:8787".into());
    let gateway_url = reqwest::Url::parse(&gateway)?;
    if gateway_url.scheme() != "http"
        || !matches!(
            gateway_url.host_str(),
            Some("127.0.0.1" | "localhost" | "[::1]")
        )
        || gateway_url.path() != "/"
    {
        return Err("gateway must be a loopback HTTP origin".into());
    }
    let pool = store::connect(&database_url).await?;
    store::migrate(&pool).await?;
    // Refuse the wrong key before opening a listener.
    let sample: Option<(String, serde_json::Value)> =
        sqlx::query_as("SELECT task_id,body FROM turns LIMIT 1")
            .fetch_optional(&pool)
            .await?;
    if let Some((id, body)) = sample {
        vault.open(&id, &body).map_err(std::io::Error::other)?;
    }
    let holder = format!("jarvis-core/{}", std::process::id());
    let lease = leases::acquire(&pool, leases::CORE_LEASE, &holder, 30.0).await?;
    tracing::info!(lease=%lease.lease_name,holder=%lease.holder,expiry=%lease.expires_at,"worker lease acquired");
    if leases::is_stale(&pool, leases::CORE_LEASE, lease.fence).await? {
        return Err("worker lease superseded".into());
    }
    let report = recovery::recover_once(&pool).await?;
    tracing::info!(?report, "recovery sweep complete");
    // A restart never automatically repeats inference or an authorized read.
    // The owner resumes provider work, or explicitly reapproves a proposal.
    sqlx::query("UPDATE tasks SET state='waiting_provider',version=version+1 WHERE state IN ('queued','running') AND task_id IN (SELECT task_id FROM turns)").execute(&pool).await?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(31))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let connections = connections::Registry::load(&config.join("connections.json"))
        .map_err(std::io::Error::other)?;
    let public_origin = env::var("JARVIS_PUBLIC_ORIGIN").ok();
    if let Some(origin) = &public_origin {
        let u = reqwest::Url::parse(origin)?;
        if u.scheme() != "https"
            || u.host_str().is_none()
            || u.path() != "/"
            || !u.username().is_empty()
            || u.password().is_some()
            || u.query().is_some()
            || u.fragment().is_some()
            || origin.ends_with('/')
        {
            return Err(
                "JARVIS_PUBLIC_ORIGIN must be an HTTPS origin without path or trailing slash"
                    .into(),
            );
        }
    }
    let app = api::App {
        public_origin,
        connections: Arc::new(connections),
        pool: pool.clone(),
        vault,
        token,
        gateway_token,
        gateway,
        provider,
        model,
        export,
        fence: lease.fence,
        host: format!("127.0.0.1:{port}"),
        client,
        busy: Arc::new(tokio::sync::Mutex::new(())),
    };
    api::prune(&app)
        .await
        .map_err(|e| std::io::Error::other(e.1))?;
    let listener = tokio::net::TcpListener::bind(&app.host).await?;
    tracing::info!(address=%app.host,provider=%app.provider,model=%app.model,"JARVIS ready");
    let renewal_pool = pool.clone();
    let renewal_holder = holder.clone();
    let fence = lease.fence;
    let renewal = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            match leases::refresh(
                &renewal_pool,
                leases::CORE_LEASE,
                &renewal_holder,
                fence,
                30.0,
            )
            .await
            {
                Ok(true) => {}
                _ => {
                    tracing::error!("worker lease lost; shutting down");
                    std::process::exit(5);
                }
            }
        }
    });
    axum::serve(listener, api::router(app))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    renewal.abort();
    sqlx::query(
        "UPDATE leases SET expires_at=now() WHERE lease_name=$1 AND holder=$2 AND fence=$3",
    )
    .bind(leases::CORE_LEASE)
    .bind(holder)
    .bind(fence)
    .execute(&pool)
    .await?;
    Ok(())
}
