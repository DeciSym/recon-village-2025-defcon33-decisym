use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for OpenAI-compatible API requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichConfig {
    /// API endpoint URL (e.g., "http://localhost:8000/v1")
    pub api_url: String,

    /// Optional API key for authentication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Model name to use
    pub model: String,

    /// The prompt or messages to send
    #[serde(flatten)]
    pub prompt: PromptConfig,

    /// Generation parameters
    #[serde(flatten)]
    pub parameters: GenerationParams,

    /// Request timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
}

fn default_timeout() -> u64 {
    300 // 5 minutes default
}

/// Prompt configuration - either completion or chat format
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PromptConfig {
    /// Simple completion prompt
    Completion { prompt: String },
    /// Chat format with messages
    Chat { messages: Vec<ChatMessage> },
}

/// Chat message format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Generation parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationParams {
    /// Maximum tokens to generate
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,

    /// Temperature for sampling
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Top-p sampling parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,

    /// Number of sequences to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,

    /// Stop sequences
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,

    /// Random seed for reproducibility
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u32>,
}

fn default_max_tokens() -> u32 {
    1024
}

fn default_temperature() -> f32 {
    0.7
}

/// Response from completion endpoint
#[derive(Debug, Deserialize)]
struct CompletionResponse {
    choices: Vec<CompletionChoice>,
}

#[derive(Debug, Deserialize)]
struct CompletionChoice {
    text: String,
    finish_reason: Option<String>,
}

/// Response from chat completion endpoint
#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
    finish_reason: Option<String>,
}

/// Client for OpenAI-compatible APIs
pub struct OpenAIClient {
    client: Client,
}

impl OpenAIClient {
    /// Create a new client
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { client })
    }

    /// Send an enrichment request based on the configuration
    pub async fn enrich(&self, config: &EnrichConfig) -> Result<String> {
        match &config.prompt {
            PromptConfig::Completion { prompt } => self.complete(config, prompt).await,
            PromptConfig::Chat { messages } => self.chat_complete(config, messages).await,
        }
    }

    /// Send a completion request
    async fn complete(&self, config: &EnrichConfig, prompt: &str) -> Result<String> {
        let url = format!("{}/completions", config.api_url);

        let mut request_body = serde_json::json!({
            "model": config.model,
            "prompt": prompt,
            "max_tokens": config.parameters.max_tokens,
            "temperature": config.parameters.temperature,
        });

        // Add optional parameters
        if let Some(top_p) = config.parameters.top_p {
            request_body["top_p"] = serde_json::json!(top_p);
        }
        if let Some(n) = config.parameters.n {
            request_body["n"] = serde_json::json!(n);
        }
        if let Some(stop) = &config.parameters.stop {
            request_body["stop"] = serde_json::json!(stop);
        }
        if let Some(seed) = config.parameters.seed {
            request_body["seed"] = serde_json::json!(seed);
        }

        let mut req = self
            .client
            .post(&url)
            .json(&request_body)
            .timeout(Duration::from_secs(config.timeout_seconds));

        if let Some(api_key) = &config.api_key {
            req = req.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = req
            .send()
            .await
            .context("Failed to send completion request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("API request failed with status {}: {}", status, error_text);
        }

        let completion: CompletionResponse = response
            .json()
            .await
            .context("Failed to parse completion response")?;

        completion
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.text)
            .ok_or_else(|| anyhow::anyhow!("No completion returned"))
    }

    /// Send a chat completion request
    async fn chat_complete(
        &self,
        config: &EnrichConfig,
        messages: &[ChatMessage],
    ) -> Result<String> {
        let url = format!("{}/chat/completions", config.api_url);

        let mut request_body = serde_json::json!({
            "model": config.model,
            "messages": messages,
            "max_tokens": config.parameters.max_tokens,
            "temperature": config.parameters.temperature,
        });

        // Add optional parameters
        if let Some(top_p) = config.parameters.top_p {
            request_body["top_p"] = serde_json::json!(top_p);
        }
        if let Some(n) = config.parameters.n {
            request_body["n"] = serde_json::json!(n);
        }
        if let Some(stop) = &config.parameters.stop {
            request_body["stop"] = serde_json::json!(stop);
        }
        if let Some(seed) = config.parameters.seed {
            request_body["seed"] = serde_json::json!(seed);
        }

        let mut req = self
            .client
            .post(&url)
            .json(&request_body)
            .timeout(Duration::from_secs(config.timeout_seconds));

        if let Some(api_key) = &config.api_key {
            req = req.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = req
            .send()
            .await
            .context("Failed to send chat completion request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("API request failed with status {}: {}", status, error_text);
        }

        let chat_completion: ChatCompletionResponse = response
            .json()
            .await
            .context("Failed to parse chat completion response")?;

        chat_completion
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message.content)
            .ok_or_else(|| anyhow::anyhow!("No chat completion returned"))
    }
}

impl EnrichConfig {
    /// Load configuration from a YAML file
    pub fn from_yaml_file(path: &std::path::Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).context("Failed to read configuration file")?;
        serde_yaml::from_str(&content).context("Failed to parse YAML configuration")
    }

    /// Load configuration from a JSON file
    pub fn from_json_file(path: &std::path::Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).context("Failed to read configuration file")?;
        serde_json::from_str(&content).context("Failed to parse JSON configuration")
    }
}
