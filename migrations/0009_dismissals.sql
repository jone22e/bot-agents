-- Demissoes: o gerente demite um funcionario por um motivo. O registro fica para que a equipe nao repita o erro;
-- o DOT demitido continua existindo (mesmo id), mas com a memoria e a conversa zeradas.
CREATE TABLE dismissals (
    id             TEXT PRIMARY KEY,
    team_id        TEXT REFERENCES teams(id) ON DELETE SET NULL,
    team_name      TEXT NOT NULL DEFAULT '',
    manager_dot_id TEXT REFERENCES dots(id) ON DELETE SET NULL,
    manager_name   TEXT NOT NULL DEFAULT '',
    dot_id         TEXT REFERENCES dots(id) ON DELETE SET NULL,
    dot_name       TEXT NOT NULL DEFAULT '',
    reason         TEXT NOT NULL,
    lesson         TEXT NOT NULL DEFAULT '',
    memories_wiped INTEGER NOT NULL DEFAULT 0,
    created_at     TEXT NOT NULL
);
CREATE INDEX dismissals_team ON dismissals(team_id, created_at);
CREATE INDEX dismissals_dot ON dismissals(dot_id, created_at);
