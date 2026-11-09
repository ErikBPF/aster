CREATE TABLE IF NOT EXISTS ai_mcp_connections (
    id TEXT PRIMARY KEY,
    revision BIGINT NOT NULL,
    content JSONB NOT NULL
);
CREATE TABLE IF NOT EXISTS ai_mcp_connection_events (
    id BIGSERIAL PRIMARY KEY,
    connection_id TEXT NOT NULL,
    revision BIGINT NOT NULL,
    actor TEXT NOT NULL,
    metadata JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS ai_mcp_confirmations (
    id TEXT PRIMARY KEY, subject TEXT NOT NULL, fingerprint TEXT NOT NULL, expires BIGINT NOT NULL
);
