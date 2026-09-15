-- aster metadata schema. Idempotent so the server can apply it on startup.
-- ponytail: one script; move to sqlx migrations when the schema evolves.

CREATE TABLE IF NOT EXISTS grants (
    subject TEXT NOT NULL,
    engine  TEXT NOT NULL,
    PRIMARY KEY (subject, engine)
);

CREATE TABLE IF NOT EXISTS audit (
    id         BIGSERIAL PRIMARY KEY,
    subject    TEXT        NOT NULL,
    engine     TEXT        NOT NULL,
    catalog    TEXT,
    schema     TEXT,
    sql        TEXT        NOT NULL,
    latency_ms BIGINT      NOT NULL,
    row_count  BIGINT      NOT NULL,
    ok         BOOLEAN     NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS audit_subject_created_idx ON audit (subject, created_at DESC);
