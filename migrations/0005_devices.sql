CREATE TABLE pairing_codes (
    code_hash TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL DEFAULT now()+interval '2 minutes',
    consumed BOOLEAN NOT NULL DEFAULT false
);
CREATE TABLE devices (
    device_id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL,
    label TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    revoked BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
