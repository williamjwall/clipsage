use sqlx::{SqlitePool, Row};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use anyhow::Result;
use crate::ollama::OllamaClient;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipItem {
    pub id: String,
    pub content: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub timestamp: DateTime<Utc>,
    pub source: Option<String>,
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Clone)]
pub struct Database {
    pool: SqlitePool,
    ollama: OllamaClient,
}

impl Database {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = SqlitePool::connect(database_url).await?;
        let ollama = OllamaClient::new("nomic-embed-text");
        
        // Create tables
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS clips (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                summary TEXT NOT NULL,
                tags TEXT NOT NULL, -- JSON array
                timestamp TEXT NOT NULL,
                source TEXT,
                embedding BLOB -- Vector embedding as binary data
            )
            "#,
        )
        .execute(&pool)
        .await?;

        // Create FTS5 virtual table for full-text search
        sqlx::query(
            r#"
            CREATE VIRTUAL TABLE IF NOT EXISTS clips_fts USING fts5(
                id UNINDEXED,
                content,
                summary,
                tags,
                source,
                content='clips',
                content_rowid='rowid'
            )
            "#,
        )
        .execute(&pool)
        .await?;

        // Create triggers to keep FTS table in sync
        sqlx::query(
            r#"
            CREATE TRIGGER IF NOT EXISTS clips_ai AFTER INSERT ON clips BEGIN
                INSERT INTO clips_fts(id, content, summary, tags, source)
                VALUES (new.id, new.content, new.summary, new.tags, new.source);
            END
            "#,
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TRIGGER IF NOT EXISTS clips_ad AFTER DELETE ON clips BEGIN
                DELETE FROM clips_fts WHERE id = old.id;
            END
            "#,
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TRIGGER IF NOT EXISTS clips_au AFTER UPDATE ON clips BEGIN
                UPDATE clips_fts SET 
                    content = new.content,
                    summary = new.summary,
                    tags = new.tags,
                    source = new.source
                WHERE id = new.id;
            END
            "#,
        )
        .execute(&pool)
        .await?;

        Ok(Database { pool, ollama })
    }

    pub async fn insert_clip(&self, clip: &ClipItem) -> Result<()> {
        let tags_json = serde_json::to_string(&clip.tags)?;
        
        // Generate embedding using Ollama if not provided
        let embedding = if clip.embedding.is_none() {
            Some(self.ollama.get_embedding(&clip.content).await?)
        } else {
            clip.embedding.clone()
        };

        let embedding_bytes = embedding.map(|e| {
            e.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>()
        });

        sqlx::query(
            r#"
            INSERT INTO clips (id, content, summary, tags, timestamp, source, embedding)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&clip.id)
        .bind(&clip.content)
        .bind(&clip.summary)
        .bind(&tags_json)
        .bind(clip.timestamp.to_rfc3339())
        .bind(&clip.source)
        .bind(embedding_bytes)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn search_clips(&self, query: &str, limit: i32) -> Result<Vec<ClipItem>> {
        // Get text search results
        let text_results = self.text_search(query, limit).await?;
        
        // Get semantic search results
        let query_embedding = self.ollama.get_embedding(query).await?;
        let semantic_results = self.semantic_search(&query_embedding, limit).await?;
        
        // Combine and deduplicate results
        let mut combined = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();
        
        for clip in text_results.into_iter().chain(semantic_results.into_iter()) {
            if seen_ids.insert(clip.id.clone()) {
                combined.push(clip);
            }
        }
        
        Ok(combined.into_iter().take(limit as usize).collect())
    }

    async fn text_search(&self, query: &str, limit: i32) -> Result<Vec<ClipItem>> {
        let rows = sqlx::query(
            r#"
            SELECT c.id, c.content, c.summary, c.tags, c.timestamp, c.source, c.embedding
            FROM clips c
            JOIN clips_fts fts ON c.id = fts.id
            WHERE clips_fts MATCH ?
            ORDER BY rank
            LIMIT ?
            "#,
        )
        .bind(query)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        self.rows_to_clips(rows).await
    }

    pub async fn get_recent_clips(&self, limit: i32) -> Result<Vec<ClipItem>> {
        let rows = sqlx::query(
            r#"
            SELECT id, content, summary, tags, timestamp, source, embedding
            FROM clips
            ORDER BY timestamp DESC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        self.rows_to_clips(rows).await
    }

    pub async fn semantic_search(&self, query_embedding: &[f32], limit: i32) -> Result<Vec<ClipItem>> {
        let all_clips = self.get_recent_clips(1000).await?;
        
        let mut scored_clips: Vec<(f32, ClipItem)> = all_clips
            .into_iter()
            .filter_map(|clip| {
                clip.embedding.as_ref().map(|embedding| {
                    let similarity = cosine_similarity(query_embedding, embedding);
                    (similarity, clip.clone())
                })
            })
            .collect();

        scored_clips.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        
        Ok(scored_clips
            .into_iter()
            .take(limit as usize)
            .map(|(_, clip)| clip)
            .collect())
    }

    async fn rows_to_clips(&self, rows: Vec<sqlx::sqlite::SqliteRow>) -> Result<Vec<ClipItem>> {
        let mut clips = Vec::new();
        
        for row in rows {
            let id: String = row.get("id");
            let content: String = row.get("content");
            let summary: String = row.get("summary");
            let tags_json: String = row.get("tags");
            let timestamp_str: String = row.get("timestamp");
            let source: Option<String> = row.get("source");
            let embedding_bytes: Option<Vec<u8>> = row.get("embedding");

            let tags: Vec<String> = serde_json::from_str(&tags_json)?;
            let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)?.with_timezone(&Utc);
            
            let embedding = embedding_bytes.map(|bytes| {
                bytes
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect()
            });

            clips.push(ClipItem {
                id,
                content,
                summary,
                tags,
                timestamp,
                source,
                embedding,
            });
        }

        Ok(clips)
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot_product / (norm_a * norm_b)
    }
} 