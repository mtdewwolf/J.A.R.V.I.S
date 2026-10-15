//! Authenticated local assistant. One owner, one active task, bounded turns.
use crate::{crypto::Vault, events, ids, inference, jcs, leases, tools};
use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, Transaction};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::Mutex;

pub const OWNER: &str = "owner";
#[derive(Clone)]
pub struct App {
    pub connections: Arc<crate::connections::Registry>,
    pub pool: PgPool,
    pub vault: Vault,
    pub token: String,
    pub gateway_token: String,
    pub gateway: String,
    pub provider: String,
    pub model: String,
    pub export: PathBuf,
    pub fence: i64,
    pub host: String,
    pub public_origin: Option<String>,
    pub client: reqwest::Client,
    pub busy: Arc<Mutex<()>>,
}
#[derive(Debug)]
pub struct ApiError(pub StatusCode, pub String);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error":self.1}))).into_response()
    }
}
impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        tracing::error!(error=%e,"database operation failed");
        Self(
            StatusCode::SERVICE_UNAVAILABLE,
            "durable storage unavailable".into(),
        )
    }
}
impl From<String> for ApiError {
    fn from(s: String) -> Self {
        Self(StatusCode::BAD_REQUEST, s)
    }
}
type Result<T> = std::result::Result<T, ApiError>;
fn conflict(message: &str) -> ApiError {
    ApiError(StatusCode::CONFLICT, message.into())
}

#[derive(Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Command {
    #[serde(rename = "pair.create")]
    PairCreate,
    #[serde(rename = "device.list")]
    DeviceList,
    #[serde(rename = "device.revoke")]
    DeviceRevoke { device_id: String },
    #[serde(rename = "task.repropose")]
    Repropose { task_id: String },
    #[serde(rename = "session.list")]
    SessionList,
    #[serde(rename = "connection.list")]
    ConnectionList,
    #[serde(rename = "connection.check")]
    ConnectionCheck { connection_id: String },
    #[serde(rename = "session.open")]
    SessionOpen,
    #[serde(rename = "input.send")]
    Input {
        session_id: String,
        request_id: String,
        text: String,
    },
    #[serde(rename = "task.get")]
    TaskGet { task_id: String },
    #[serde(rename = "task.list")]
    TaskList,
    #[serde(rename = "task.resume")]
    Resume { task_id: String },
    #[serde(rename = "task.cancel")]
    Cancel { task_id: String },
    #[serde(rename = "approval.grant")]
    Grant {
        task_id: String,
        proposal_id: String,
        digest: String,
    },
    #[serde(rename = "approval.deny")]
    Deny {
        task_id: String,
        proposal_id: String,
    },
    #[serde(rename = "events.since")]
    Events { session_id: String, cursor: i64 },
    #[serde(rename = "memory.propose")]
    MemoryPropose { key: String, value: String },
    #[serde(rename = "memory.confirm")]
    MemoryConfirm { memory_id: String, digest: String },
    #[serde(rename = "memory.list")]
    MemoryList,
    #[serde(rename = "memory.delete")]
    MemoryDelete { memory_id: String },
    #[serde(rename = "session.delete")]
    SessionDelete { session_id: String },
    #[serde(rename = "system.status")]
    Status,
}

