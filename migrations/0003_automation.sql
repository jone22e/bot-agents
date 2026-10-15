-- DOTS Fase 3: eventos, gatilhos, agendamentos, checkpoints, delegacao e perguntas.

ALTER TABLE dots ADD COLUMN role TEXT NOT NULL DEFAULT 'worker' CHECK (role IN ('worker','supervisor'));

ALTER TABLE tool_policies ADD COLUMN can_delegate_to_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE tool_policies ADD COLUMN max_delegation_depth INTEGER NOT NULL DEFAULT 2;

ALTER TABLE tasks ADD COLUMN max_attempts INTEGER NOT NULL DEFAULT 1;
ALTER TABLE tasks ADD COLUMN origin TEXT NOT NULL DEFAULT 'user';
ALTER TABLE tasks ADD COLUMN parent_task_id TEXT;
ALTER TABLE tasks ADD COLUMN delegation_chain_json TEXT NOT NULL DEFAULT '[]';
CREATE INDEX idx_tasks_parent ON tasks(parent_task_id);

ALTER TABLE approvals ADD COLUMN kind TEXT NOT NULL DEFAULT 'permission' CHECK (kind IN ('permission','question'));
ALTER TABLE approvals ADD COLUMN options_json TEXT NOT NULL DEFAULT '[]';

CREATE TABLE events (
    id              TEXT PRIMARY KEY,
    type            TEXT NOT NULL,
    source          TEXT NOT NULL,
    occurred_at     TEXT NOT NULL,
    payload_json    TEXT NOT NULL DEFAULT '{}',
    correlation_id  TEXT,
    idempotency_key TEXT UNIQUE,
    depth           INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_events_type_time ON events(type, occurred_at DESC);

CREATE TABLE event_subscriptions (
    id                TEXT PRIMARY KEY,
    dot_id            TEXT NOT NULL REFERENCES dots(id) ON DELETE CASCADE,
    event_type        TEXT NOT NULL,
    filter_json       TEXT NOT NULL DEFAULT '{}',
    prompt_template   TEXT NOT NULL,
    retry_safe        INTEGER NOT NULL DEFAULT 0 CHECK (retry_safe IN (0,1)),
    cooldown_seconds  INTEGER NOT NULL DEFAULT 60,
    enabled           INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0,1)),
    last_triggered_at TEXT,
    created_at        TEXT NOT NULL
);
CREATE INDEX idx_subscriptions_type ON event_subscriptions(event_type, enabled);

CREATE TABLE event_deliveries (
    id              TEXT PRIMARY KEY,
    event_id        TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    subscription_id TEXT NOT NULL REFERENCES event_subscriptions(id) ON DELETE CASCADE,
    task_id         TEXT,
    status          TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    UNIQUE (event_id, subscription_id)
);

CREATE TABLE schedules (
    id          TEXT PRIMARY KEY,
    dot_id      TEXT NOT NULL REFERENCES dots(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL CHECK (kind IN ('every','daily','weekly','cron')),
    spec        TEXT NOT NULL,
    prompt      TEXT NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0,1)),
    next_run_at TEXT,
    last_run_at TEXT,
    created_at  TEXT NOT NULL
);
CREATE INDEX idx_schedules_next ON schedules(enabled, next_run_at);

CREATE TABLE checkpoints (
    id                TEXT PRIMARY KEY,
    task_id           TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    execution_id      TEXT NOT NULL,
    step              TEXT NOT NULL,
    state_json        TEXT NOT NULL DEFAULT '{}',
    context           TEXT NOT NULL DEFAULT '',
    results           TEXT NOT NULL DEFAULT '',
    dependencies_json TEXT NOT NULL DEFAULT '[]',
    next_action       TEXT NOT NULL DEFAULT '',
    artifacts_json    TEXT NOT NULL DEFAULT '[]',
    source            TEXT NOT NULL DEFAULT 'auto',
    created_at        TEXT NOT NULL
);
CREATE INDEX idx_checkpoints_task ON checkpoints(task_id, created_at DESC);

CREATE TABLE tool_invocations (
    id           TEXT PRIMARY KEY,
    task_id      TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    execution_id TEXT NOT NULL,
    tool         TEXT NOT NULL,
    input_hash   TEXT NOT NULL,
    outcome      TEXT NOT NULL,
    output       TEXT NOT NULL DEFAULT '',
    created_at   TEXT NOT NULL
);
CREATE INDEX idx_tool_invocations_lookup ON tool_invocations(task_id, tool, input_hash);

CREATE TABLE delegations (
    id              TEXT PRIMARY KEY,
    from_dot_id     TEXT NOT NULL,
    to_dot_id       TEXT NOT NULL,
    parent_task_id  TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    child_task_id   TEXT,
    correlation_id  TEXT NOT NULL,
    request         TEXT NOT NULL,
    result          TEXT,
    status          TEXT NOT NULL CHECK (status IN ('pending','running','completed','failed','cancelled')),
    created_at      TEXT NOT NULL,
    resolved_at     TEXT
);
CREATE INDEX idx_delegations_parent ON delegations(parent_task_id);
CREATE INDEX idx_delegations_child ON delegations(child_task_id);
