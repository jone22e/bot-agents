use dots_core::{DotsError, DotsResult, Memory, MemoryCreate, MemoryKind, MemoryUpdate, new_id};
use rusqlite::{Connection, OptionalExtension, Row, params};

use super::{parse_ts, ts};
use crate::storage_err;

pub struct MemoriesRepo<'a> {
    conn: &'a mut Connection,
}

const COLS: &str = "id, dot_id, kind, content, importance, source, created_at, updated_at";

fn map(r: &Row<'_>) -> rusqlite::Result<(String, String, String, String, i32, Option<String>, String, String)> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
}

fn to_mem(r: (String, String, String, String, i32, Option<String>, String, String)) -> DotsResult<Memory> {
    let (id, dot_id, kind, content, importance, source, created_at, updated_at) = r;
    Ok(Memory { id, dot_id, kind: MemoryKind::parse(&kind)?, content, importance, source, created_at: parse_ts(&created_at)?, updated_at: parse_ts(&updated_at)? })
}

/// Converte texto livre em consulta FTS5 segura: termos entre aspas unidos por OR.
fn fts_query(text: &str) -> Option<String> {
    let terms: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() >= 3)
        .map(|t| format!("\"{}\"", t.replace('"', "")))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .take(24)
        .collect();
    if terms.is_empty() { None } else { Some(terms.join(" OR ")) }
}

