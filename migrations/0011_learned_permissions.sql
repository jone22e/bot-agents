-- Permissoes aprendidas: aprovada uma vez, a acao do mesmo tipo (ex.: publicar no Flexi) nao pede de novo para o DOT.
CREATE TABLE learned_permissions (
    id          TEXT PRIMARY KEY,
    dot_id      TEXT NOT NULL REFERENCES dots(id) ON DELETE CASCADE,
    action_key  TEXT NOT NULL,
    summary     TEXT NOT NULL DEFAULT '',
    approval_id TEXT,
    created_at  TEXT NOT NULL,
    UNIQUE (dot_id, action_key)
);
