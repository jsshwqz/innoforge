use crate::patent::{CadArtifact, CadContextKind, CadValidation};
use anyhow::{Context, Result};
use rusqlite::{params, OptionalExtension};

impl super::Database {
    pub fn insert_cad_artifact(&self, artifact: &CadArtifact) -> Result<CadArtifact> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let revision: i64 = tx.query_row(
            "SELECT COALESCE(MAX(revision), 0) + 1 FROM cad_artifacts WHERE context_kind=?1 AND context_id=?2",
            params![artifact.context_kind.as_str(), artifact.context_id],
            |row| row.get(0),
        )?;
        let assumptions = serde_json::to_string(&artifact.assumptions)?;
        let validation = serde_json::to_string(&artifact.validation)?;
        tx.execute(
            "INSERT INTO cad_artifacts (id,context_kind,context_id,parent_artifact_id,revision,prompt,assumptions_json,preview_rel_path,fcstd_rel_path,step_rel_path,validation_json) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![artifact.id, artifact.context_kind.as_str(), artifact.context_id, artifact.parent_artifact_id, revision, artifact.prompt, assumptions, artifact.preview_rel_path, artifact.fcstd_rel_path, artifact.step_rel_path, validation],
        )?;
        tx.commit()?;
        drop(conn);
        self.get_cad_artifact(&artifact.id)?
            .context("inserted CAD artifact is missing")
    }

    pub fn get_cad_artifact(&self, id: &str) -> Result<Option<CadArtifact>> {
        let conn = self.conn();
        conn.query_row(
            "SELECT id,context_kind,context_id,parent_artifact_id,revision,prompt,assumptions_json,preview_rel_path,fcstd_rel_path,step_rel_path,validation_json,created_at FROM cad_artifacts WHERE id=?1",
            params![id], map_artifact,
        ).optional().map_err(Into::into)
    }

    pub fn list_cad_artifacts(
        &self,
        kind: &CadContextKind,
        context_id: &str,
    ) -> Result<Vec<CadArtifact>> {
        let conn = self.conn();
        let mut statement = conn.prepare("SELECT id,context_kind,context_id,parent_artifact_id,revision,prompt,assumptions_json,preview_rel_path,fcstd_rel_path,step_rel_path,validation_json,created_at FROM cad_artifacts WHERE context_kind=?1 AND context_id=?2 ORDER BY revision DESC")?;
        let rows = statement.query_map(params![kind.as_str(), context_id], map_artifact)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }
}

fn map_artifact(row: &rusqlite::Row<'_>) -> rusqlite::Result<CadArtifact> {
    let kind: String = row.get(1)?;
    let assumptions: String = row.get(6)?;
    let validation: String = row.get(10)?;
    Ok(CadArtifact {
        id: row.get(0)?,
        context_kind: match kind.as_str() {
            "idea" => CadContextKind::Idea,
            "patent" => CadContextKind::Patent,
            "oa" => CadContextKind::Oa,
            _ => return Err(rusqlite::Error::InvalidQuery),
        },
        context_id: row.get(2)?,
        parent_artifact_id: row.get(3)?,
        revision: row.get(4)?,
        prompt: row.get(5)?,
        assumptions: serde_json::from_str(&assumptions).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                6,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        preview_rel_path: row.get(7)?,
        fcstd_rel_path: row.get(8)?,
        step_rel_path: row.get(9)?,
        validation: serde_json::from_str::<CadValidation>(&validation).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                10,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        created_at: row.get(11)?,
    })
}
