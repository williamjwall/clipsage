use anyhow::Result;
use serde::{Deserialize, Serialize};
use reqwest::Client;

const OLLAMA_API_URL: &str = "http://localhost:11434";

#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    model: String,
    prompt: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    embedding: Vec<f32>,
}

pub struct OllamaClient {
    client: Client,
    model: String,
}

impl OllamaClient {
    pub fn new(model: &str) -> Self {
        Self {
            client: Client::new(),
            model: model.to_string(),
        }
    }

    pub async fn get_embedding(&self, text: &str) -> Result<Vec<f32>> {
        let request = EmbeddingRequest {
            model: self.model.clone(),
            prompt: text.to_string(),
        };

        let response = self.client
            .post(format!("{}/api/embeddings", OLLAMA_API_URL))
            .json(&request)
            .send()
            .await?
            .json::<EmbeddingResponse>()
            .await?;

        Ok(response.embedding)
    }

    pub async fn generate_summary(&self, text: &str) -> Result<String> {
        let prompt = format!(
            "Summarize the following text in one short sentence:\n\n{}",
            text
        );

        let request = serde_json::json!({
            "model": self.model,
            "prompt": prompt,
            "stream": false
        });

        let response = self.client
            .post(format!("{}/api/generate", OLLAMA_API_URL))
            .json(&request)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        Ok(response["response"].as_str().unwrap_or("").to_string())
    }
} 