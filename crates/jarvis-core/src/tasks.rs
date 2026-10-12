//! Task state machine, steps and execution intent.
//!
//! Durable task states (architecture.md): `queued`, `running`,
//! `waiting_approval`, `waiting_provider`, `waiting_review`, `succeeded`,
//! `failed`, `cancelled`. Steps: `planned`, `authorized`, `dispatched`,
//! `completed`, `failed`, `outcome_unknown`.
//!
//! Two invariants matter for recovery correctness (AC-17–AC-19, AC-23):
//!
//! 1. Every task transition carries the expected `version`; the UPDATE applies
//!    as `version = version + 1` or not at all, so a stale worker can never
//!    overwrite newer state (compare-and-swap).
//! 2. State transition + event + execution intent are written in ONE
//!    transaction, so a crash can only leave two possible worlds: intent
//!    absent (step still `authorized` → fresh approval needed) or intent
//!    present with no outcome (`dispatched` → `waiting_review`). A third
//!    world — intent partially visible — is impossible.

use serde_json::{json, Value};
use sqlx::PgPool;

use crate::events;
use crate::ids;
use thiserror::Error;

/// Task states as documented in architecture.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Queued,
    Running,
    WaitingApproval,
    WaitingProvider,
    WaitingReview,
    Succeeded,
    Failed,
    Cancelled,
}

impl TaskState {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskState::Queued => "queued",
            TaskState::Running => "running",
            TaskState::WaitingApproval => "waiting_approval",
            TaskState::WaitingProvider => "waiting_provider",
            TaskState::WaitingReview => "waiting_review",
            TaskState::Succeeded => "succeeded",
            TaskState::Failed => "failed",
            TaskState::Cancelled => "cancelled",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            TaskState::Succeeded | TaskState::Failed | TaskState::Cancelled
        )
    }
}

