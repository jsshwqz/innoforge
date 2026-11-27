//! 模块职责：字符 N-Gram 特征哈希 + 专利语料 IDF 加权向量嵌入（512 维统一空间）及存量专利可断点后台回填（MB0b）。
//! 上游调用者：`src/routes/search.rs` (`/api/search/vector`)、`src/routes/patent.rs` (`/api/patents/backfill-*`)、`src/search/enrichment.rs`。
//! 模块专属禁区：严禁对 TF 向量做无特征维映射的直接降序重排（会导致查询与文档不在同一坐标空间）；严禁在回填循环中无上限全表锁库。

use super::Database;
use anyhow::Result;
use serde_json::json;
use std::collections::HashMap;

const EMBEDDING_DIM: usize = 512;

/// 专利高频套话与通用结构词停用表（在向量空间中大幅压低 IDF 权重，防止套话主导余弦相似度）
const PATENT_BOILERPLATE_GRAMS: &[&str] = &[
    "一种",
    "包括",
    "所述",
    "装置",
    "方法",
    "系统",
    "用于",
    "特征",
    "在于",
    "特征在于",
    "其特征在于",
    "本发明",
    "实用",
    "新型",
    "实用新型",
    "本实用",
    "权利要求",
    "权利",
    "要求",
    "根据",
    "根据权利",
    "所述一种",
    "连接",
    "连接件",
    "壳体",
    "外壳",
    "底座",
    "支架",
    "本体",
    "固定件",
    "设置",
    "设置有",
    "具有",
    "通过",
    "以及",
    "进一步",
    "固定",
    "安装",
    "设有",
    "位于",
    "内部",
    "外部",
    "结构",
    "组件",
    "机构",
    "部件",
    "部分",
    "第一",
    "第二",
    "第三",
    "提供",
    "涉及",
    "涉及一种",
    "公开",
    "公开了",
    "公开了一种",
    "技术",
    "方案",
    "技术方案",
    "技术领域",
    "背景技术",
    "发明内容",
    "具体实施",
    "实施方式",
    "实施例",
    "有益效果",
];

/// 包含任一核心套话片段的复合 N-Gram 统一降权（避免 `"一种包括壳体与连接件的"` 衍生出的 3/4-gram 获得加权）
const PATENT_BOILERPLATE_SUBSTRINGS: &[&str] = &[
    "一种",
    "所述",
    "包括",
    "特征",
    "在于",
    "本发明",
    "实用",
    "新型",
    "权利",
    "要求",
    "根据",
    "装置",
    "方法",
    "系统",
    "壳体",
    "外壳",
    "连接",
    "设置",
    "安装",
    "固定",
    "支架",
    "底座",
    "组件",
    "部件",
    "机构",
    "技术",
    "方案",
    "实施",
    "公开",
    "涉及",
    "第一",
    "第二",
    "第三",
];

/// 计算单个 N-Gram 的专利语料 IDF 权重：
/// - 命中专利套话停用表 → 降权至 `0.05`；
/// - 含套话片段或虚词连接字（`的/与/和/及/中/在/为/由/于`）的组合词 → 降权至 `0.15`；
/// - 纯专业技术 2-gram / 3-gram / 4-gram → 赋权 `1.0` / `1.3` / `1.6`。
pub fn patent_ngram_idf(gram: &str) -> f32 {
    if PATENT_BOILERPLATE_GRAMS.contains(&gram) {
        return 0.05;
    }
    if PATENT_BOILERPLATE_SUBSTRINGS
        .iter()
        .any(|bp| gram.contains(bp))
    {
        return 0.15;
    }
    if gram.chars().any(|c| {
        matches!(
            c,
            '的' | '与'
                | '和'
                | '及'
                | '中'
                | '在'
                | '为'
                | '由'
                | '于'
                | '将'
                | '对'
                | '以'
                | '其'
                | '件'
        )
    }) {
        return 0.25;
    }
    let char_len = gram.chars().count();
    match char_len {
        4 => 1.6,
        3 => 1.3,
        _ => 1.0,
    }
}

/// FNV-1a 64-bit 确定性哈希：将同一 N-Gram 固定映射到 `0..EMBEDDING_DIM` 的同一维坐标与符号
fn fnv1a_hash_gram(gram: &str) -> (usize, f32) {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in gram.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let bucket = (hash as usize) % EMBEDDING_DIM;
    // 使用高位比特做有符号特征哈希（Signed Feature Hashing / Weinberger et al.），消除哈希碰撞内积偏置
    let sign = if ((hash >> 32) & 1) == 0 {
        1.0f32
    } else {
        -1.0f32
    };
    (bucket, sign)
}

