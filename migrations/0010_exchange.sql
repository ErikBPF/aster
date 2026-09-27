CREATE TABLE IF NOT EXISTS notebook_exchange_summary (
    subject TEXT NOT NULL,
    notebook TEXT NOT NULL,
    summary TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (subject, notebook)
);

CREATE TABLE IF NOT EXISTS notebook_exchange_result (
    subject TEXT NOT NULL,
    notebook TEXT NOT NULL,
    cell TEXT NOT NULL,
    result JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (subject, notebook, cell)
);
