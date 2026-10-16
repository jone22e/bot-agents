-- Grupo geral: o usuario escreve para todos os gerentes de uma vez. Cada mensagem vira um pedido na conversa de cada
-- gerente (tarefa com origem "group:<id>"); as respostas deles sao lidas dessas conversas e aparecem no grupo.
CREATE TABLE group_messages (
    id            TEXT PRIMARY KEY,
    content       TEXT NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at    TEXT NOT NULL
);
CREATE INDEX group_messages_created ON group_messages(created_at);
CREATE INDEX idx_tasks_origin ON tasks(origin);
