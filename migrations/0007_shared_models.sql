-- Shared registrations are separate from subject-owned llm_helpers. The
-- application encrypts each token before it reaches PostgreSQL.
CREATE TABLE IF NOT EXISTS shared_models (
    id TEXT PRIMARY KEY,
    base_url TEXT NOT NULL,
    model TEXT NOT NULL,
    key_version TEXT NOT NULL,
    nonce BYTEA NOT NULL,
    ciphertext BYTEA NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
