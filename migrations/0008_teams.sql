-- Equipes: um gerente (DOT) e funcionarios (DOTs). Um DOT gerencia no maximo uma equipe e e funcionario de no maximo uma.
CREATE TABLE teams (
    id             TEXT PRIMARY KEY,
    name           TEXT NOT NULL,
    description    TEXT NOT NULL DEFAULT '',
    manager_dot_id TEXT UNIQUE REFERENCES dots(id) ON DELETE SET NULL,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL
);
CREATE TABLE team_members (
    team_id    TEXT NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    dot_id     TEXT NOT NULL UNIQUE REFERENCES dots(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    PRIMARY KEY (team_id, dot_id)
);
