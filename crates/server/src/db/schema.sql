-- Schema version 1. Timestamps are RFC 3339 UTC strings, days are
-- YYYY-MM-DD budget days, durations are milliseconds.

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;

CREATE TABLE profiles (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL
) STRICT;

CREATE TABLE program_groups (
    id            INTEGER PRIMARY KEY,
    profile_id    INTEGER NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    -- core::schedule::WeekSchedule as JSON.
    schedule      TEXT NOT NULL,
    blocked_since TEXT
) STRICT;

CREATE TABLE rules (
    id       INTEGER PRIMARY KEY,
    group_id INTEGER NOT NULL REFERENCES program_groups (id) ON DELETE CASCADE,
    -- core::rules::Rule as JSON.
    rule     TEXT NOT NULL
) STRICT;

CREATE TABLE bonuses (
    id       INTEGER PRIMARY KEY,
    group_id INTEGER NOT NULL REFERENCES program_groups (id) ON DELETE CASCADE,
    day      TEXT NOT NULL,
    minutes  INTEGER NOT NULL
) STRICT;

CREATE TABLE devices (
    id                 INTEGER PRIMARY KEY,
    name               TEXT NOT NULL,
    platform           TEXT,
    token_hash         TEXT UNIQUE,
    enroll_code_hash   TEXT UNIQUE,
    enroll_expires     TEXT,
    last_seen          TEXT,
    -- Local accounts reported by the agent, as a JSON array of names.
    local_users        TEXT NOT NULL DEFAULT '[]'
) STRICT;

CREATE TABLE bindings (
    device_id  INTEGER NOT NULL REFERENCES devices (id) ON DELETE CASCADE,
    os_user    TEXT NOT NULL,
    profile_id INTEGER NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    PRIMARY KEY (device_id, os_user)
) STRICT;

-- Usage history outlives deleted groups and devices, so no foreign keys.
CREATE TABLE usage (
    id          TEXT PRIMARY KEY,
    device_id   INTEGER NOT NULL,
    group_id    INTEGER NOT NULL,
    day         TEXT NOT NULL,
    start       TEXT NOT NULL,
    duration_ms INTEGER NOT NULL,
    -- 1 if any version of the record arrived well after it was recorded,
    -- that is, the device was offline while it was used.
    offline     INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE INDEX usage_by_day ON usage (day, group_id);

CREATE TABLE parent_actions (
    id      INTEGER PRIMARY KEY,
    at      TEXT NOT NULL,
    summary TEXT NOT NULL
) STRICT;

CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY,
    expires    TEXT NOT NULL
) STRICT;

INSERT INTO profiles (id, name) VALUES (1, 'Ребёнок');
