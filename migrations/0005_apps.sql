-- DOTS Fase Flexi (migracao 5): aplicativos conectados e vinculo por DOT. Segredos nunca ficam aqui.

CREATE TABLE app_connections (
    id                TEXT PRIMARY KEY,
    kind              TEXT NOT NULL,
    label             TEXT NOT NULL,
    base_url          TEXT NOT NULL,
    server            TEXT,
    email             TEXT NOT NULL,
    user_name         TEXT,
    user_id           INTEGER,
    empresa           TEXT,
    base              TEXT,
    status            TEXT NOT NULL CHECK (status IN ('connected','needs_login','mfa_pending','error')),
    status_detail     TEXT,
    remember_password INTEGER NOT NULL DEFAULT 0 CHECK (remember_password IN (0,1)),
    last_login_at     TEXT,
    last_check_at     TEXT,
    created_at        TEXT NOT NULL
);

CREATE TABLE dot_integrations (
    dot_id        TEXT NOT NULL REFERENCES dots(id) ON DELETE CASCADE,
    kind          TEXT NOT NULL,
    connection_id TEXT NOT NULL REFERENCES app_connections(id) ON DELETE CASCADE,
    created_at    TEXT NOT NULL,
    PRIMARY KEY (dot_id, kind)
);
