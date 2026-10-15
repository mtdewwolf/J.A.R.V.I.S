-- Private bodies are AEAD ciphertext. Audit events contain only references.
CREATE TABLE sessions (
    session_id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE turns (
    task_id TEXT PRIMARY KEY REFERENCES tasks(task_id),
    session_id TEXT NOT NULL REFERENCES sessions(session_id),
    owner_id TEXT NOT NULL,
    request_id TEXT NOT NULL,
    generation_id TEXT NOT NULL,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    model_turns INTEGER NOT NULL DEFAULT 0,
    body JSONB NOT NULL,
    UNIQUE(owner_id, request_id)
);
CREATE TABLE proposals (
    proposal_id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES turns(task_id),
    generation_id TEXT NOT NULL,
    step_id TEXT NOT NULL REFERENCES steps(step_id),
    owner_id TEXT NOT NULL,
    tool TEXT NOT NULL,
    digest TEXT NOT NULL,
    body JSONB NOT NULL,
    fence BIGINT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL DEFAULT now() + interval '60 seconds',
    consumed BOOLEAN NOT NULL DEFAULT false
);
CREATE TABLE memories (
    memory_id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL,
    key_hash TEXT NOT NULL,
    digest TEXT NOT NULL,
    body JSONB NOT NULL,
    confirmed BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL DEFAULT now() + interval '24 hours'
);
CREATE UNIQUE INDEX memories_confirmed_key ON memories(owner_id,key_hash) WHERE confirmed;
CREATE TABLE erasures (
    id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL,
    erased_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
