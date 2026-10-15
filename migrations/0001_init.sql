-- DOTS: esquema inicial (Fase 1). Nao alterar depois de executada; criar nova migracao.

CREATE TABLE installations (
    id          TEXT PRIMARY KEY,
    created_at  TEXT NOT NULL,
    app_version TEXT NOT NULL,
    data_dir    TEXT NOT NULL,
    platform    TEXT NOT NULL
);

CREATE TABLE dots (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    objective   TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL CHECK (status IN ('active','paused','archived')),
    limits_json TEXT NOT NULL DEFAULT '{}',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE INDEX idx_dots_status ON dots(status);

-- Instrucoes permanentes, versionadas. A versao atual e a de maior numero.
CREATE TABLE dot_instructions (
    id         TEXT PRIMARY KEY,
    dot_id     TEXT NOT NULL REFERENCES dots(id) ON DELETE CASCADE,
    version    INTEGER NOT NULL,
    content    TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE (dot_id, version)
);

-- Vinculo DOT -> motor. Um unico primario por DOT.
CREATE TABLE dot_engines (
    id          TEXT PRIMARY KEY,
    dot_id      TEXT NOT NULL REFERENCES dots(id) ON DELETE CASCADE,
    engine      TEXT NOT NULL CHECK (engine IN ('codex','claude','agy')),
    is_primary  INTEGER NOT NULL DEFAULT 0 CHECK (is_primary IN (0,1)),
    config_json TEXT NOT NULL DEFAULT '{}',
    created_at  TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_dot_engines_primary ON dot_engines(dot_id) WHERE is_primary = 1;

CREATE TABLE dot_workspaces (
    id         TEXT PRIMARY KEY,
    dot_id     TEXT NOT NULL REFERENCES dots(id) ON DELETE CASCADE,
    path       TEXT NOT NULL,
    is_primary INTEGER NOT NULL DEFAULT 0 CHECK (is_primary IN (0,1)),
    can_read   INTEGER NOT NULL DEFAULT 1 CHECK (can_read IN (0,1)),
    can_write  INTEGER NOT NULL DEFAULT 1 CHECK (can_write IN (0,1)),
    created_at TEXT NOT NULL,
    UNIQUE (dot_id, path)
);
CREATE UNIQUE INDEX idx_dot_workspaces_primary ON dot_workspaces(dot_id) WHERE is_primary = 1;

-- Tarefas e execucoes: estrutura preparada para a Fase 2 (sem uso na Fase 1).
CREATE TABLE tasks (
    id              TEXT PRIMARY KEY,
    dot_id          TEXT NOT NULL REFERENCES dots(id) ON DELETE CASCADE,
    conversation_id TEXT,
    title           TEXT NOT NULL,
    prompt          TEXT NOT NULL,
    status          TEXT NOT NULL CHECK (status IN (
        'QUEUED','RUNNING','WAITING_USER','WAITING_APPROVAL','WAITING_EXTERNAL',
        'PAUSED','COMPLETED','FAILED','CANCELLED')),
    priority        INTEGER NOT NULL DEFAULT 0,
    source          TEXT NOT NULL DEFAULT 'user',
    correlation_id  TEXT,
    result_json     TEXT,
    error           TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    started_at      TEXT,
    finished_at     TEXT
);
CREATE INDEX idx_tasks_dot_status ON tasks(dot_id, status);
CREATE INDEX idx_tasks_status_priority ON tasks(status, priority DESC, created_at);

CREATE TABLE executions (
    id          TEXT PRIMARY KEY,
    task_id     TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    dot_id      TEXT NOT NULL REFERENCES dots(id) ON DELETE CASCADE,
    engine      TEXT NOT NULL,
    attempt     INTEGER NOT NULL DEFAULT 1,
    status      TEXT NOT NULL CHECK (status IN ('RUNNING','COMPLETED','FAILED','CANCELLED','TIMEOUT')),
    pid         INTEGER,
    exit_code   INTEGER,
    engine_session_id TEXT,
    stdout_path TEXT,
    stderr_path TEXT,
    error       TEXT,
    started_at  TEXT NOT NULL,
    finished_at TEXT
);
CREATE INDEX idx_executions_task ON executions(task_id);

CREATE TABLE conversations (
    id         TEXT PRIMARY KEY,
    dot_id     TEXT NOT NULL REFERENCES dots(id) ON DELETE CASCADE,
    title      TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX idx_conversations_dot ON conversations(dot_id, updated_at DESC);

CREATE TABLE messages (
    id              TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    role            TEXT NOT NULL CHECK (role IN ('user','dot','system','tool')),
    content         TEXT NOT NULL,
    task_id         TEXT,
    execution_id    TEXT,
    metadata_json   TEXT NOT NULL DEFAULT '{}',
    created_at      TEXT NOT NULL
);
CREATE INDEX idx_messages_conversation ON messages(conversation_id, created_at);

CREATE TABLE memories (
    id         TEXT PRIMARY KEY,
    dot_id     TEXT NOT NULL REFERENCES dots(id) ON DELETE CASCADE,
    kind       TEXT NOT NULL CHECK (kind IN ('identity','working','long_term','history')),
    content    TEXT NOT NULL,
    importance INTEGER NOT NULL DEFAULT 0,
    source     TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX idx_memories_dot_kind ON memories(dot_id, kind);

CREATE TABLE engine_configurations (
    engine              TEXT PRIMARY KEY CHECK (engine IN ('codex','claude','agy')),
    enabled             INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0,1)),
    executable_override TEXT,
    last_detection_json TEXT,
    updated_at          TEXT NOT NULL
);

CREATE TABLE audit_logs (
    id           TEXT PRIMARY KEY,
    actor        TEXT NOT NULL,
    action       TEXT NOT NULL,
    entity_type  TEXT NOT NULL,
    entity_id    TEXT,
    details_json TEXT NOT NULL DEFAULT '{}',
    created_at   TEXT NOT NULL
);
CREATE INDEX idx_audit_logs_created ON audit_logs(created_at DESC);
CREATE INDEX idx_audit_logs_entity ON audit_logs(entity_type, entity_id);

CREATE TABLE application_settings (
    key        TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