impl std::str::FromStr for TaskState {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "queued" => Ok(TaskState::Queued),
            "running" => Ok(TaskState::Running),
            "waiting_approval" => Ok(TaskState::WaitingApproval),
            "waiting_provider" => Ok(TaskState::WaitingProvider),
            "waiting_review" => Ok(TaskState::WaitingReview),
            "succeeded" => Ok(TaskState::Succeeded),
            "failed" => Ok(TaskState::Failed),
            "cancelled" => Ok(TaskState::Cancelled),
            other => Err(format!("unknown task state: {other}")),
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Task {
    pub task_id: String,
    pub command_id: String,
    pub owner_id: String,
    pub state: String,
    pub version: i64,
    pub checkpoint: Value,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Step {
    pub step_id: String,
    pub task_id: String,
    pub task_version: i64,
    pub kind: String,
    pub state: String,
    pub execution_id: Option<String>,
    pub outcome: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum TaskError {
    /// Task is no longer at the version the caller expected.
    #[error("task {task_id} is not at version {expected}")]
    StaleVersion { task_id: String, expected: i64 },
    /// Step is not in the expected state (e.g. already dispatched).
    #[error("step {step_id} is not in state {expected}")]
    InvalidStepState {
        step_id: String,
        expected: &'static str,
    },
    #[error("database error: {0}")]
    Db(String),
}

impl From<sqlx::Error> for TaskError {
    fn from(e: sqlx::Error) -> Self {
        TaskError::Db(e.to_string())
    }
}

/// Where a simulated crash lands relative to the intent transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashPoint {
    /// Simulates dying BEFORE the intent transaction commits: nothing is
    /// visible after restart (exercises AC-19).
    BeforeIntentCommit,
    /// Simulates dying AFTER the intent committed but before any execution
    /// happened (exercises AC-18 and AC-23).
    AfterIntentCommit,
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum DispatchError {
    #[error("simulated crash: {0:?}")]
    Crash(CrashPoint),
    #[error(transparent)]
    Task(#[from] TaskError),
}

impl From<sqlx::Error> for DispatchError {
    fn from(e: sqlx::Error) -> Self {
        DispatchError::Task(TaskError::Db(e.to_string()))
    }
}

/// A task row plus the state it came from, returned by transitions.
#[derive(Debug, Clone, sqlx::FromRow)]
struct TaskTransition {
    pub task_id: String,
    pub command_id: String,
    pub owner_id: String,
    pub state: String,
    pub version: i64,
    pub checkpoint: Value,
    pub previous_state: String,
}

impl TaskTransition {
    fn into_task(self) -> Task {
        Task {
            task_id: self.task_id,
            command_id: self.command_id,
            owner_id: self.owner_id,
            state: self.state,
            version: self.version,
            checkpoint: self.checkpoint,
        }
    }
}

const TASK_COLUMNS: &str = "task_id, command_id, owner_id, state, version, checkpoint";

pub async fn create_task(
    pool: &PgPool,
    command_id: &str,
    owner_id: &str,
) -> Result<Task, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let task = sqlx::query_as::<_, Task>(&format!(
        r#"
            INSERT INTO tasks (task_id, command_id, owner_id, state, version, checkpoint)
            VALUES ($1, $2, $3, 'queued', 1, '{{}}'::jsonb)
            RETURNING {TASK_COLUMNS}
            "#
    ))
    .bind(ids::new_id("tsk"))
    .bind(command_id)
    .bind(owner_id)
    .fetch_one(&mut *tx)
    .await?;

    events::append_scoped_in_tx(
        &mut tx,
        owner_id,
        "task.state_changed",
        json!({ "task_id": task.task_id, "from": null, "to": "queued", "version": task.version }),
        None,
        Some(&task.task_id),
    )
    .await?;
    tx.commit().await?;
    Ok(task)
}

pub async fn load(pool: &PgPool, task_id: &str) -> Result<Option<Task>, sqlx::Error> {
    sqlx::query_as::<_, Task>(&format!(
        "SELECT {TASK_COLUMNS} FROM tasks WHERE task_id = $1"
    ))
    .bind(task_id)
    .fetch_optional(pool)
    .await
}

/// Compare-and-swap transition. Fails with [`TaskError::StaleVersion`] when the
/// task has moved on. Transition, event and checkpoint write commit atomically.
pub async fn transition(
    pool: &PgPool,
    task_id: &str,
    expected_version: i64,
    next: TaskState,
) -> Result<Task, TaskError> {
    let mut tx = pool.begin().await?;
    let updated: Option<TaskTransition> = sqlx::query_as::<_, TaskTransition>(
        r#"
        WITH previous AS (
            SELECT state FROM tasks WHERE task_id = $2 AND version = $3
        )
        UPDATE tasks t
        SET state = $1, version = t.version + 1, updated_at = now()
        FROM previous p
        WHERE t.task_id = $2 AND t.version = $3
        RETURNING t.task_id, t.command_id, t.owner_id, t.state, t.version, t.checkpoint,
                  p.state AS previous_state
        "#,
    )
    .bind(next.as_str())
    .bind(task_id)
    .bind(expected_version)
    .fetch_optional(&mut *tx)
    .await?;

    match updated {
        Some(t) => {
            events::append_scoped_in_tx(
                &mut tx,
                &t.owner_id,
                "task.state_changed",
                json!({ "task_id": t.task_id, "from": t.previous_state, "to": t.state, "version": t.version }),
                None,
                Some(&t.task_id),
            )
            .await?;
            tx.commit().await?;
            Ok(t.into_task())
        }
        None => {
            tx.rollback().await.ok();
            Err(TaskError::StaleVersion {
                task_id: task_id.to_string(),
                expected: expected_version,
            })
        }
    }
}

/// Persist a checkpoint naming the task's next step, with CAS.
pub async fn save_checkpoint(
    pool: &PgPool,
    task_id: &str,
    expected_version: i64,
    checkpoint: Value,
) -> Result<Task, TaskError> {
    let mut tx = pool.begin().await?;
    let updated: Option<TaskTransition> = sqlx::query_as::<_, TaskTransition>(
        r#"
        WITH previous AS (
            SELECT state FROM tasks WHERE task_id = $2 AND version = $3
        )
        UPDATE tasks t
        SET checkpoint = $1, version = t.version + 1, updated_at = now()
        FROM previous p
        WHERE t.task_id = $2 AND t.version = $3
        RETURNING t.task_id, t.command_id, t.owner_id, t.state, t.version, t.checkpoint,
                  p.state AS previous_state
        "#,
    )
    .bind(sqlx::types::Json(checkpoint))
    .bind(task_id)
    .bind(expected_version)
    .fetch_optional(&mut *tx)
    .await?;

    match updated {
        Some(t) => {
            events::append_scoped_in_tx(
                &mut tx,
                &t.owner_id,
                "task.checkpointed",
                json!({ "task_id": t.task_id, "from": t.previous_state, "version": t.version }),
                None,
                Some(&t.task_id),
            )
            .await?;
            tx.commit().await?;
            Ok(t.into_task())
        }
        None => {
            tx.rollback().await.ok();
            Err(TaskError::StaleVersion {
                task_id: task_id.to_string(),
                expected: expected_version,
            })
        }
    }
}

pub async fn create_step(
    pool: &PgPool,
    task_id: &str,
    task_version: i64,
    kind: &str,
) -> Result<Step, sqlx::Error> {
    sqlx::query_as::<_, Step>(
        r#"
        INSERT INTO steps (step_id, task_id, task_version, kind, state)
        VALUES ($1, $2, $3, $4, 'planned')
        RETURNING step_id, task_id, task_version, kind, state, execution_id, outcome
        "#,
    )
    .bind(ids::new_id("stp"))
    .bind(task_id)
    .bind(task_version)
    .bind(kind)
    .fetch_one(pool)
    .await
}

/// Mark a step `authorized`: a grant exists (PR-4 binds grants; PR-2 records
/// the state machine transition and its evidence).
pub async fn authorize_step(pool: &PgPool, step_id: &str) -> Result<Step, TaskError> {
    let mut tx = pool.begin().await?;
    let step: Step = sqlx::query_as::<_, Step>(
        r#"
        UPDATE steps SET state = 'authorized', updated_at = now()
        WHERE step_id = $1 AND state = 'planned'
        RETURNING step_id, task_id, task_version, kind, state, execution_id, outcome
        "#,
    )
    .bind(step_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| TaskError::InvalidStepState {
        step_id: step_id.to_string(),
        expected: "planned",
    })?;

    events::append_scoped_in_tx(
        &mut tx,
        "owner",
        "step.authorized",
        json!({ "task_id": step.task_id, "step_id": step_id }),
        None,
        Some(&step.task_id),
    )
    .await?;
    tx.commit().await?;
    Ok(step)
}

/// Record execution intent: one transaction writes the step's transition to
/// `dispatched` with its unique execution ID, the task's transition to
/// `running`, and the audit event. `crash` simulates dying before or after
/// that commit; the function performs no execution itself.
pub async fn record_dispatch_intent(
    pool: &PgPool,
    task_id: &str,
    step_id: &str,
    expected_task_version: i64,
    crash: Option<CrashPoint>,
) -> Result<Step, DispatchError> {
    if crash == Some(CrashPoint::BeforeIntentCommit) {
        // Nothing persisted — exactly the world a pre-commit crash leaves.
        return Err(DispatchError::Crash(CrashPoint::BeforeIntentCommit));
    }

    let mut tx = pool.begin().await?;

    // CAS the task first: a stale worker must fail here, not below.
    let task: Task = match sqlx::query_as::<_, Task>(&format!(
        r#"
        UPDATE tasks SET state = 'running', version = version + 1, updated_at = now()
        WHERE task_id = $1 AND version = $2 AND state <> 'cancelled'
        RETURNING {TASK_COLUMNS}
        "#
    ))
    .bind(task_id)
    .bind(expected_task_version)
    .fetch_optional(&mut *tx)
    .await?
    {
        Some(task) => task,
        None => {
            tx.rollback().await.ok();
            return Err(DispatchError::Task(TaskError::StaleVersion {
                task_id: task_id.to_string(),
                expected: expected_task_version,
            }));
        }
    };

    let execution_id = ids::new_id("exe");
    let step: Step = match sqlx::query_as::<_, Step>(
        r#"
        UPDATE steps SET state = 'dispatched', execution_id = $2, updated_at = now()
        WHERE step_id = $1 AND state = 'authorized'
        RETURNING step_id, task_id, task_version, kind, state, execution_id, outcome
        "#,
    )
    .bind(step_id)
    .bind(&execution_id)
    .fetch_optional(&mut *tx)
    .await?
    {
        Some(step) => step,
        None => {
            tx.rollback().await.ok();
            return Err(DispatchError::Task(TaskError::InvalidStepState {
                step_id: step_id.to_string(),
                expected: "authorized",
            }));
        }
    };

    events::append_scoped_in_tx(
        &mut tx,
        &task.owner_id,
        "step.dispatched",
        json!({ "task_id": task_id, "step_id": step_id, "execution_id": execution_id, "intent": true }),
        None,
        Some(task_id),
    )
    .await?;

    tx.commit().await?;

    if crash == Some(CrashPoint::AfterIntentCommit) {
        // Intent committed; execution never ran — AC-18/AC-23 territory.
        return Err(DispatchError::Crash(CrashPoint::AfterIntentCommit));
    }
    Ok(step)
}

/// Record the observed outcome of a dispatched step and close its task.
pub async fn complete_step(
    pool: &PgPool,
    task_id: &str,
    step_id: &str,
    expected_task_version: i64,
    outcome: &str,
    next_task_state: TaskState,
) -> Result<(Step, Task), DispatchError> {
    let mut tx = pool.begin().await?;
    let task: Task = match sqlx::query_as::<_, Task>(&format!(
        r#"
        UPDATE tasks SET state = $1, version = version + 1, updated_at = now()
        WHERE task_id = $2 AND version = $3
        RETURNING {TASK_COLUMNS}
        "#
    ))
    .bind(next_task_state.as_str())
    .bind(task_id)
    .bind(expected_task_version)
    .fetch_optional(&mut *tx)
    .await?
    {
        Some(task) => task,
        None => {
            tx.rollback().await.ok();
            return Err(DispatchError::Task(TaskError::StaleVersion {
                task_id: task_id.to_string(),
                expected: expected_task_version,
            }));
        }
    };

    let step: Step = sqlx::query_as::<_, Step>(
        r#"
        UPDATE steps SET state = 'completed', outcome = $2, updated_at = now()
        WHERE step_id = $1
        RETURNING step_id, task_id, task_version, kind, state, execution_id, outcome
        "#,
    )
    .bind(step_id)
    .bind(outcome)
    .fetch_one(&mut *tx)
    .await?;

    events::append_scoped_in_tx(
        &mut tx,
        &task.owner_id,
        "step.completed",
        json!({ "task_id": task_id, "step_id": step_id, "outcome": outcome }),
        None,
        Some(task_id),
    )
    .await?;

    tx.commit().await?;
    Ok((step, task))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands;
    use serde_json::json;
    use sqlx::Row;

    async fn seed(pool: &PgPool) -> Task {
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
            commands::IngestOutcome::Ingested { command_id }
            | commands::IngestOutcome::Replayed { command_id } => command_id,
        };
        create_task(pool, &command_id, "owner_a").await.unwrap()
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn stale_version_cannot_transition(
        pool: PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let task = seed(&pool).await;
        let moved = transition(&pool, &task.task_id, task.version, TaskState::Running).await?;
        assert_eq!(moved.state, "running");
        assert_eq!(moved.version, task.version + 1);

        let stale = transition(
            &pool,
            &task.task_id,
            task.version,
            TaskState::WaitingApproval,
        )
        .await;
        assert_eq!(
            stale.unwrap_err(),
            TaskError::StaleVersion {
                task_id: task.task_id.clone(),
                expected: task.version,
            }
        );

        let reloaded = load(&pool, &task.task_id).await?.unwrap();
        assert_eq!(reloaded.state, "running", "stale write left no trace");
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn double_dispatch_intent_is_rejected(
        pool: PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let task = seed(&pool).await;
        let step = create_step(&pool, &task.task_id, task.version, "workspace.read_text").await?;
        authorize_step(&pool, &step.step_id).await?;

        let task = transition(&pool, &task.task_id, task.version, TaskState::Running).await?;
        let dispatched =
            record_dispatch_intent(&pool, &task.task_id, &step.step_id, task.version, None).await?;
        assert_eq!(dispatched.state, "dispatched");
        assert!(dispatched.execution_id.is_some());

        let again =
            record_dispatch_intent(&pool, &task.task_id, &step.step_id, task.version, None).await;
        assert_eq!(
            again.unwrap_err(),
            // The version check fires first: a second dispatch carrying the
            // pre-dispatch version is a stale write, never a re-execution
            // (AC-06 single-consumption).
            DispatchError::Task(TaskError::StaleVersion {
                task_id: task.task_id.clone(),
                expected: task.version,
            })
        );

        let executions: i64 =
            sqlx::query("SELECT count(*) FROM events WHERE type = 'step.dispatched'")
                .fetch_one(&pool)
                .await?
                .get(0);
        assert_eq!(executions, 1, "one execution intent, never two");
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn checkpoint_round_trips_and_cas_guards(
        pool: PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let task = seed(&pool).await;
        let checkpoint = json!({ "next_step": "respond", "turn": 1 });
        let saved = save_checkpoint(&pool, &task.task_id, task.version, checkpoint.clone()).await?;
        assert_eq!(saved.checkpoint, checkpoint);

        let stale = save_checkpoint(&pool, &task.task_id, task.version, json!({})).await;
        assert!(matches!(stale.unwrap_err(), TaskError::StaleVersion { .. }));

        let reloaded = load(&pool, &task.task_id).await?.unwrap();
        assert_eq!(reloaded.checkpoint, checkpoint, "checkpoint survived");
        Ok(())
    }
}