pub fn router(app: App) -> Router {
    Router::new()
        .route(
            "/",
            get(|| async {
                asset(
                    "text/html; charset=utf-8",
                    include_str!("../../../clients/web/index.html"),
                )
            }),
        )
        .route(
            "/app.js",
            get(|| async {
                asset(
                    "text/javascript",
                    include_str!("../../../clients/web/app.js"),
                )
            }),
        )
        .route(
            "/app.css",
            get(|| async { asset("text/css", include_str!("../../../clients/web/app.css")) }),
        )
        .route(
            "/sw.js",
            get(|| async {
                asset(
                    "text/javascript",
                    include_str!("../../../clients/web/sw.js"),
                )
            }),
        )
        .route(
            "/manifest.webmanifest",
            get(|| async {
                asset(
                    "application/manifest+json",
                    include_str!("../../../clients/web/manifest.webmanifest"),
                )
            }),
        )
        .route(
            "/icon.svg",
            get(|| async { asset("image/svg+xml", include_str!("../../../assets/icon.svg")) }),
        )
        .route("/healthz", get(|| async { "ok\n" }))
        .route("/v1/command", post(command))
        .route("/v1/pair", post(pair))
        .layer(DefaultBodyLimit::max(128 * 1024))
        .with_state(app)
}
fn check_headers(app: &App, headers: &HeaderMap) -> Result<()> {
    let host = headers
        .get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let local_origin = format!("http://{}", app.host);
    let public_host = app
        .public_origin
        .as_ref()
        .and_then(|o| o.strip_prefix("https://"));
    if host != app.host && Some(host) != public_host {
        return Err(ApiError(StatusCode::FORBIDDEN, "unexpected Host".into()));
    }
    if let Some(origin) = headers.get("origin") {
        let origin = origin.to_str().unwrap_or("");
        if origin != local_origin && Some(origin) != app.public_origin.as_deref() {
            return Err(ApiError(StatusCode::FORBIDDEN, "unexpected Origin".into()));
        }
    }
    Ok(())
}
fn authenticate(app: &App, headers: &HeaderMap) -> Result<()> {
    check_headers(app, headers)?;
    let expected = format!("Bearer {}", app.token);
    let actual = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    // Hash first so the comparison length is fixed; tokens are random 256 bits.
    use sha2::{Digest, Sha256};
    let a = Sha256::digest(actual.as_bytes());
    let b = Sha256::digest(expected.as_bytes());
    let diff = a.iter().zip(b.iter()).fold(0u8, |d, (a, b)| d | (a ^ b));
    if diff != 0 {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "authentication required".into(),
        ));
    }
    Ok(())
}
async fn command(State(app): State<App>, headers: HeaderMap, bytes: Bytes) -> Result<Json<Value>> {
    if let Err(error) = authenticate(&app, &headers) {
        if error.0 != StatusCode::UNAUTHORIZED {
            return Err(error);
        }
        let raw = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .unwrap_or("");
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM devices WHERE owner_id=$1 AND token_hash=$2 AND NOT revoked)").bind(OWNER).bind(secret_hash(raw)).fetch_one(&app.pool).await?;
        if !valid {
            return Err(error);
        }
    }
    let value = crate::strict_json::parse(&bytes).map_err(|_| {
        ApiError(
            StatusCode::BAD_REQUEST,
            "invalid or duplicate-key JSON".into(),
        )
    })?;
    let cmd: Command = serde_json::from_value(value)
        .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "invalid command fields".into()))?;
    // Read/control commands remain available during a provider attempt.
    let output = match cmd {
        Command::PairCreate => {
            let code = random_token();
            let mut tx = app.pool.begin().await?;
            fence(&app, &mut tx).await?;
            sqlx::query("DELETE FROM pairing_codes WHERE expires_at<now() OR consumed")
                .execute(&mut *tx)
                .await?;
            sqlx::query("INSERT INTO pairing_codes(code_hash,owner_id) VALUES($1,$2)")
                .bind(secret_hash(&code))
                .bind(OWNER)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            json!({"code":code,"expires_in_seconds":120,"pair_url":format!("{}#pair={}",app.public_origin.clone().unwrap_or(format!("http://{}",app.host)),code),"phone_reachable":app.public_origin.is_some()})
        }
        Command::DeviceList => {
            let rows:Vec<(String,String,bool)> =sqlx::query_as("SELECT device_id,label,revoked FROM devices WHERE owner_id=$1 ORDER BY created_at DESC").bind(OWNER).fetch_all(&app.pool).await?;
            json!({"devices":rows.into_iter().map(|(device_id,label,revoked)|json!({"device_id":device_id,"label":label,"revoked":revoked})).collect::<Vec<_>>()})
        }
        Command::DeviceRevoke { device_id } => {
            let mut tx = app.pool.begin().await?;
            fence(&app, &mut tx).await?;
            sqlx::query("UPDATE devices SET revoked=true WHERE device_id=$1 AND owner_id=$2")
                .bind(&device_id)
                .bind(OWNER)
                .execute(&mut *tx)
                .await?;
            events::append_in_tx(
                &mut tx,
                OWNER,
                "device.revoked",
                json!({"device_id":device_id}),
            )
            .await?;
            tx.commit().await?;
            json!({"revoked":true,"device_id":device_id})
        }
        Command::SessionList => {
            let sessions:Vec<(String,String)> =sqlx::query_as("SELECT session_id,created_at::text FROM sessions WHERE owner_id=$1 ORDER BY created_at DESC LIMIT 100").bind(OWNER).fetch_all(&app.pool).await?;
            json!({"sessions":sessions.into_iter().map(|(session_id,created_at)|json!({"session_id":session_id,"created_at":created_at})).collect::<Vec<_>>()})
        }
        Command::ConnectionList => json!({"connections":app.connections.list()}),
        Command::ConnectionCheck { connection_id } => {
            app.connections.check(&app.client, &connection_id).await?
        }
        Command::Status => {
            json!({"provider":app.provider,"model":app.model,"mode":if app.provider=="fake"{"demo"}else{"configured"},"connections":[{"id":"workspace","status":"configured","tool":"workspace.read_text"}],"interface":"terminal-and-browser","voice":"browser dictation and spoken replies; device support required","public_origin":app.public_origin})
        }
        Command::TaskGet { task_id } => view(&app, &task_id).await?,
        Command::TaskList => {
            let rows: Vec<(String,)> = sqlx::query_as(
                "SELECT turns.task_id FROM turns JOIN tasks USING(task_id) WHERE turns.owner_id=$1 ORDER BY tasks.created_at DESC LIMIT 50",
            )
            .bind(OWNER)
            .fetch_all(&app.pool)
            .await?;
            let mut list = Vec::new();
            for (id,) in rows {
                list.push(view(&app, &id).await?);
            }
            json!({"tasks":list})
        }
        Command::Events { session_id, cursor } => {
            session(&app, &session_id).await?;
            let rows = events::replay(&app.pool, &session_id, cursor, 1000).await?;
            json!({"events":rows.into_iter().map(|e|json!({"cursor":e.id,"type":e.event_type,"task_id":e.task_id,"payload":e.payload,"occurred_at":e.created_at})).collect::<Vec<_>>()})
        }
        Command::Cancel { task_id } => cancel(&app, &task_id).await?,
        Command::MemoryList => json!({"memories":memory_list(&app).await?}),
        other => {
            let _guard = app.busy.try_lock().map_err(|_| {
                conflict("another operation is running; status and cancellation remain available")
            })?;
            match other {
                Command::SessionOpen => {
                    let id = ids::new_id("s");
                    sqlx::query("INSERT INTO sessions(session_id,owner_id) VALUES($1,$2)")
                        .bind(&id)
                        .bind(OWNER)
                        .execute(&app.pool)
                        .await?;
                    json!({"session_id":id})
                }
                Command::Input {
                    session_id,
                    request_id,
                    text,
                } => input(&app, &session_id, &request_id, &text).await?,
                Command::Repropose { task_id } => repropose(&app, &task_id).await?,
                Command::Resume { task_id } => {
                    let turn = load(&app, &task_id).await?;
                    if !matches!(
                        turn.state.as_str(),
                        "waiting_provider" | "queued" | "running"
                    ) {
                        return Err(conflict(
                            "task requires approval, review, or is already finished",
                        ));
                    }
                    run(&app, &task_id).await?
                }
                Command::Grant {
                    task_id,
                    proposal_id,
                    digest,
                } => grant(&app, &task_id, &proposal_id, &digest).await?,
                Command::Deny {
                    task_id,
                    proposal_id,
                } => {
                    let mut tx = app.pool.begin().await?;
                    fence(&app, &mut tx).await?;
                    let changed=sqlx::query("UPDATE proposals SET consumed=true WHERE proposal_id=$1 AND task_id=$2 AND owner_id=$3 AND NOT consumed").bind(&proposal_id).bind(&task_id).bind(OWNER).execute(&mut *tx).await?.rows_affected();
                    if changed != 1 {
                        return Err(conflict("proposal is no longer pending"));
                    }
                    sqlx::query("UPDATE tasks SET state='cancelled',version=version+1 WHERE task_id=$1 AND state IN ('waiting_approval','waiting_review')").bind(&task_id).execute(&mut *tx).await?;
                    audit(
                        &mut tx,
                        &task_id,
                        "approval.denied",
                        json!({"proposal_id":proposal_id}),
                    )
                    .await?;
                    tx.commit().await?;
                    view(&app, &task_id).await?
                }
                Command::MemoryPropose { key, value } => memory_propose(&app, &key, &value).await?,
                Command::MemoryConfirm { memory_id, digest } => {
                    memory_confirm(&app, &memory_id, &digest).await?
                }
                Command::MemoryDelete { memory_id } => memory_delete(&app, &memory_id).await?,
                Command::SessionDelete { session_id } => session_delete(&app, &session_id).await?,
                _ => unreachable!(),
            }
        }
    };
    Ok(Json(output))
}
async fn fence(app: &App, tx: &mut Transaction<'_, Postgres>) -> Result<()> {
    let current: Option<(i64,)> = sqlx::query_as(
        "SELECT fence FROM leases WHERE lease_name=$1 AND expires_at>now() FOR SHARE",
    )
    .bind(leases::CORE_LEASE)
    .fetch_optional(&mut **tx)
    .await?;
    if current.map(|r| r.0) != Some(app.fence) {
        return Err(ApiError(
            StatusCode::SERVICE_UNAVAILABLE,
            "worker lease expired or superseded".into(),
        ));
    }
    Ok(())
}
async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    task: &str,
    kind: &str,
    payload: Value,
) -> Result<()> {
    let sid: Option<(String,)> = sqlx::query_as("SELECT session_id FROM turns WHERE task_id=$1")
        .bind(task)
        .fetch_optional(&mut **tx)
        .await?;
    events::append_scoped_in_tx(
        tx,
        OWNER,
        kind,
        payload,
        sid.as_ref().map(|r| r.0.as_str()),
        Some(task),
    )
    .await?;
    Ok(())
}
async fn session(app: &App, id: &str) -> Result<()> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sessions WHERE session_id=$1 AND owner_id=$2)",
    )
    .bind(id)
    .bind(OWNER)
    .fetch_one(&app.pool)
    .await?;
    if !exists {
        return Err(ApiError(StatusCode::NOT_FOUND, "session not found".into()));
    }
    Ok(())
}
#[derive(sqlx::FromRow)]
struct Turn {
    memory_refs: Vec<String>,
    task_id: String,
    session_id: String,
    generation_id: String,
    provider: String,
    model: String,
    model_turns: i32,
    body: Value,
    state: String,
}
async fn load(app: &App, id: &str) -> Result<Turn> {
    sqlx::query_as("SELECT r.task_id,r.session_id,r.generation_id,r.provider,r.model,r.model_turns,r.body,t.state,r.memory_refs FROM turns r JOIN tasks t USING(task_id) WHERE r.task_id=$1 AND r.owner_id=$2").bind(id).bind(OWNER).fetch_optional(&app.pool).await?.ok_or(ApiError(StatusCode::NOT_FOUND,"task not found".into()))
}
async fn view(app: &App, id: &str) -> Result<Value> {
    let t = load(app, id).await?;
    let body = app.vault.open(id, &t.body)?;
    let pending:Option<(String,String,Value,bool,String,i64)> =sqlx::query_as("SELECT proposal_id,digest,body,expires_at>now(),tool,fence FROM proposals WHERE task_id=$1 AND NOT consumed ORDER BY proposal_id LIMIT 1").bind(id).fetch_optional(&app.pool).await?;
    let proposal = if let Some((pid, digest, sealed, valid, tool, proposal_fence)) = pending {
        let args = app.vault.open(&pid, &sealed)?;
        json!({"proposal_id":pid,"digest":digest,"tool":tool,"version":1,"arguments":args,"risk":"R1","max_uses":1,"expires_in_seconds":60,"expired":!valid,"disclosure":if t.provider=="ollama"{"file contents sent to configured local model"}else if t.provider=="fake"{"demo provider; no external model"}else{"file contents sent to configured hosted model"},"connection_disclosure":if tool=="connection.call"{app.connections.description(&args).unwrap_or("connection configuration changed; new proposal required".into())}else{"owner export directory only".into()},"reapproval_required":t.state=="waiting_review" || proposal_fence!=app.fence})
    } else {
        Value::Null
    };
    Ok(
        json!({"task_id":t.task_id,"session_id":t.session_id,"state":t.state,"provider":t.provider,"model":t.model,"model_turns":t.model_turns,"input":body["messages"][0]["content"],"response":body["response"],"error":body["error"],"proposal":proposal,"demo":t.provider=="fake"}),
    )
}
async fn input(app: &App, sid: &str, request: &str, text: &str) -> Result<Value> {
    session(app, sid).await?;
    if text.trim().is_empty() || text.len() > 32768 || request.is_empty() || request.len() > 128 {
        return Err("input or request ID exceeds limits".to_string().into());
    }
    let replay: Option<(String,)> =
        sqlx::query_as("SELECT task_id FROM turns WHERE owner_id=$1 AND request_id=$2")
            .bind(OWNER)
            .bind(request)
            .fetch_optional(&app.pool)
            .await?;
    if let Some((id,)) = replay {
        return view(app, &id).await;
    }
    let mut tx = app.pool.begin().await?;
    fence(app, &mut tx).await?;
    let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tasks JOIN turns USING(task_id) WHERE turns.owner_id=$1 AND state NOT IN ('succeeded','failed','cancelled'))").bind(OWNER).fetch_one(&mut *tx).await?;
    if active {
        return Err(conflict("finish, cancel, or resume the active task first"));
    }
    let id = ids::new_id("tsk");
    let cmd = ids::new_id("cmd");
    let generation = ids::new_id("g");
    let body=app.vault.seal(&id,&json!({"messages":[{"role":"user","content":text,"untrusted":true}],"response":null,"error":null}))?;
    sqlx::query("INSERT INTO commands(command_id,owner_id,request_id,command_type,payload,state) VALUES($1,$2,$3,'input.send','{}','accepted')").bind(&cmd).bind(OWNER).bind(request).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO tasks(task_id,command_id,owner_id,state,checkpoint) VALUES($1,$2,$3,'queued','{}')").bind(&id).bind(&cmd).bind(OWNER).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO turns(task_id,session_id,owner_id,request_id,generation_id,provider,model,body) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(&id).bind(sid).bind(OWNER).bind(request).bind(generation).bind(&app.provider).bind(&app.model).bind(body).execute(&mut *tx).await?;
    audit(&mut tx, &id, "input.accepted", json!({"task_id":id})).await?;
    tx.commit().await?;
    run(app, &id).await
}
async fn run(app: &App, id: &str) -> Result<Value> {
    let t = load(app, id).await?;
    let mut body = app.vault.open(id, &t.body)?;
    if t.model_turns >= crate::limits::MAX_MODEL_TURNS as i32 {
        return fail(app, id, body, "model turn budget exhausted", "failed").await;
    }
    let mut tx = app.pool.begin().await?;
    fence(app, &mut tx).await?;
    let updated=sqlx::query("UPDATE tasks SET state='running',version=version+1 WHERE task_id=$1 AND state IN ('queued','running','waiting_provider')").bind(id).execute(&mut *tx).await?.rows_affected();
    if updated != 1 {
        return Err(conflict("task is no longer runnable"));
    }
    sqlx::query("UPDATE turns SET model_turns=model_turns+1 WHERE task_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        id,
        "response.started",
        json!({"task_id":id,"turn":t.model_turns+1}),
    )
    .await?;
    tx.commit().await?;
    let history:Vec<(String,Value,Vec<String>)> =sqlx::query_as("SELECT r.task_id,r.body,r.memory_refs FROM turns r JOIN tasks t USING(task_id) WHERE session_id=$1 AND r.owner_id=$2 AND t.state='succeeded' ORDER BY t.created_at DESC LIMIT 12").bind(&t.session_id).bind(OWNER).fetch_all(&app.pool).await?;
    let mut messages = Vec::new();
    let mut used = body["messages"].to_string().len();
    let mut previous = Vec::new();
    let mut memory_refs = t.memory_refs.clone();
    for (hid, sealed, refs) in history {
        let h = app.vault.open(&hid, &sealed)?;
        if !h["response"].is_string() {
            continue;
        }
        let size = h["messages"][0].to_string().len() + h["response"].to_string().len();
        if used + size > 96 * 1024 {
            break;
        }
        used += size;
        memory_refs.extend(refs);
        previous.push(h);
    }
    for h in previous.into_iter().rev() {
        messages.push(h["messages"][0].clone());
        messages.push(json!({"role":"assistant","content":h["response"],"untrusted":true}));
    }
    messages.extend(
        body["messages"]
            .as_array()
            .ok_or("invalid stored messages".to_string())?
            .iter()
            .cloned(),
    );
    let all_memories = memory_list(app).await?;
    let mut memories = Vec::new();
    let mut memory_bytes = 0;
    for memory in all_memories {
        let size = memory.to_string().len();
        if memory_bytes + size > 32768 {
            break;
        }
        memory_bytes += size;
        memory_refs.push(memory["memory_id"].as_str().unwrap().to_string());
        memories.push(memory);
    }
    memory_refs.sort();
    memory_refs.dedup();
    let mut tx = app.pool.begin().await?;
    fence(app, &mut tx).await?;
    let running: bool =
        sqlx::query_scalar("SELECT state='running' FROM tasks WHERE task_id=$1 FOR UPDATE")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if !running {
        tx.rollback().await?;
        return view(app, id).await;
    }
    sqlx::query("UPDATE turns SET memory_refs=$1 WHERE task_id=$2")
        .bind(&memory_refs)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let request = inference::request(
        &t.generation_id,
        &t.provider,
        &t.model,
        json!(messages),
        &json!(memories).to_string(),
        &app.connections,
    );
    let answer = match tokio::time::timeout(
        std::time::Duration::from_secs(31),
        inference::infer(
            &app.client,
            &app.gateway,
            &app.gateway_token,
            request,
            &app.connections,
        ),
    )
    .await
    {
        Ok(Ok(a)) => a,
        Ok(Err(e)) => return fail(app, id, body, &e, "waiting_provider").await,
        Err(_) => {
            return fail(
                app,
                id,
                body,
                "provider deadline exceeded",
                "waiting_provider",
            )
            .await
        }
    };
    let mut tx = app.pool.begin().await?;
    fence(app, &mut tx).await?;
    let state: String = sqlx::query_scalar("SELECT state FROM tasks WHERE task_id=$1 FOR UPDATE")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if state != "running" {
        tx.rollback().await?;
        return view(app, id).await;
    }
    body["error"] = Value::Null;
    if let Some((tool, args)) = answer.proposal {
        let pid = ids::new_id("prp");
        let step = ids::new_id("stp");
        let digest = jcs::canonical_digest(&args)
            .map_err(|_| "cannot canonicalize arguments".to_string())?;
        sqlx::query("INSERT INTO steps(step_id,task_id,task_version,kind,state) SELECT $1,task_id,version,$3,'planned' FROM tasks WHERE task_id=$2").bind(&step).bind(id).bind(&tool).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO proposals(proposal_id,task_id,generation_id,step_id,owner_id,tool,digest,body,fence,config_digest) VALUES($1,$2,$3,$4,$5,$9,$6,$7,$8,$10)").bind(&pid).bind(id).bind(&t.generation_id).bind(&step).bind(OWNER).bind(&digest).bind(app.vault.seal(&pid,&args)?).bind(app.fence).bind(&tool).bind(&app.connections.fingerprint).execute(&mut *tx).await?;
        // Store model proposal as context, never as a committed answer.
        body["messages"].as_array_mut().unwrap().push(json!({"role":"assistant","content":format!("Proposed {tool} with {}. {}",args,answer.text),"untrusted":true}));
        sqlx::query("UPDATE tasks SET state='waiting_approval',version=version+1 WHERE task_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        audit(
            &mut tx,
            id,
            "tool.proposed",
            json!({"proposal_id":pid,"tool":tool,"step_id":step}),
        )
        .await?;
    } else {
        body["response"] = json!(answer.text);
        sqlx::query("UPDATE tasks SET state='succeeded',version=version+1 WHERE task_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        audit(
            &mut tx,
            id,
            "response.committed",
            json!({"task_id":id,"provider":t.provider,"model":t.model}),
        )
        .await?;
    }
    sqlx::query("UPDATE turns SET body=$1 WHERE task_id=$2")
        .bind(app.vault.seal(id, &body)?)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    view(app, id).await
}
async fn fail(app: &App, id: &str, mut body: Value, error: &str, state: &str) -> Result<Value> {
    let mut tx = app.pool.begin().await?;
    fence(app, &mut tx).await?;
    let changed = sqlx::query(
        "UPDATE tasks SET state=$1,version=version+1 WHERE task_id=$2 AND state IN ('running','waiting_provider','queued')",
    )
    .bind(state)
    .bind(id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if changed == 1 {
        body["error"] = json!(error);
        sqlx::query("UPDATE turns SET body=$1 WHERE task_id=$2")
            .bind(app.vault.seal(id, &body)?)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        audit(
            &mut tx,
            id,
            "degraded.notice",
            json!({"reason":"provider_or_budget_failure"}),
        )
        .await?;
    }
    tx.commit().await?;
    view(app, id).await
}
async fn cancel(app: &App, id: &str) -> Result<Value> {
    load(app, id).await?;
    let mut tx = app.pool.begin().await?;
    fence(app, &mut tx).await?;
    sqlx::query("UPDATE tasks SET state='cancelled',version=version+1 WHERE task_id=$1 AND state NOT IN ('succeeded','failed','cancelled')").bind(id).execute(&mut *tx).await?;
    sqlx::query("UPDATE proposals SET consumed=true WHERE task_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    audit(&mut tx, id, "task.cancelled", json!({"task_id":id})).await?;
    tx.commit().await?;
    view(app, id).await
}

#[derive(sqlx::FromRow)]
struct ApprovalRow {
    digest: String,
    body: Value,
    step_id: String,
    generation_id: String,
    valid: bool,
    fence: i64,
    tool: String,
    config_digest: String,
}

async fn grant(app: &App, id: &str, pid: &str, digest: &str) -> Result<Value> {
    let t = load(app, id).await?;
    let mut tx = app.pool.begin().await?;
    fence(app, &mut tx).await?;
    let state: String = sqlx::query_scalar("SELECT state FROM tasks WHERE task_id=$1 FOR UPDATE")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if !matches!(state.as_str(), "waiting_approval" | "waiting_review") {
        return Err(conflict("task is not awaiting approval"));
    }
    let row:Option<ApprovalRow> =sqlx::query_as("SELECT digest,body,step_id,generation_id,expires_at>now() AS valid,fence,tool,config_digest FROM proposals WHERE proposal_id=$1 AND task_id=$2 AND owner_id=$3 AND NOT consumed FOR UPDATE").bind(pid).bind(id).bind(OWNER).fetch_optional(&mut *tx).await?;
    let ApprovalRow {
        digest: stored,
        body: sealed,
        step_id: step,
        generation_id: generation,
        valid,
        fence: proposal_fence,
        tool,
        config_digest,
    } = row.ok_or(conflict("proposal already used or missing"))?;
    if stored != digest || generation != t.generation_id {
        return Err(conflict("approval digest or generation mismatch"));
    }
    // Fresh explicit reapproval after restart/review may renew proposal expiry,
    // but only this owner endpoint can do so. The model and recovery never can.
    if !valid && state != "waiting_review" && proposal_fence == app.fence {
        return Err(conflict("proposal expired; request a fresh proposal"));
    }
    let args = app.vault.open(pid, &sealed)?;
    if tool == "connection.call" && config_digest != app.connections.fingerprint {
        return Err(conflict(
            "connection configuration changed; fresh proposal required",
        ));
    }
    tools::validate_proposal(&tool, &args, &app.connections)?;
    let execution = ids::new_id("exe");
    sqlx::query("UPDATE proposals SET consumed=true WHERE proposal_id=$1")
        .bind(pid)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE steps SET state='dispatched',execution_id=$1,outcome=NULL WHERE step_id=$2",
    )
    .bind(&execution)
    .bind(&step)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE tasks SET state='running',version=version+1 WHERE task_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        id,
        "tool.dispatched",
        json!({"proposal_id":pid,"execution_id":execution,"step_id":step}),
    )
    .await?;
    tx.commit().await?;
    let outcome: std::result::Result<Value, String> = if tool == "connection.call" {
        match tokio::time::timeout(
            std::time::Duration::from_secs(20),
            app.connections.execute(&app.client, &args),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err("connection operation timed out; outcome unknown".into()),
        }
    } else {
        let root = app.export.clone();
        let args: tools::ReadArgs =
            serde_json::from_value(args).map_err(|_| "invalid read arguments".to_string())?;
        match tokio::time::timeout(
            std::time::Duration::from_secs(2),
            tokio::task::spawn_blocking(move || tools::read(&root, &args)),
        )
        .await
        {
            Ok(Ok(result)) => result.map(|text| json!({"text":text})),
            _ => Err("read interrupted or timed out".into()),
        }
    };
    let content = match outcome {
        Ok(result) => json!({"ok":true,"result":result}),
        Err(error) => json!({"ok":false,"error":error}),
    };
    let mut tx = app.pool.begin().await?;
    fence(app, &mut tx).await?;
    let state: String = sqlx::query_scalar("SELECT state FROM tasks WHERE task_id=$1 FOR UPDATE")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query("UPDATE steps SET state=$1,outcome=$2 WHERE step_id=$3 AND execution_id=$4")
        .bind(if content["ok"] == true {
            "completed"
        } else {
            "failed"
        })
        .bind(if content["ok"] == true {
            "success"
        } else {
            "failure"
        })
        .bind(&step)
        .bind(&execution)
        .execute(&mut *tx)
        .await?;
    if state != "running" {
        tx.commit().await?;
        return view(app, id).await;
    }
    let mut body = app.vault.open(id, &t.body)?;
    body["messages"]
        .as_array_mut()
        .unwrap()
        .push(json!({"role":"tool","content":content.to_string(),"untrusted":true}));
    sqlx::query("UPDATE turns SET body=$1 WHERE task_id=$2")
        .bind(app.vault.seal(id, &body)?)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        id,
        "tool.result",
        json!({"execution_id":execution,"ok":content["ok"]}),
    )
    .await?;
    tx.commit().await?;
    run(app, id).await
}

