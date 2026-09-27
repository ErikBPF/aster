CREATE TABLE IF NOT EXISTS notebook_conversations (
    subject TEXT NOT NULL,
    notebook TEXT NOT NULL,
    id TEXT NOT NULL UNIQUE,
    revision BIGINT NOT NULL DEFAULT 0 CHECK (revision >= 0),
    messages JSONB NOT NULL DEFAULT '[]'::jsonb,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (subject, notebook)
);
