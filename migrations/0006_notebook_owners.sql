CREATE TABLE IF NOT EXISTS notebook_owners (
    source TEXT NOT NULL,
    notebook_id TEXT NOT NULL,
    owner_subject TEXT NOT NULL,
    source_blob TEXT NOT NULL,
    PRIMARY KEY (source, notebook_id)
);

CREATE TABLE IF NOT EXISTS notebook_owner_events (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    source TEXT NOT NULL,
    notebook_id TEXT NOT NULL,
    previous_owner TEXT,
    owner_subject TEXT NOT NULL,
    source_blob TEXT NOT NULL,
    actor_subject TEXT NOT NULL,
    reason TEXT,
    changed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
