CREATE TABLE IF NOT EXISTS team_git_targets (
    team TEXT PRIMARY KEY,
    repository TEXT NOT NULL,
    repository_id BIGINT NOT NULL,
    installation_id BIGINT NOT NULL,
    default_branch TEXT NOT NULL,
    default_commit TEXT NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    configured_by TEXT NOT NULL,
    configured_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS team_git_target_events (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    team TEXT NOT NULL,
    previous_version BIGINT,
    version BIGINT NOT NULL,
    repository TEXT NOT NULL,
    repository_id BIGINT NOT NULL,
    installation_id BIGINT NOT NULL,
    default_branch TEXT NOT NULL,
    default_commit TEXT NOT NULL,
    actor_subject TEXT NOT NULL,
    changed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
