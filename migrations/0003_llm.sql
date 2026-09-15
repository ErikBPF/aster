-- Per-caller LLM completion endpoints. The token is stored as given and is
-- never returned by the API.
CREATE TABLE IF NOT EXISTS llm_configs (
    subject TEXT PRIMARY KEY,
    base_url TEXT NOT NULL,
    model TEXT NOT NULL,
    api_key TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
