CREATE TABLE IF NOT EXISTS durable_workspaces (
    id TEXT PRIMARY KEY,
    subject TEXT NOT NULL,
    name TEXT NOT NULL,
    writer_hash TEXT NOT NULL,
    epoch BIGINT NOT NULL CHECK (epoch > 0),
    pending_operation JSONB,
    deleted BOOLEAN NOT NULL DEFAULT FALSE,
    cleanup_at TIMESTAMPTZ,
    layout JSONB NOT NULL DEFAULT '{}',
    layout_version BIGINT NOT NULL DEFAULT 0 CHECK (layout_version >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS durable_workspaces_owner ON durable_workspaces(subject);

CREATE TABLE IF NOT EXISTS durable_workspace_checkouts (
    id TEXT PRIMARY KEY,
    workspace TEXT NOT NULL REFERENCES durable_workspaces(id),
    team TEXT NOT NULL,
    repository_id BIGINT NOT NULL,
    binding JSONB NOT NULL,
    branch TEXT NOT NULL,
    version BIGINT NOT NULL DEFAULT 0 CHECK (version >= 0),
    snapshot_key TEXT,
    snapshot_hash TEXT,
    snapshot_bytes BIGINT NOT NULL DEFAULT 0 CHECK (snapshot_bytes >= 0),
    UNIQUE (workspace, team, repository_id)
);
