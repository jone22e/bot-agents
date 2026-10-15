-- Fase Flexi: de qual papel (modelo) cada DOT ligado ao Flexi nasceu, para ligar a hierarquia automaticamente.
ALTER TABLE dot_integrations ADD COLUMN role_template TEXT;
