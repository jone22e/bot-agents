# ADR 0003: SQLite via rusqlite embutido, ator em thread dedicada, migracoes SQL com checksum

Data: 2026-10-15. Estado: aceito.

## Decisao
- `rusqlite` com feature `bundled` (nenhuma instalacao externa) e `backup` (copia online).
- `Connection` nao e `Sync`: vive em uma unica thread (ator) alimentada por canal; escritas serializadas, sem `Mutex` entre tasks tokio.
- Migracoes `migrations/NNNN_*.sql` embutidas por `include_str!`, tabela `schema_migrations` com checksum; backup automatico antes de migrar; banco mais novo que o binario e erro (sem downgrade).

## Alternativas rejeitadas
- Crate `rusqlite_migration`: pouco ganho frente aos ganchos necessarios (backup, checksum, versao).
- Pool de conexoes: complexidade sem necessidade na Fase 1.