async fn memory_list(app: &App) -> Result<Vec<Value>> {
    let rows:Vec<(String,Value)> =sqlx::query_as("SELECT memory_id,body FROM memories WHERE owner_id=$1 AND confirmed AND expires_at>now() ORDER BY created_at LIMIT 100").bind(OWNER).fetch_all(&app.pool).await?;
    let mut out = Vec::new();
    for (id, sealed) in rows {
        let mut body = app.vault.open(&id, &sealed)?;
        body["memory_id"] = json!(id);
        out.push(body);
    }
    Ok(out)
}
async fn memory_propose(app: &App, key: &str, value: &str) -> Result<Value> {
    if key.trim().is_empty() || key.len() > 128 || value.len() > 2048 {
        return Err("memory key/value exceeds limits".to_string().into());
    }
    let body = json!({"key":key,"value":value,"scope":"owner","status":"confirmed"});
    let id = ids::new_id("mem");
    let digest = jcs::canonical_digest(&body).map_err(|_| "invalid memory content".to_string())?;
    let mut tx = app.pool.begin().await?;
    fence(app, &mut tx).await?;
    sqlx::query(
        "INSERT INTO memories(memory_id,owner_id,key_hash,digest,body) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(&id)
    .bind(OWNER)
    .bind(app.vault.key_hash(key))
    .bind(&digest)
    .bind(app.vault.seal(&id, &body)?)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(json!({"memory_id":id,"digest":digest,"content":body,"requires_confirmation":true}))
}
async fn memory_confirm(app: &App, id: &str, digest: &str) -> Result<Value> {
    let mut tx = app.pool.begin().await?;
    fence(app, &mut tx).await?;
    let row:Option<(String,String)> =sqlx::query_as("SELECT digest,key_hash FROM memories WHERE memory_id=$1 AND owner_id=$2 AND NOT confirmed AND expires_at>now() FOR UPDATE").bind(id).bind(OWNER).fetch_optional(&mut *tx).await?;
    let (stored, key_hash) =
        row.ok_or(conflict("memory candidate missing, confirmed or expired"))?;
    if stored != digest {
        return Err(conflict("exact memory content digest mismatch"));
    }
    // Exact confirmation explicitly supersedes the previous value of this key.
    let previous: Vec<(String,)> = sqlx::query_as(
        "SELECT memory_id FROM memories WHERE owner_id=$1 AND key_hash=$2 AND confirmed",
    )
    .bind(OWNER)
    .bind(&key_hash)
    .fetch_all(&mut *tx)
    .await?;
    for (old,) in previous {
        erase_derived(app, &mut tx, &old).await?;
    }
    sqlx::query("INSERT INTO erasures(id,owner_id) SELECT memory_id,owner_id FROM memories WHERE owner_id=$1 AND key_hash=$2 AND confirmed ON CONFLICT DO NOTHING").bind(OWNER).bind(&key_hash).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM memories WHERE owner_id=$1 AND key_hash=$2 AND confirmed")
        .bind(OWNER)
        .bind(&key_hash)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE memories SET confirmed=true,expires_at=now()+interval '30 days' WHERE memory_id=$1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    events::append_in_tx(&mut tx, OWNER, "memory.confirmed", json!({"memory_id":id})).await?;
    tx.commit().await?;
    Ok(json!({"confirmed":true,"memory_id":id}))
}
async fn memory_delete(app: &App, id: &str) -> Result<Value> {
    let mut tx = app.pool.begin().await?;
    fence(app, &mut tx).await?;
    erase_derived(app, &mut tx, id).await?;
    sqlx::query("DELETE FROM memories WHERE memory_id=$1 AND owner_id=$2")
        .bind(id)
        .bind(OWNER)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO erasures(id,owner_id) VALUES($1,$2) ON CONFLICT DO NOTHING")
        .bind(id)
        .bind(OWNER)
        .execute(&mut *tx)
        .await?;
    events::append_in_tx(&mut tx, OWNER, "memory.deleted", json!({"memory_id":id})).await?;
    tx.commit().await?;
    Ok(
        json!({"deleted":true,"memory_id":id,"note":"memory and tracked derived conversation payloads erased; backups are not enabled"}),
    )
}
async fn session_delete(app: &App, sid: &str) -> Result<Value> {
    session(app, sid).await?;
    let mut tx = app.pool.begin().await?;
    fence(app, &mut tx).await?;
    let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM turns JOIN tasks USING(task_id) WHERE session_id=$1 AND state NOT IN ('succeeded','failed','cancelled'))").bind(sid).fetch_one(&mut *tx).await?;
    if active {
        return Err(conflict(
            "cancel the active task before deleting its session",
        ));
    }
    sqlx::query(
        "DELETE FROM proposals WHERE task_id IN (SELECT task_id FROM turns WHERE session_id=$1)",
    )
    .bind(sid)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM turns WHERE session_id=$1")
        .bind(sid)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM sessions WHERE session_id=$1 AND owner_id=$2")
        .bind(sid)
        .bind(OWNER)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO erasures(id,owner_id) VALUES($1,$2) ON CONFLICT DO NOTHING")
        .bind(sid)
        .bind(OWNER)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(json!({"deleted":true,"session_id":sid}))
}

