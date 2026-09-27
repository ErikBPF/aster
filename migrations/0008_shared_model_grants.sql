-- Grants belong to one registration incarnation. A delete/recreate cannot
-- inherit them; the independent event table retains the former audit trail.
CREATE SEQUENCE IF NOT EXISTS shared_model_registration_generation_seq;

ALTER TABLE shared_models
    ADD COLUMN IF NOT EXISTS registration_generation BIGINT;
UPDATE shared_models
    SET registration_generation = nextval('shared_model_registration_generation_seq')
    WHERE registration_generation IS NULL;
ALTER TABLE shared_models
    ALTER COLUMN registration_generation SET DEFAULT nextval('shared_model_registration_generation_seq'),
    ALTER COLUMN registration_generation SET NOT NULL;

ALTER TABLE shared_models
    ADD COLUMN IF NOT EXISTS grant_revision BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS grants JSONB NOT NULL DEFAULT '[]'::jsonb;

CREATE TABLE IF NOT EXISTS shared_model_grant_events (
    id BIGSERIAL PRIMARY KEY,
    model_id TEXT NOT NULL,
    registration_generation BIGINT NOT NULL,
    revision BIGINT NOT NULL,
    actor TEXT NOT NULL,
    before_grants JSONB NOT NULL,
    after_grants JSONB NOT NULL,
    changed_at_ms BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS shared_model_grant_events_model_id_id
    ON shared_model_grant_events (model_id, id);
