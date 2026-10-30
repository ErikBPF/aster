-- Server-owned AI conversation sessions. One row per subject, notebook scope
-- (the notebook key already encodes an optional cell) and session id. Existing
-- single-conversation rows become the first session for their scope.
CREATE TABLE IF NOT EXISTS conversation_sessions (
    id TEXT PRIMARY KEY,
    subject TEXT NOT NULL,
    notebook TEXT NOT NULL,
    phrase TEXT NOT NULL DEFAULT 'New session',
    revision BIGINT NOT NULL DEFAULT 0 CHECK (revision >= 0),
    messages JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS conversation_sessions_scope_idx
    ON conversation_sessions (subject, notebook, updated_at DESC);

INSERT INTO conversation_sessions
    (id, subject, notebook, phrase, revision, messages, created_at, updated_at)
SELECT
    id,
    subject,
    notebook,
    CASE
        WHEN phrase IS NULL OR phrase = '' THEN 'New session'
        WHEN char_length(phrase) > 60 THEN left(phrase, 60) || '…'
        ELSE phrase
    END,
    revision,
    messages,
    updated_at,
    updated_at
FROM (
    SELECT
        id,
        subject,
        notebook,
        btrim(regexp_replace(messages -> 0 ->> 'content', '\s+', ' ', 'g')) AS phrase,
        revision,
        messages,
        updated_at
    FROM notebook_conversations
) AS legacy
ON CONFLICT (id) DO NOTHING;
