-- Notificacoes: um DOT avisa o usuario (dots_notify_user) de algo que precisa dele. Ficam no topo da tela
-- ate o usuario clicar (abrir a conversa ou limpar); nada some sozinho.
CREATE TABLE notifications (
    id         TEXT PRIMARY KEY,
    dot_id     TEXT NOT NULL REFERENCES dots(id) ON DELETE CASCADE,
    task_id    TEXT,
    title      TEXT NOT NULL,
    detail     TEXT NOT NULL DEFAULT '',
    action     TEXT NOT NULL DEFAULT '',
    status     TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL,
    cleared_at TEXT
);
CREATE INDEX notifications_pending ON notifications(status, created_at);
CREATE INDEX notifications_dot ON notifications(dot_id, status);
