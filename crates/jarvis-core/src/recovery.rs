//! Recovery sweep: restart-safe resumption that never renews grants.
//!
//! On startup the core resumes from durable state instead of trusting anything
//! in memory. The contract (architecture.md, "Durable task state"):
//!
//! - a checkpointed running task resumes at its checkpoint (AC-17);
//! - a dispatched step whose outcome never committed moves the task to
//!   `waiting_review` and the step to `outcome_unknown` — the original grant
//!   is neither renewed nor reused, and rereading needs fresh approval (AC-18);
//! - an authorized step that never dispatched stays unauthorized for
//!   execution: the task moves to `waiting_approval` and recovery executes
//!   nothing (AC-19);
//! - an unknown outcome is reported as unknown, never invented (AC-23).
//!
//! Every transition is idempotent: a second sweep finds nothing to do.

use serde_json::{json, Value};
use sqlx::PgPool;

use crate::events;
use crate::tasks::{TaskError, TaskState};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct RecoveryReport {
    /// Tasks with a durable checkpoint that were resumed (AC-17).
    pub resumed: usize,
    /// Dispatched steps whose outcome never committed (AC-18, AC-23).
    pub awaiting_review: usize,
    /// Authorized steps awaiting fresh approval after restart (AC-19).
    pub reapproval_required: usize,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct OpenTask {
    task_id: String,
    owner_id: String,
    state: String,
    checkpoint: Value,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct CrashedStep {
    step_id: String,
    task_id: String,
    owner_id: String,
    task_version: i64,
    execution_id: Option<String>,
}

pub async fn recover_once(pool: &PgPool) -> Result<RecoveryReport, sqlx::Error> {
    let mut report = RecoveryReport::default();

    let open_tasks: Vec<OpenTask> = sqlx::query_as::<_, OpenTask>(
        r#"
        SELECT task_id, owner_id, state, checkpoint
        FROM tasks
        WHERE state IN ('queued', 'running', 'waiting_approval', 'waiting_provider')
        ORDER BY created_at
        "#,
    )
    .fetch_all(pool)
    .await?;

    // AC-17: checkpointed running tasks resume exactly once. The SQL filter
    // already excludes terminal tasks; the typed check below is defense in
    // depth so an unexpected stored state cannot be acted upon.
    for task in &open_tasks {
        if matches!(parse_state(&task.state), Ok(state) if state.is_terminal()) {
            continue;
        }
        if task.state != "running" {
            continue;
        }
        report.resumed += resume_checkpoint(pool, task).await?;
    }

    // AC-18/AC-23: dispatched-without-outcome first; its transition is the
    // stronger claim and must not be masked by re-approval handling.
    let crashed: Vec<CrashedStep> = sqlx::query_as::<_, CrashedStep>(
        r#"
        SELECT s.step_id, s.task_id, t.owner_id, t.version AS task_version, s.execution_id
        FROM steps s
        JOIN tasks t ON t.task_id = s.task_id
        WHERE s.state = 'dispatched' AND s.outcome IS NULL AND t.state = 'running'
        "#,
    )
    .fetch_all(pool)
    .await?;
    for step in &crashed {
        report.awaiting_review += park_for_review(pool, step).await?;
    }

    // AC-19: authorized-but-undispatched steps require fresh approval.
    let authorized: Vec<CrashedStep> = sqlx::query_as::<_, CrashedStep>(
        r#"
        SELECT s.step_id, s.task_id, t.owner_id, t.version AS task_version, s.execution_id
        FROM steps s
        JOIN tasks t ON t.task_id = s.task_id
        WHERE s.state = 'authorized' AND t.state = 'running'
        "#,
    )
    .fetch_all(pool)
    .await?;
    for step in &authorized {
        report.reapproval_required += require_reapproval(pool, step).await?;
    }

    Ok(report)
}

/// Emit `recovery.resumed` for a running task, at most once, keeping the
/// checkpoint byte-identical.
async fn resume_checkpoint(pool: &PgPool, task: &OpenTask) -> Result<usize, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let inserted: Option<(i64,)> = sqlx::query_as(
        r#"
        INSERT INTO events (session_id, task_id, owner_id, type, payload)
        SELECT NULL, $1, $2, 'recovery.resumed', $3::jsonb
        WHERE NOT EXISTS (
            SELECT 1 FROM events WHERE task_id = $1 AND type = 'recovery.resumed'
        )
        RETURNING id
        "#,
    )
    .bind(&task.task_id)
    .bind(&task.owner_id)
    .bind(sqlx::types::Json(json!({
        "task_id": task.task_id,
        "checkpoint": task.checkpoint,
    })))
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(usize::from(inserted.is_some()))
}

/// AC-18/AC-23: mark the step `outcome_unknown`, park the task in
/// `waiting_review`, record the execution ID for the review trail.
async fn park_for_review(pool: &PgPool, step: &CrashedStep) -> Result<usize, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let stepped: Option<(String,)> = sqlx::query_as(
        r#"
        UPDATE steps SET state = 'outcome_unknown', updated_at = now()
        WHERE step_id = $1 AND state = 'dispatched'
        RETURNING step_id
        "#,
    )
    .bind(&step.step_id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some((step_id,)) = stepped else {
        tx.rollback().await.ok();
        return Ok(0);
    };

    // CAS on the task version read at sweep start.
    let parked: Option<(String, i64)> = sqlx::query_as(
        r#"
        UPDATE tasks SET state = $1, version = version + 1, updated_at = now()
        WHERE task_id = $2 AND version = $3 AND state = 'running'
        RETURNING task_id, version
        "#,
    )
    .bind(TaskState::WaitingReview.as_str())
    .bind(&step.task_id)
    .bind(step.task_version)
    .fetch_optional(&mut *tx)
    .await?;

    let Some((task_id, version)) = parked else {
        tx.rollback().await.ok();
        return Ok(0);
    };

    events::append_scoped_in_tx(
        &mut tx,
        &step.owner_id,
        "recovery.awaiting_review",
        json!({
            "task_id": task_id,
            "step_id": step_id,
            "execution_id": step.execution_id,
            "outcome": "unknown",
            "task_version": version,
        }),
        None,
        Some(&step.task_id),
    )
    .await?;
    tx.commit().await?;
    Ok(1)
}

/// AC-19: move the task to `waiting_approval`. The step keeps its `authorized`
/// state with no execution ID: recovery executes nothing and renews nothing.
async fn require_reapproval(pool: &PgPool, step: &CrashedStep) -> Result<usize, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let parked: Option<(String, i64)> = sqlx::query_as(
        r#"
        UPDATE tasks SET state = $1, version = version + 1, updated_at = now()
        WHERE task_id = $2 AND version = $3 AND state = 'running'
        RETURNING task_id, version
        "#,
    )
    .bind(TaskState::WaitingApproval.as_str())
    .bind(&step.task_id)
    .bind(step.task_version)
    .fetch_optional(&mut *tx)
    .await?;

    let Some((task_id, version)) = parked else {
        tx.rollback().await.ok();
        return Ok(0);
    };

    events::append_scoped_in_tx(
        &mut tx,
        &step.owner_id,
        "recovery.reapproval_required",
        json!({
            "task_id": task_id,
            "step_id": step.step_id,
            "execution_id": step.execution_id,
            "task_version": version,
            "reason": "authorized_before_restart; fresh approval required",
        }),
        None,
        Some(&step.task_id),
    )
    .await?;
    tx.commit().await?;
    Ok(1)
}

