-- A subject may register several OpenAI-compatible helpers, each under a short
-- name; the pair is the identity. Replaces the single-row llm_configs shape,
-- carrying any existing registration over as the helper named "default".
CREATE TABLE IF NOT EXISTS llm_helpers (
    subject TEXT NOT NULL,
    id TEXT NOT NULL,
    base_url TEXT NOT NULL,
    model TEXT NOT NULL,
    api_key TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (subject, id)
);

-- llm_configs is created by 0003 on every boot, so this copy is guarded and the
-- table is retired here; re-running the script is a no-op.
DO $$
BEGIN
    IF to_regclass('public.llm_configs') IS NOT NULL THEN
        INSERT INTO llm_helpers (subject, id, base_url, model, api_key)
        SELECT subject, 'default', base_url, model, api_key FROM llm_configs
        ON CONFLICT DO NOTHING;
    END IF;
END $$;

DROP TABLE IF EXISTS llm_configs;
