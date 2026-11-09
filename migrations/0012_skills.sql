CREATE TABLE IF NOT EXISTS ai_skills (
    id TEXT PRIMARY KEY,
    owner TEXT NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    sharing_revision BIGINT NOT NULL CHECK (sharing_revision > 0),
    audience TEXT NOT NULL CHECK (audience IN ('private','team','organization')),
    team TEXT,
    CHECK ((audience = 'team') = (team IS NOT NULL))
);
CREATE TABLE IF NOT EXISTS ai_skill_versions (
    skill_id TEXT NOT NULL REFERENCES ai_skills(id),
    revision BIGINT NOT NULL CHECK (revision > 0),
    content JSONB NOT NULL,
    PRIMARY KEY (skill_id, revision)
);
