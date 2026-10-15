# ADR 0004: tipos TS gerados do Rust com ts-rs

Data: 2026-10-15. Estado: aceito.

## Decisao
Todo tipo que atravessa o IPC deriva `TS` em `dots-core`; `pnpm gen:types` (= `cargo test -p dots-core export_bindings`)
regenera `packages/shared-types/src/generated`. Envelopes do IPC e tipos exclusivos do app Tauri sao escritos a mao em `index.ts`.
Campos `u64` expostos a UI usam `#[ts(type = "number")]` para evitar `bigint`.