async fn erase_derived(app: &App, tx: &mut Transaction<'_, Postgres>, memory: &str) -> Result<()> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT task_id FROM turns WHERE owner_id=$1 AND $2=ANY(memory_refs) FOR UPDATE",
    )
    .bind(OWNER)
    .bind(memory)
    .fetch_all(&mut **tx)
    .await?;
    for (id,) in rows {
        let redacted=app.vault.seal(&id,&json!({"messages":[{"role":"user","content":"[erased with associated memory]","untrusted":true}],"response":null,"error":"Conversation content erased with associated memory"}))?;
        sqlx::query("DELETE FROM proposals WHERE task_id=$1")
            .bind(&id)
            .execute(&mut **tx)
            .await?;
        sqlx::query("UPDATE turns SET body=$1,memory_refs='{}' WHERE task_id=$2")
            .bind(redacted)
            .bind(&id)
            .execute(&mut **tx)
            .await?;
        sqlx::query("UPDATE tasks SET state='cancelled',version=version+1 WHERE task_id=$1 AND state NOT IN ('succeeded','failed','cancelled')").bind(&id).execute(&mut **tx).await?;
    }
    Ok(())
}

pub async fn prune(app: &App) -> Result<()> {
    let mut tx = app.pool.begin().await?;
    fence(app, &mut tx).await?;
    let expired: Vec<(String,)> =
        sqlx::query_as("SELECT memory_id FROM memories WHERE expires_at<now() AND owner_id=$1")
            .bind(OWNER)
            .fetch_all(&mut *tx)
            .await?;
    for (id,) in expired {
        erase_derived(app, &mut tx, &id).await?;
        sqlx::query("INSERT INTO erasures(id,owner_id) VALUES($1,$2) ON CONFLICT DO NOTHING")
            .bind(&id)
            .bind(OWNER)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("DELETE FROM memories WHERE expires_at<now() AND owner_id=$1")
        .bind(OWNER)
        .execute(&mut *tx)
        .await?;
    let old:Vec<(String,)> =sqlx::query_as("SELECT turns.task_id FROM turns JOIN tasks USING(task_id) WHERE tasks.created_at<now()-interval '30 days' AND turns.owner_id=$1").bind(OWNER).fetch_all(&mut *tx).await?;
    for (id,) in old {
        let body = app.vault.seal(
            &id,
            &json!({"messages":[],"response":null,"error":"Payload expired after 30 days"}),
        )?;
        sqlx::query("DELETE FROM proposals WHERE task_id=$1")
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE turns SET body=$1,memory_refs='{}' WHERE task_id=$2")
            .bind(body)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE tasks SET state='cancelled',version=version+1 WHERE task_id=$1 AND state NOT IN ('succeeded','failed','cancelled')").bind(&id).execute(&mut *tx).await?;
    }
    sqlx::query("DELETE FROM events WHERE created_at<now()-interval '90 days'")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

fn asset(content_type: &str, body: &'static str) -> Response {
    let mut response = body.into_response();
    let headers = response.headers_mut();
    headers.insert("content-type", content_type.parse().unwrap());
    headers.insert("content-security-policy","default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'".parse().unwrap());
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    headers.insert("referrer-policy", "no-referrer".parse().unwrap());
    headers.insert("cache-control", "no-cache".parse().unwrap());
    response
}

async fn repropose(app: &App, id: &str) -> Result<Value> {
    let t = load(app, id).await?;
    let mut tx = app.pool.begin().await?;
    fence(app, &mut tx).await?;
    let state: String = sqlx::query_scalar("SELECT state FROM tasks WHERE task_id=$1 FOR UPDATE")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if !matches!(state.as_str(), "waiting_review" | "waiting_approval") {
        return Err(conflict("task is not awaiting tool review"));
    }
    let old:Option<(String,String,Value)> =sqlx::query_as("SELECT proposal_id,tool,body FROM proposals WHERE task_id=$1 ORDER BY expires_at DESC LIMIT 1").bind(id).fetch_optional(&mut *tx).await?;
    let (old_id, tool, sealed) = old.ok_or(conflict("no recoverable proposal"))?;
    let args = app.vault.open(&old_id, &sealed)?;
    tools::validate_proposal(&tool, &args, &app.connections)?;
    let pid = ids::new_id("prp");
    let step = ids::new_id("stp");
    let digest = jcs::canonical_digest(&args).map_err(|_| "invalid arguments".to_string())?;
    sqlx::query("UPDATE proposals SET consumed=true WHERE task_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO steps(step_id,task_id,task_version,kind,state) SELECT $1,task_id,version,$3,'planned' FROM tasks WHERE task_id=$2").bind(&step).bind(id).bind(&tool).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO proposals(proposal_id,task_id,generation_id,step_id,owner_id,tool,digest,body,fence,config_digest) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(&pid).bind(id).bind(&t.generation_id).bind(&step).bind(OWNER).bind(&tool).bind(digest).bind(app.vault.seal(&pid,&args)?).bind(app.fence).bind(&app.connections.fingerprint).execute(&mut *tx).await?;
    sqlx::query("UPDATE tasks SET state='waiting_approval',version=version+1 WHERE task_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    audit(&mut tx,id,"tool.reproposed",json!({"proposal_id":pid,"previous_proposal_id":old_id,"previous_outcome":if state=="waiting_review"{"unknown"}else{"not_executed"}})).await?;
    tx.commit().await?;
    view(app, id).await
}

fn random_token() -> String {
    use rand::RngCore;
    let mut bytes = [0; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn secret_hash(value: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PairRequest {
    code: String,
    label: String,
}
async fn pair(State(app): State<App>, headers: HeaderMap, bytes: Bytes) -> Result<Json<Value>> {
    check_headers(&app, &headers)?;
    let body = crate::strict_json::parse(&bytes).map_err(|_| "invalid pairing JSON".to_string())?;
    let request: PairRequest =
        serde_json::from_value(body).map_err(|_| "invalid pairing request".to_string())?;
    if request.code.len() != 64 || request.label.is_empty() || request.label.len() > 128 {
        return Err("invalid pairing code or device name".to_string().into());
    }
    let mut tx = app.pool.begin().await?;
    fence(&app, &mut tx).await?;
    let consumed=sqlx::query("UPDATE pairing_codes SET consumed=true WHERE code_hash=$1 AND owner_id=$2 AND NOT consumed AND expires_at>now()").bind(secret_hash(&request.code)).bind(OWNER).execute(&mut *tx).await?.rows_affected();
    if consumed != 1 {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "pairing code expired or already used".into(),
        ));
    }
    let device = ids::new_id("dev");
    let token = random_token();
    sqlx::query("INSERT INTO devices(device_id,owner_id,label,token_hash) VALUES($1,$2,$3,$4)")
        .bind(&device)
        .bind(OWNER)
        .bind(&request.label)
        .bind(secret_hash(&token))
        .execute(&mut *tx)
        .await?;
    events::append_in_tx(&mut tx, OWNER, "device.paired", json!({"device_id":device})).await?;
    tx.commit().await?;
    Ok(Json(json!({"device_id":device,"token":token})))
}

#[cfg(test)]
mod tests {
    use super::*;
    pub async fn app(pool: PgPool) -> App {
        let grant = leases::acquire(&pool, leases::CORE_LEASE, "test", 60.0)
            .await
            .unwrap();
        App {
            connections: Arc::new(crate::connections::Registry::default()),
            pool,
            vault: Vault::test(),
            token: "test-token".into(),
            gateway_token: "gateway-test".into(),
            gateway: "http://127.0.0.1:1".into(),
            provider: "fake".into(),
            model: "plain".into(),
            export: PathBuf::from("/missing"),
            fence: grant.fence,
            host: "127.0.0.1:8790".into(),
            public_origin: None,
            client: reqwest::Client::new(),
            busy: Arc::new(Mutex::new(())),
        }
    }
    #[sqlx::test(migrations = "../../migrations")]
    async fn exact_confirmation_supersession_and_erasure(pool: PgPool) {
        let a = app(pool).await;
        let p = memory_propose(&a, "name", "Tony").await.unwrap();
        let id = p["memory_id"].as_str().unwrap();
        assert!(memory_list(&a).await.unwrap().is_empty());
        assert!(memory_confirm(&a, id, "wrong").await.is_err());
        memory_confirm(&a, id, p["digest"].as_str().unwrap())
            .await
            .unwrap();
        assert_eq!(memory_list(&a).await.unwrap()[0]["value"], "Tony");
        let p = memory_propose(&a, "name", "Pepper").await.unwrap();
        let id = p["memory_id"].as_str().unwrap();
        memory_confirm(&a, id, p["digest"].as_str().unwrap())
            .await
            .unwrap();
        assert_eq!(memory_list(&a).await.unwrap().len(), 1);
        memory_delete(&a, id).await.unwrap();
        assert!(memory_list(&a).await.unwrap().is_empty());
    }
    #[sqlx::test(migrations = "../../migrations")]
    async fn authenticated_commands_and_fence_checks(pool: PgPool) {
        let a = app(pool).await;
        let mut h = HeaderMap::new();
        h.insert("host", a.host.parse().unwrap());
        assert!(authenticate(&a, &h).is_err());
        h.insert("authorization", "Bearer test-token".parse().unwrap());
        assert!(authenticate(&a, &h).is_ok());
        h.insert("origin", "https://evil.example".parse().unwrap());
        assert!(authenticate(&a, &h).is_err());
        sqlx::query("UPDATE leases SET fence=fence+1")
            .execute(&a.pool)
            .await
            .unwrap();
        let mut tx = a.pool.begin().await.unwrap();
        assert!(fence(&a, &mut tx).await.is_err());
    }
    #[sqlx::test(migrations = "../../migrations")]
    async fn forgetting_memory_erases_derived_turn_and_cancels_pending_work(pool: PgPool) {
        let a = app(pool).await;
        let candidate = memory_propose(&a, "name", "Tony").await.unwrap();
        let memory = candidate["memory_id"].as_str().unwrap();
        memory_confirm(&a, memory, candidate["digest"].as_str().unwrap())
            .await
            .unwrap();
        sqlx::query("INSERT INTO sessions(session_id,owner_id) VALUES('s_test',$1)")
            .bind(OWNER)
            .execute(&a.pool)
            .await
            .unwrap();
        let task = input(&a, "s_test", "test-request", "What is my name?")
            .await
            .unwrap();
        assert_eq!(task["state"], "waiting_provider");
        memory_delete(&a, memory).await.unwrap();
        let erased = view(&a, task["task_id"].as_str().unwrap()).await.unwrap();
        assert_eq!(erased["state"], "cancelled");
        assert!(erased["response"].is_null());
        assert!(!erased.to_string().contains("What is my name?"));
    }
    #[sqlx::test(migrations = "../../migrations")]
    async fn pairing_is_one_use_and_device_revocation_denies_access(pool: PgPool) {
        let a = app(pool).await;
        let code = random_token();
        sqlx::query("INSERT INTO pairing_codes(code_hash,owner_id) VALUES($1,$2)")
            .bind(secret_hash(&code))
            .bind(OWNER)
            .execute(&a.pool)
            .await
            .unwrap();
        let mut h = HeaderMap::new();
        h.insert("host", a.host.parse().unwrap());
        let bytes = Bytes::from(json!({"code":code,"label":"Android"}).to_string());
        let Json(device) = pair(State(a.clone()), h.clone(), bytes.clone())
            .await
            .unwrap();
        assert!(pair(State(a.clone()), h.clone(), bytes).await.is_err());
        h.insert(
            "authorization",
            format!("Bearer {}", device["token"].as_str().unwrap())
                .parse()
                .unwrap(),
        );
        let status = Bytes::from(json!({"type":"system.status"}).to_string());
        assert!(command(State(a.clone()), h.clone(), status.clone())
            .await
            .is_ok());
        sqlx::query("UPDATE devices SET revoked=true")
            .execute(&a.pool)
            .await
            .unwrap();
        assert_eq!(
            command(State(a), h, status).await.unwrap_err().0,
            StatusCode::UNAUTHORIZED
        );
    }
}
