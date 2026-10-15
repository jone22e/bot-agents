-- DOTS Fase 2: tarefas, execucoes, conversas, memoria com busca, aprovacoes e politicas.

ALTER TABLE tasks ADD COLUMN kind TEXT NOT NULL DEFAULT 'task' CHECK (kind IN ('chat','task'));
ALTER TABLE tasks ADD COLUMN retry_safe INTEGER NOT NULL DEFAULT 0 CHECK (retry_safe IN (0,1));
ALTER TABLE tasks ADD COLUMN engine TEXT;
ALTER TABLE tasks ADD COLUMN cancel_requested INTEGER NOT NULL DEFAULT 0 CHECK (cancel_requested IN (0,1));
ALTER TABLE tasks ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN attachments_json TEXT NOT NULL DEFAULT '[]';

ALTER TABLE executions ADD COLUMN workspace_path TEXT;
ALTER TABLE executions ADD COLUMN events_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE executions ADD COLUMN cost_usd REAL;
ALTER TABLE executions ADD COLUMN usage_json TEXT NOT NULL DEFAULT '{}';

CREATE TABLE execution_events (
    id           TEXT PRIMARY KEY,
    execution_id TEXT NOT NULL REFERENCES executions(id) ON DELETE CASCADE,
    seq          INTEGER NOT NULL,
    type         TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    UNIQUE (execution_id, seq)
);

ALTER TABLE conversations ADD COLUMN engine_sessions_json TEXT NOT NULL DEFAULT '{}';

CREATE TABLE approvals (
    id             TEXT PRIMARY KEY,
    execution_id   TEXT NOT NULL REFERENCES executions(id) ON DELETE CASCADE,
    task_id        TEXT NOT NULL,
    dot_id         TEXT NOT NULL,
    tool           TEXT NOT NULL,
    input_json     TEXT NOT NULL,
    risk           TEXT NOT NULL CHECK (risk IN ('read','write','exec','critical')),
    resources_json TEXT NOT NULL DEFAULT '[]',
    summary        TEXT NOT NULL,
    status         TEXT NOT NULL CHECK (status IN ('pending','approved','rejected','expired')),
    requested_at   TEXT NOT NULL,
    expires_at     TEXT NOT NULL,
    resolved_at    TEXT,
    reason         TEXT
);
CREATE INDEX idx_approvals_status ON approvals(status, requested_at DESC);

CREATE TABLE tool_policies (
    dot_id                TEXT PRIMARY KEY REFERENCES dots(id) ON DELETE CASCADE,
    auto_approve_level    TEXT NOT NULL DEFAULT 'read' CHECK (auto_approve_level IN ('none','read','write','exec')),
    allowed_commands_json TEXT NOT NULL DEFAULT '[]',
    allow_network         INTEGER NOT NULL DEFAULT 0 CHECK (allow_network IN (0,1)),
    max_file_bytes        INTEGER NOT NULL DEFAULT 2097152,
    engine_native_write   INTEGER NOT NULL DEFAULT 0 CHECK (engine_native_write IN (0,1)),
    updated_at            TEXT NOT NULL
);

CREATE TABLE workspace_changes (
    id           TEXT PRIMARY KEY,
    execution_id TEXT NOT NULL REFERENCES executions(id) ON DELETE CASCADE,
    dot_id       TEXT NOT NULL,
    path         TEXT NOT NULL,
    kind         TEXT NOT NULL,
    summary      TEXT NOT NULL DEFAULT '',
    created_at   TEXT NOT NULL
);
CREATE INDEX idx_workspace_changes_execution ON workspace_changes(execution_id);

ALTER TABLE dot_workspaces ADD COLUMN is_external INTEGER NOT NULL DEFAULT 0 CHECK (is_external IN (0,1));

-- Busca textual de memorias (FTS5 autonomo, sincronizado por triggers; nao depende de rowid estavel).
CREATE VIRTUAL TABLE memories_fts USING fts5(memory_id UNINDEXED, dot_id UNINDEXED, content, tokenize = 'unicode61 remove_diacritics 2');
CREATE TRIGGER memories_ai AFTER INSERT ON memories BEGIN
    INSERT INTO memories_fts (memory_id, dot_id, content) VALUES (new.id, new.dot_id, new.content);
END;
CREATE TRIGGER memories_ad AFTER DELETE ON memories BEGIN
    DELETE FROM memories_fts WHERE memory_id = old.id;
END;
CREATE TRIGGER memories_au AFTER UPDATE OF content ON memories BEGIN
    DELETE FROM memories_fts WHERE memory_id = old.id;
    INSERT INTO memories_fts (memory_id, dot_id, content) VALUES (new.id, new.dot_id, new.content);
END;
