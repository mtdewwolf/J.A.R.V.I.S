-- PR-2: durable slice of the event store.
--
-- Content-free event metadata only (docs/phase-0/architecture.md, "Durable task
-- state"): private payloads, encrypted messages and memory records arrive in
-- PR-4/PR-5. Tasks, steps and events land in one migration-reviewed schema;
-- state transitions, events and execution intents are written by a single
-- transaction per transition (see src/tasks.rs).

CREATE TABLE IF NOT EXISTS commands (
    command_id   TEXT PRIMARY KEY,
    owner_id     TEXT NOT NULL,
    request_id   TEXT NOT NULL,
    command_type TEXT NOT NULL,
    payload      JSONB NOT NULL,
    state        TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Input deduplication: one accepted command per owner/request pair.
    CONSTRAINT commands_owner_request_unique UNIQUE (owner_id, request_id)
);
CREATE INDEX IF NOT EXISTS commands_owner_created_idx ON commands (owner_id, created_at);

CREATE TABLE IF NOT EXISTS tasks (
    task_id    TEXT PRIMARY KEY,
    command_id TEXT NOT NULL REFERENCES commands (command_id),
    owner_id   TEXT NOT NULL,
    -- Compare-and-swap column: every transition requires the expected version.
    state      TEXT NOT NULL,
    version    BIGINT NOT NULL DEFAULT 1,
    checkpoint JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT tasks_state_valid CHECK (state IN (
        'queued', 'running', 'waiting_approval', 'waiting_provider',
        'waiting_review', 'succeeded', 'failed', 'cancelled'))
);
CREATE INDEX IF NOT EXISTS tasks_owner_state_idx ON tasks (owner_id, state);

CREATE TABLE IF NOT EXISTS steps (
    step_id      TEXT PRIMARY KEY,
    task_id      TEXT NOT NULL REFERENCES tasks (task_id),
    task_version BIGINT NOT NULL,
    kind         TEXT NOT NULL,
    state        TEXT NOT NULL,
    -- Dispatch deduplication key: written as execution intent before dispatch.
    execution_id TEXT,
    outcome      TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT steps_state_valid CHECK (state IN (
        'planned', 'authorized', 'dispatched', 'completed', 'failed', 'outcome_unknown')),
    CONSTRAINT steps_execution_unique UNIQUE (execution_id)
);
CREATE INDEX IF NOT EXISTS steps_task_version_idx ON steps (task_id, task_version);

CREATE TABLE IF NOT EXISTS events (
    id         BIGSERIAL PRIMARY KEY,
    session_id TEXT,
    task_id    TEXT,
    owner_id   TEXT NOT NULL,
    type       TEXT NOT NULL,
    payload    JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS events_task_idx ON events (task_id, id);
CREATE INDEX IF NOT EXISTS events_session_cursor_idx ON events (session_id, id);

-- Single-worker lease with monotonic fencing: a restarted or stale worker
-- cannot commit after its lease has been taken over.
CREATE TABLE IF NOT EXISTS leases (
    lease_name TEXT PRIMARY KEY,
    holder     TEXT NOT NULL,
    fence      BIGINT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);
