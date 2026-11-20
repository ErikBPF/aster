-- Keep the legacy single team target intact while workspace repositories opt in.
CREATE TABLE IF NOT EXISTS workspace_repositories (
    team TEXT NOT NULL CHECK (team <> ''),
    repository_id BIGINT NOT NULL CHECK (repository_id > 0),
    repository TEXT NOT NULL CHECK (repository <> ''),
    installation_id BIGINT NOT NULL CHECK (installation_id > 0),
    default_branch TEXT NOT NULL CHECK (default_branch <> ''),
    default_commit TEXT NOT NULL CHECK (default_commit ~ '^([0-9a-fA-F]{40}|[0-9a-fA-F]{64})$'),
    version BIGINT NOT NULL CHECK (version > 0),
    configured_by TEXT NOT NULL CHECK (configured_by <> ''),
    configured_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (team, repository_id),
    UNIQUE (team, repository)
);

CREATE TABLE IF NOT EXISTS workspace_repository_events (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    team TEXT NOT NULL,
    repository_id BIGINT NOT NULL,
    repository TEXT NOT NULL,
    installation_id BIGINT NOT NULL,
    default_branch TEXT NOT NULL,
    default_commit TEXT NOT NULL,
    previous_version BIGINT,
    version BIGINT NOT NULL,
    actor_subject TEXT NOT NULL,
    changed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