/// Compute a char-n-gram TF-IDF feature-hashed embedding for the given text.
///
/// 先按空白与中英文标点切分连续词段，再在各词段内部提取 `n = 2..=4` 字符 N-Gram（严禁跨空格拼接伪词元）
/// + sublinear TF + patent-corpus IDF weighting + signed feature hashing into `EMBEDDING_DIM` (512).
pub fn compute_tfidf_embedding(text: &str) -> Vec<f32> {
    let mut tf: HashMap<String, f32> = HashMap::new();
    for segment in text.split(|c: char| {
        c.is_whitespace()
            || c.is_ascii_punctuation()
            || matches!(
                c,
                '，' | '。'
                    | '；'
                    | '：'
                    | '、'
                    | '（'
                    | '）'
                    | '【'
                    | '】'
                    | '《'
                    | '》'
                    | '“'
                    | '”'
                    | '‘'
                    | '’'
                    | '！'
                    | '？'
                    | '—'
                    | '…'
            )
    }) {
        let chars: Vec<char> = segment.chars().map(|c| c.to_ascii_lowercase()).collect();
        if chars.len() < 2 {
            continue;
        }
        for n in 2..=4 {
            if chars.len() >= n {
                for i in 0..=chars.len() - n {
                    let gram: String = chars[i..i + n].iter().collect();
                    *tf.entry(gram).or_insert(0.0) += 1.0;
                }
            }
        }
    }
    if tf.is_empty() {
        return vec![0.0f32; EMBEDDING_DIM];
    }

    let mut emb = vec![0.0f32; EMBEDDING_DIM];
    for (gram, raw_count) in &tf {
        // 亚线性 TF: 1 + ln(count)
        let sublinear_tf = 1.0 + raw_count.ln();
        let idf = patent_ngram_idf(gram);
        let weight = sublinear_tf * idf;
        let (bucket, sign) = fnv1a_hash_gram(gram);
        emb[bucket] += sign * weight;
    }

    let norm_sq: f32 = emb.iter().map(|v| v * v).sum();
    let norm = if norm_sq > 1e-12 { norm_sq.sqrt() } else { 0.0 };
    if norm > 0.0 {
        for v in &mut emb {
            *v /= norm;
        }
    }
    emb
}

/// Cosine similarity between two vectors (clamped to `[0.0, 1.0]`).
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    (dot / (norm_a * norm_b)).clamp(0.0, 1.0)
}

/// Try to compute and persist an embedding for a patent.
pub fn try_compute_and_save_embedding(db: &Database, patent_id: &str, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    let embedding = compute_tfidf_embedding(text);
    if let Err(e) = db.save_patent_embedding(patent_id, &embedding, "char-tfidf-v2") {
        tracing::warn!(
            "Embedding computation failed for patent {}: {}",
            patent_id,
            e
        );
    }
}

impl Database {
    /// Save a patent embedding (INSERT OR REPLACE).
    pub fn save_patent_embedding(
        &self,
        patent_id: &str,
        embedding: &[f32],
        model_name: &str,
    ) -> Result<(), rusqlite::Error> {
        let c = self.conn();
        let bytes: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
        let hash: u64 = embedding.iter().fold(0u64, |acc, f| {
            acc.wrapping_add(f.to_bits() as u64) ^ 0x9E3779B9u64
        });
        let hash_str = hash.to_string();

        c.execute(
            "INSERT OR REPLACE INTO patents_embedding (patent_id, embedding, model_name, text_hash, updated_at) VALUES (?1, ?2, ?3, ?4, datetime('now'))",
            rusqlite::params![patent_id, rusqlite::types::Value::Blob(bytes), model_name, hash_str],
        )?;
        Ok(())
    }

    /// Get a patent embedding by ID.
    pub fn get_patent_embedding(
        &self,
        patent_id: &str,
    ) -> Result<Option<Vec<f32>>, rusqlite::Error> {
        let c = self.conn();
        let mut stmt = c.prepare("SELECT embedding FROM patents_embedding WHERE patent_id = ?1")?;
        let rows = stmt.query_map(rusqlite::params![patent_id], |row: &rusqlite::Row| {
            let blob: rusqlite::types::Value = row.get(0)?;
            match blob {
                rusqlite::types::Value::Blob(bytes) => {
                    let mut embedding = Vec::with_capacity(bytes.len() / 4);
                    for i in (0..bytes.len()).step_by(4) {
                        if i + 3 < bytes.len() {
                            let bytes4 = [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]];
                            embedding.push(f32::from_le_bytes(bytes4));
                        }
                    }
                    Ok(embedding)
                }
                _ => Ok(vec![]),
            }
        })?;

