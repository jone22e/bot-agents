-- Modelo padrao por motor (opcional). O modelo por DOT fica em dot_engines.config_json ("model").

ALTER TABLE engine_configurations ADD COLUMN model TEXT;
