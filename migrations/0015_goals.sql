-- Metas (ADR 0017): o usuario declara uma meta na conversa; o gerente registra com dots_goal_create e lanca o
-- progresso a cada rodada com dots_goal_progress. O Flexi web mostra o andamento (espelhado por flexi_sync).
CREATE TABLE IF NOT EXISTS goals (
    id           TEXT PRIMARY KEY,
    dot_id       TEXT NOT NULL REFERENCES dots(id) ON DELETE CASCADE,
    title        TEXT NOT NULL,
    unit         TEXT NOT NULL DEFAULT '',
    target       REAL NOT NULL,
    current      REAL NOT NULL DEFAULT 0,
    period_start TEXT NOT NULL,
    period_end   TEXT,
    status       TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','done','cancelled')),
    task_id      TEXT,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    closed_at    TEXT
);
CREATE INDEX IF NOT EXISTS idx_goals_dot ON goals(dot_id, status, created_at);

CREATE TABLE IF NOT EXISTS goal_updates (
    id         TEXT PRIMARY KEY,
    goal_id    TEXT NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
    value      REAL NOT NULL,
    delta      REAL NOT NULL,
    note       TEXT NOT NULL DEFAULT '',
    task_id    TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_goal_updates_goal ON goal_updates(goal_id, created_at);
