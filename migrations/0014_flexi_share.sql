-- Compartilhamento de DOTs no Flexi web (ADR 0016): interruptor + usuarios escolhidos, e a conversa unica
-- (tipo grupo) que recebe as mensagens vindas do Flexi.
ALTER TABLE dots ADD COLUMN flexi_share_json TEXT NOT NULL DEFAULT '{"enabled":false,"users":[]}';
ALTER TABLE dots ADD COLUMN flexi_conversation_id TEXT REFERENCES conversations(id) ON DELETE SET NULL;
