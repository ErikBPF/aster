CREATE TABLE IF NOT EXISTS personal_model_policy (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    revision BIGINT NOT NULL DEFAULT 0,
    subjects JSONB NOT NULL DEFAULT '[]',
    teams JSONB NOT NULL DEFAULT '[]'
);
INSERT INTO personal_model_policy (singleton) VALUES (TRUE) ON CONFLICT DO NOTHING;
CREATE TABLE IF NOT EXISTS personal_model_policy_events (
    revision BIGINT PRIMARY KEY,
    actor TEXT NOT NULL,
    subjects JSONB NOT NULL,
    teams JSONB NOT NULL,
    changed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE llm_helpers ADD COLUMN IF NOT EXISTS key_version TEXT;
ALTER TABLE llm_helpers ADD COLUMN IF NOT EXISTS nonce BYTEA;
ALTER TABLE llm_helpers ADD COLUMN IF NOT EXISTS ciphertext BYTEA;