impl<'a> MemoriesRepo<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    pub fn get(&self, id: &str) -> DotsResult<Memory> {
        self.conn
            .query_row(&format!("SELECT {COLS} FROM memories WHERE id = ?1"), params![id], map)
            .optional()
            .map_err(storage_err)?
            .map(to_mem)
            .transpose()?
            .ok_or_else(|| DotsError::NotFound(format!("memoria {id}")))
    }

    pub fn list(&self, dot_id: &str, kind: Option<MemoryKind>, limit: u32) -> DotsResult<Vec<Memory>> {
        let limit = if limit == 0 { 500 } else { limit };
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {COLS} FROM memories WHERE dot_id = ?1 AND (?2 IS NULL OR kind = ?2) ORDER BY importance DESC, updated_at DESC LIMIT ?3"))
            .map_err(storage_err)?;
        let rows = stmt.query_map(params![dot_id, kind.map(|k| k.as_str().to_string()), limit], map).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(to_mem)).collect()
    }

    pub fn create(&mut self, input: MemoryCreate) -> DotsResult<Memory> {
        let content = input.content.trim();
        if content.is_empty() {
            return Err(DotsError::Validation("conteudo da memoria e obrigatorio".into()));
        }
        let id = new_id();
        let now = ts(&dots_core::now());
        self.conn
            .execute(
                "INSERT INTO memories (id, dot_id, kind, content, importance, source, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                params![id, input.dot_id, input.kind.as_str(), content, input.importance.unwrap_or(0), input.source, now],
            )
            .map_err(storage_err)?;
        self.get(&id)
    }

    pub fn update(&mut self, id: &str, patch: MemoryUpdate) -> DotsResult<Memory> {
        let current = self.get(id)?;
        let content = patch.content.map(|c| c.trim().to_string()).unwrap_or(current.content);
        if content.is_empty() {
            return Err(DotsError::Validation("conteudo da memoria e obrigatorio".into()));
        }
        self.conn
            .execute(
                "UPDATE memories SET content = ?1, importance = ?2, kind = ?3, updated_at = ?4 WHERE id = ?5",
                params![content, patch.importance.unwrap_or(current.importance), patch.kind.unwrap_or(current.kind).as_str(), ts(&dots_core::now()), id],
            )
            .map_err(storage_err)?;
        self.get(id)
    }

    pub fn delete(&mut self, id: &str) -> DotsResult<()> {
        let n = self.conn.execute("DELETE FROM memories WHERE id = ?1", params![id]).map_err(storage_err)?;
        if n == 0 {
            return Err(DotsError::NotFound(format!("memoria {id}")));
        }
        Ok(())
    }

    /// Zera a memoria do DOT (todas as categorias). Devolve quantas foram apagadas.
    pub fn delete_all_for_dot(&mut self, dot_id: &str) -> DotsResult<u32> {
        let n = self.conn.execute("DELETE FROM memories WHERE dot_id = ?1", params![dot_id]).map_err(storage_err)?;
        Ok(n as u32)
    }

    /// Memoria de trabalho: uma por (dot, source). Cria ou substitui.
    pub fn upsert_working(&mut self, dot_id: &str, source: &str, content: &str) -> DotsResult<Memory> {
        let existing: Option<String> = self
            .conn
            .query_row("SELECT id FROM memories WHERE dot_id = ?1 AND kind = 'working' AND source = ?2", params![dot_id, source], |r| r.get(0))
            .optional()
            .map_err(storage_err)?;
        match existing {
            Some(id) => self.update(&id, MemoryUpdate { content: Some(content.to_string()), ..Default::default() }),
            None => self.create(MemoryCreate { dot_id: dot_id.into(), kind: MemoryKind::Working, content: content.into(), importance: Some(0), source: Some(source.into()) }),
        }
    }

    /// Busca por relevancia (FTS5 bm25 + importancia). Sem termos validos, retorna as mais importantes/recentes.
    pub fn search(&self, dot_id: &str, query: &str, kinds: &[MemoryKind], limit: u32) -> DotsResult<Vec<Memory>> {
        let limit = if limit == 0 { 20 } else { limit };
        let kind_filter = if kinds.is_empty() {
            String::new()
        } else {
            format!(" AND m.kind IN ({})", kinds.iter().map(|k| format!("'{}'", k.as_str())).collect::<Vec<_>>().join(","))
        };
        if let Some(q) = fts_query(query) {
            let sql = format!(
                "SELECT {} FROM memories_fts f JOIN memories m ON m.id = f.memory_id
                 WHERE memories_fts MATCH ?1 AND m.dot_id = ?2{kind_filter}
                 ORDER BY (bm25(memories_fts) - m.importance * 0.5) LIMIT ?3",
                COLS.split(',').map(|c| format!("m.{}", c.trim())).collect::<Vec<_>>().join(", ")
            );
            let mut stmt = self.conn.prepare(&sql).map_err(storage_err)?;
            let rows = stmt.query_map(params![q, dot_id, limit], map).map_err(storage_err)?;
            let found: Vec<Memory> = rows.map(|r| r.map_err(storage_err).and_then(to_mem)).collect::<DotsResult<_>>()?;
            if !found.is_empty() {
                return Ok(found);
            }
        }
        let sql = format!("SELECT {COLS} FROM memories m WHERE m.dot_id = ?1{kind_filter} ORDER BY m.importance DESC, m.updated_at DESC LIMIT ?2");
        let mut stmt = self.conn.prepare(&sql).map_err(storage_err)?;
        let rows = stmt.query_map(params![dot_id, limit], map).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(to_mem)).collect()
    }

    /// Remove memorias de historico/trabalho alem de `keep` por DOT (compactacao por tamanho).
    pub fn prune(&mut self, dot_id: &str, kind: MemoryKind, keep: u32) -> DotsResult<u32> {
        let n = self
            .conn
            .execute(
                "DELETE FROM memories WHERE dot_id = ?1 AND kind = ?2 AND id NOT IN (
                    SELECT id FROM memories WHERE dot_id = ?1 AND kind = ?2 ORDER BY importance DESC, updated_at DESC LIMIT ?3)",
                params![dot_id, kind.as_str(), keep],
            )
            .map_err(storage_err)?;
        Ok(n as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::fts_query;

    #[test]
    fn builds_safe_fts_query() {
        assert_eq!(fts_query("ola \"mundo\" x (teste)").as_deref(), Some("\"mundo\" OR \"ola\" OR \"teste\""));
        assert_eq!(fts_query("a b"), None);
    }
}
