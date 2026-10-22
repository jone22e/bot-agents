-- Avaliacao opcional informada pelo agente ao solicitar uma acao.
ALTER TABLE approvals ADD COLUMN confidence_percent INTEGER CHECK (confidence_percent BETWEEN 0 AND 100);
ALTER TABLE approvals ADD COLUMN reversible INTEGER CHECK (reversible IN (0, 1));