        if let Some(emb) = rows.flatten().next() {
            return Ok(Some(emb));
        }
        Ok(None)
    }

    /// List patents that still need v2 embedding.
    pub fn list_patents_needing_embedding(&self) -> Result<Vec<String>, rusqlite::Error> {
        let c = self.conn();
        let mut stmt = c.prepare(
            "SELECT p.id FROM patents p
             LEFT JOIN patents_embedding pe ON p.id = pe.patent_id
             WHERE (pe.patent_id IS NULL OR pe.model_name != 'char-tfidf-v2')
             AND (p.description IS NOT NULL OR p.abstract_text IS NOT NULL)
             LIMIT 500",
        )?;
        let rows = stmt.query_map(rusqlite::params![], |row: &rusqlite::Row| {
            let id: String = row.get(0)?;
            Ok(id)
        })?;
        let mut ids = Vec::new();
        for row in rows {
            ids.push(row?);
        }
        Ok(ids)
    }

    /// Count how many patents have embeddings.
    pub fn count_embeddings(&self) -> Result<i64, rusqlite::Error> {
        let c = self.conn();
        let count: i64 = c.query_row(
            "SELECT COUNT(*) FROM patents_embedding",
            rusqlite::params![],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// MB0b: 查询存量专利的向量与分块索引覆盖状态
    ///
    /// 注：轻量著录批量导入（`source LIKE '%_lightweight'`）默认仅走 FTS5 词法粗筛，命中后再按需补全文与向量，
    /// 因此不计入待回填向量队列（单独在 `lightweight_patents` 披露），防止百万级公报拖垮本地全表向量检索。
    pub fn get_backfill_index_status(&self) -> Result<serde_json::Value, rusqlite::Error> {
        let c = self.conn();
        let all_patents: i64 = c.query_row("SELECT COUNT(*) FROM patents", [], |row| row.get(0))?;
        let lightweight_patents: i64 = c.query_row(
            "SELECT COUNT(*) FROM patents WHERE COALESCE(source, '') LIKE '%_lightweight'",
            [],
            |row| row.get(0),
        )?;
        let total_patents = (all_patents - lightweight_patents).max(0);
        let embedded_v2: i64 = c.query_row(
            "SELECT COUNT(*) FROM patents_embedding pe
             INNER JOIN patents p ON p.id = pe.patent_id
             WHERE pe.model_name = 'char-tfidf-v2'
               AND COALESCE(p.source, '') NOT LIKE '%_lightweight'",
            [],
            |row| row.get(0),
        )?;
        let chunked_patents: i64 = c.query_row(
            "SELECT COUNT(DISTINCT pc.patent_id) FROM patent_chunks pc
             INNER JOIN patents p ON p.id = pc.patent_id
             WHERE COALESCE(p.source, '') NOT LIKE '%_lightweight'",
            [],
            |row| row.get(0),
        )?;
        let needing_work: i64 = c.query_row(
            "SELECT COUNT(*) FROM patents p
             LEFT JOIN patents_embedding pe ON p.id = pe.patent_id AND pe.model_name = 'char-tfidf-v2'
             WHERE COALESCE(p.source, '') NOT LIKE '%_lightweight'
               AND (pe.patent_id IS NULL
                    OR NOT EXISTS (SELECT 1 FROM patent_chunks pc WHERE pc.patent_id = p.id))",
            [],
            |row| row.get(0),
        )?;
        let ready_patents = (total_patents - needing_work).max(0);
        let progress_percent = if total_patents <= 0 {
            100.0
        } else {
            ((ready_patents as f64 / total_patents as f64) * 100.0 * 10.0).round() / 10.0
        };
        Ok(json!({
            "status": "ok",
            "total_patents": total_patents,
            "all_patents": all_patents,
            "lightweight_patents": lightweight_patents,
            "ready_patents": ready_patents,
            "embedded_patents": embedded_v2,
            "chunked_patents": chunked_patents,
            "remaining": needing_work,
            "done": needing_work == 0,
            "progress_percent": progress_percent,
        }))
    }

    /// MB0b: 按批可断点回填存量专利的 `patents_embedding` (`char-tfidf-v2`) 与 `patent_chunks`
    pub fn backfill_embeddings_and_chunks_batch(
        &self,
        batch_size: usize,
    ) -> Result<serde_json::Value, rusqlite::Error> {
        let limit = batch_size.clamp(1, 200);
        let batch_rows: Vec<(String, String, String, String, String)> = {
            let c = self.conn();
            let mut stmt = c.prepare(
                "SELECT p.id, COALESCE(p.title, ''), COALESCE(p.abstract_text, ''), COALESCE(p.claims, ''), COALESCE(p.description, '')
                 FROM patents p
                 LEFT JOIN patents_embedding pe ON p.id = pe.patent_id AND pe.model_name = 'char-tfidf-v2'
                 WHERE COALESCE(p.source, '') NOT LIKE '%_lightweight'
                   AND (pe.patent_id IS NULL
                        OR NOT EXISTS (SELECT 1 FROM patent_chunks pc WHERE pc.patent_id = p.id))
                 LIMIT ?1",
            )?;
            let rows = stmt.query_map(rusqlite::params![limit as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?;
            let mut list = Vec::new();
            for r in rows {
                list.push(r?);
            }
            list
        };

        let mut processed = 0usize;
        let mut chunks_saved = 0usize;
        for (pid, title, abstract_text, claims, description) in batch_rows {
            let full_for_emb = format!("{title} {abstract_text} {claims}");
            let emb = compute_tfidf_embedding(&full_for_emb);
            self.save_patent_embedding(&pid, &emb, "char-tfidf-v2")?;

            let chunks = crate::rag::build_chunks_from_patent(
                &pid,
                &title,
                &abstract_text,
                &claims,
                &description,
            );
            if !chunks.is_empty() {
                chunks_saved += self.save_patent_chunks(&pid, &chunks, "char-tfidf-v2")?;
            } else {
                // 即使专利所有文本字段均为空，也写入一条合法 source_type ('abstract') 的占位分块避免断点游标死循环
                let placeholder = vec![crate::db::rag::PatentChunk {
                    id: format!("{pid}_empty_0"),
                    patent_id: pid.clone(),
                    chunk_index: 0,
                    source_type: "abstract".to_string(),
                    section_tag: "Meta:Empty".to_string(),
                    content: title,
                    embedding: vec![0.0f32; EMBEDDING_DIM],
                }];
                chunks_saved += self.save_patent_chunks(&pid, &placeholder, "char-tfidf-v2")?;
            }
            processed += 1;
        }

        let mut status = self.get_backfill_index_status()?;
        if let Some(obj) = status.as_object_mut() {
            obj.insert("processed".to_string(), json!(processed));
            obj.insert("chunks_saved".to_string(), json!(chunks_saved));
        }
        Ok(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idf_feature_hashed_embedding_suppresses_boilerplate_and_ranks_domain_match_higher() {
        let query = "一种包括壳体与连接件的硫化物固态电池快离子导体隔热结构";
        let related_doc = "硫化物固态电解质电池的快离子导体复合隔热片及热失控防护组件";
        let boilerplate_noise_doc = "一种包括壳体与连接件的折叠屏手机铰链旋转定位装置及方法";

        let q_emb = compute_tfidf_embedding(query);
        let rel_emb = compute_tfidf_embedding(related_doc);
        let noise_emb = compute_tfidf_embedding(boilerplate_noise_doc);

        let sim_related = cosine_similarity(&q_emb, &rel_emb);
        let sim_noise = cosine_similarity(&q_emb, &noise_emb);

        assert!(
            sim_related > 0.25,
            "Domain-related patent must have meaningful cosine similarity: {sim_related}"
        );
        assert!(
            sim_related > sim_noise * 2.0,
            "IDF must suppress boilerplate overlap so related ({sim_related:.3}) >> noise ({sim_noise:.3})"
        );
    }

    #[test]
    fn backfill_embeddings_and_chunks_is_resumable_and_converges_to_done() {
        let db = Database::init(":memory:").expect("init db");
        for i in 1..=3 {
            let p = crate::patent::Patent {
                id: format!("pat-bf-{i}"),
                patent_number: format!("CN99000{i}A"),
                title: format!("测试固态电池隔热结构 {i}"),
                abstract_text: "一种采用气凝胶与相变复合层的电池热失控隔热装置。".to_string(),
                claims: "1. 一种固态电池隔热结构，包括气凝胶层与导热骨架。".to_string(),
                description: "[0001] 本发明涉及固态电池热管理领域。".to_string(),
                ..Default::default()
            };
            db.insert_patent(&p).expect("insert patent");
        }

        // 额外插入 1 条所有文本字段均为空的脏专利，验证占位分块满足 CHECK 约束且不导致回填死循环
        db.insert_patent(&crate::patent::Patent {
            id: "pat-bf-empty".to_string(),
            patent_number: "CN990009A".to_string(),
            ..Default::default()
        })
        .expect("insert empty patent");

        let initial = db.get_backfill_index_status().expect("status");
        assert_eq!(initial["total_patents"], 4);
        assert_eq!(initial["remaining"], 4);
        assert_eq!(initial["done"], false);

        // 第 1 批只处理 2 条
        let step1 = db.backfill_embeddings_and_chunks_batch(2).expect("batch 1");
        assert_eq!(step1["processed"], 2);
        assert_eq!(step1["remaining"], 2);
        assert_eq!(step1["done"], false);

        // 第 2 批断点续跑处理剩余 2 条（含空文本专利）并收敛至 done=true
        let step2 = db.backfill_embeddings_and_chunks_batch(2).expect("batch 2");
        assert_eq!(step2["processed"], 2);
        assert_eq!(step2["remaining"], 0);
        assert_eq!(step2["done"], true);
        assert_eq!(step2["embedded_patents"], 4);
        assert_eq!(step2["chunked_patents"], 4);
        assert_eq!(step2["ready_patents"], 4);
    }

    #[test]
    fn backfill_status_separates_ready_from_v2_embedded_when_chunks_missing() {
        let db = Database::init(":memory:").expect("init db");
        for i in 1..=2 {
            let p = crate::patent::Patent {
                id: format!("pat-partial-{i}"),
                patent_number: format!("CN99010{i}A"),
                title: format!("测试微压氧舱控制系统 {i}"),
                abstract_text: "一种微压氧舱压力监测与自动泄压装置。".to_string(),
                ..Default::default()
            };
            db.insert_patent(&p).expect("insert patent");
            try_compute_and_save_embedding(&db, &p.id, &format!("{} {}", p.title, p.abstract_text));
        }

        // 仅第 1 篇写入段落分块，第 2 篇仅有 v2 向量但无分块
        let chunks = crate::rag::build_chunks_from_patent(
            "pat-partial-1",
            "测试微压氧舱控制系统 1",
            "一种微压氧舱压力监测与自动泄压装置。",
            "",
            "",
        );
        db.save_patent_chunks("pat-partial-1", &chunks, "char-tfidf-v2")
            .expect("save chunks");

        let st = db.get_backfill_index_status().expect("status");
        assert_eq!(st["total_patents"], 2);
        assert_eq!(st["embedded_patents"], 2);
        assert_eq!(st["chunked_patents"], 1);
        assert_eq!(st["ready_patents"], 1);
        assert_eq!(st["remaining"], 1);
        assert_eq!(st["progress_percent"], 50.0);
        assert_eq!(
            st["ready_patents"].as_i64().unwrap_or(0) + st["remaining"].as_i64().unwrap_or(0),
            st["total_patents"].as_i64().unwrap_or(0)
        );
    }

    #[test]
    fn backfill_status_excludes_lightweight_gazette_stubs_from_remaining() {
        let db = Database::init(":memory:").expect("init db");
        let full = crate::patent::Patent {
            id: "pat-full-1".to_string(),
            patent_number: "CN990201A".to_string(),
            title: "全量富化微压氧舱控制装置".to_string(),
            abstract_text: "包含完整权利要求与说明书。".to_string(),
            source: "google_patents".to_string(),
            ..Default::default()
        };
        let lw = crate::patent::Patent {
            id: "pat-lw-1".to_string(),
            patent_number: "CN990202A".to_string(),
            title: "公报轻量著录专利".to_string(),
            abstract_text: "仅用于 FTS5 粗筛。".to_string(),
            source: "folder_sync_lightweight".to_string(),
            ..Default::default()
        };
        db.batch_import_patents_chunked(&[full], false, "char-tfidf-v2", None)
            .expect("import full");
        db.batch_import_patents_chunked(&[lw], true, "char-tfidf-v2", None)
            .expect("import lightweight");

        let st = db.get_backfill_index_status().expect("status");
        assert_eq!(st["all_patents"], 2);
        assert_eq!(st["lightweight_patents"], 1);
        assert_eq!(st["total_patents"], 1);
        assert_eq!(st["ready_patents"], 1);
        assert_eq!(st["remaining"], 0);
        assert_eq!(st["done"], true);
    }
}