/// Parse a stored state string, mapping anything unknown to a typed error.
pub fn parse_state(state: &str) -> Result<TaskState, TaskError> {
    state
        .parse::<TaskState>()
        .map_err(|_| TaskError::InvalidStepState {
            step_id: String::new(),
            expected: "valid task state",
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands;
    use crate::tasks::{self, CrashPoint, TaskState};
    use serde_json::json;
    use sqlx::Row;

    async fn seeded_task(pool: &PgPool) -> tasks::Task {
        let ingested = commands::ingest(
            pool,
            "owner_a",
            &crate::ids::new_id("req"),
            "input.send",
            json!({}),
        )
        .await
        .unwrap();
        let command_id = match ingested {
            commands::IngestOutcome::Ingested { command_id } => command_id,
            commands::IngestOutcome::Replayed { command_id } => command_id,
        };
        tasks::create_task(pool, &command_id, "owner_a")
            .await
            .unwrap()
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn checkpointed_task_resumes_once(
        pool: PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let task = seeded_task(&pool).await;
        let task =
            tasks::transition(&pool, &task.task_id, task.version, TaskState::Running).await?;
        let checkpoint = json!({ "next_step": "respond", "turn": 2 });
        let task =
            tasks::save_checkpoint(&pool, &task.task_id, task.version, checkpoint.clone()).await?;

        let first = recover_once(&pool).await?;
        assert_eq!(first.resumed, 1, "checkpointed task resumed");

        let second = recover_once(&pool).await?;
        assert_eq!(second.resumed, 0, "resume is idempotent");

        let events: i64 =
            sqlx::query("SELECT count(*) FROM events WHERE type = 'recovery.resumed'")
                .fetch_one(&pool)
                .await?
                .get(0);
        assert_eq!(events, 1);

        let reloaded = tasks::load(&pool, &task.task_id).await?.unwrap();
        assert_eq!(reloaded.checkpoint, checkpoint, "checkpoint intact");
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn dispatched_without_outcome_parks_for_review(
        pool: PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let task = seeded_task(&pool).await;
        let step =
            tasks::create_step(&pool, &task.task_id, task.version, "workspace.read_text").await?;
        tasks::authorize_step(&pool, &step.step_id).await?;
        let task =
            tasks::transition(&pool, &task.task_id, task.version, TaskState::Running).await?;

        // Crash after the intent committed, before any execution ran.
        let crashed = tasks::record_dispatch_intent(
            &pool,
            &task.task_id,
            &step.step_id,
            task.version,
            Some(CrashPoint::AfterIntentCommit),
        )
        .await;
        assert_eq!(
            crashed.unwrap_err(),
            tasks::DispatchError::Crash(CrashPoint::AfterIntentCommit)
        );

        let report = recover_once(&pool).await?;
        assert_eq!(report.awaiting_review, 1, "AC-18: dispatched step parked");

        let task_after = tasks::load(&pool, &task.task_id).await?.unwrap();
        assert_eq!(task_after.state, "waiting_review");

        let step_after: (String, Option<String>, Option<String>) =
            sqlx::query_as("SELECT state, execution_id, outcome FROM steps WHERE step_id = $1")
                .bind(&step.step_id)
                .fetch_one(&pool)
                .await?;
        assert_eq!(
            step_after.0, "outcome_unknown",
            "AC-23: outcome is unknown, not invented"
        );
        assert!(step_after.1.is_some(), "execution ID preserved for review");
        assert!(step_after.2.is_none(), "no outcome fabricated");

        // A second sweep changes nothing; the review waits for fresh approval.
        let second = recover_once(&pool).await?;
        assert_eq!(second.awaiting_review, 0, "no duplicate recovery events");
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn authorized_but_undispatched_requires_fresh_approval(
        pool: PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let task = seeded_task(&pool).await;
        let step =
            tasks::create_step(&pool, &task.task_id, task.version, "workspace.read_text").await?;
        tasks::authorize_step(&pool, &step.step_id).await?;
        let _task =
            tasks::transition(&pool, &task.task_id, task.version, TaskState::Running).await?;

        let report = recover_once(&pool).await?;
        assert_eq!(report.reapproval_required, 1, "AC-19: reapproval required");

        let task_after = tasks::load(&pool, &task.task_id).await?.unwrap();
        assert_eq!(task_after.state, "waiting_approval");

        let step_after: (String, Option<String>) =
            sqlx::query_as("SELECT state, execution_id FROM steps WHERE step_id = $1")
                .bind(&step.step_id)
                .fetch_one(&pool)
                .await?;
        assert_eq!(
            step_after.0, "authorized",
            "step not dispatched by recovery"
        );
        assert!(
            step_after.1.is_none(),
            "AC-19: recovery auto-executed nothing"
        );
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn pre_commit_crash_leaves_nothing_behind(
        pool: PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let task = seeded_task(&pool).await;
        let step =
            tasks::create_step(&pool, &task.task_id, task.version, "workspace.read_text").await?;
        tasks::authorize_step(&pool, &step.step_id).await?;
        let task =
            tasks::transition(&pool, &task.task_id, task.version, TaskState::Running).await?;

        let crashed = tasks::record_dispatch_intent(
            &pool,
            &task.task_id,
            &step.step_id,
            task.version,
            Some(CrashPoint::BeforeIntentCommit),
        )
        .await;
        assert_eq!(
            crashed.unwrap_err(),
            tasks::DispatchError::Crash(CrashPoint::BeforeIntentCommit)
        );

        let report = recover_once(&pool).await?;
        // The task was still `running` with an authorized step: re-approval,
        // never a dispatch.
        assert_eq!(report.reapproval_required, 1);
        assert_eq!(report.awaiting_review, 0);

        let step_after: (String, Option<String>) =
            sqlx::query_as("SELECT state, execution_id FROM steps WHERE step_id = $1")
                .bind(&step.step_id)
                .fetch_one(&pool)
                .await?;
        assert_eq!(step_after.0, "authorized");
        assert!(
            step_after.1.is_none(),
            "no intent visible after pre-commit crash"
        );
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn completed_tasks_are_untouched(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
        let task = seeded_task(&pool).await;
        let step =
            tasks::create_step(&pool, &task.task_id, task.version, "workspace.read_text").await?;
        tasks::authorize_step(&pool, &step.step_id).await?;
        let task =
            tasks::transition(&pool, &task.task_id, task.version, TaskState::Running).await?;
        let dispatched =
            tasks::record_dispatch_intent(&pool, &task.task_id, &step.step_id, task.version, None)
                .await?;
        let (_step, closed) = tasks::complete_step(
            &pool,
            &task.task_id,
            &step.step_id,
            task.version + 1,
            "ok",
            TaskState::Succeeded,
        )
        .await?;
        assert_eq!(closed.state, "succeeded");

        let report = recover_once(&pool).await?;
        assert_eq!(
            report,
            RecoveryReport::default(),
            "terminal work is never reopened"
        );
        assert_eq!(dispatched.state, "dispatched");
        Ok(())
    }

    #[test]
    fn unknown_state_is_a_typed_error() {
        assert!(matches!(parse_state("queued"), Ok(TaskState::Queued)));
        assert!(parse_state("nonsense").is_err());
    }
}
