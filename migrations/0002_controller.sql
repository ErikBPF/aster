-- Controller-owned projections of the configured pool. The server keeps reading
-- its configuration from the environment; the controller mirrors it here so
-- cluster state and availability are auditable from the metadata database.
CREATE TABLE IF NOT EXISTS engines (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    endpoint TEXT NOT NULL,
    routing_group TEXT,
    health TEXT NOT NULL,
    checked_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS catalogs (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    endpoint TEXT NOT NULL,
    catalog TEXT,
    health TEXT NOT NULL,
    checked_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
