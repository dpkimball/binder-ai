use serde::{Deserialize, Serialize};

/// Named capability used in requirements. New variants can be added without
/// changing selector control flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelCapability {
    Chat,
    Coding,
    Reasoning,
    Planning,
    Tools,
    StructuredOutput,
    Streaming,
    Embeddings,
    Vision,
    Audio,
    LongContext,
}

/// Explicit model capabilities. Routing matches these, never vendor names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ModelCapabilities {
    #[serde(default)]
    pub chat: bool,
    #[serde(default)]
    pub coding: bool,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default)]
    pub planning: bool,
    #[serde(default)]
    pub tools: bool,
    #[serde(default)]
    pub structured_output: bool,
    #[serde(default)]
    pub streaming: bool,
    #[serde(default)]
    pub embeddings: bool,
    #[serde(default)]
    pub vision: bool,
    #[serde(default)]
    pub audio: bool,
    #[serde(default)]
    pub long_context: bool,
}

impl ModelCapabilities {
    pub fn supports(self, capability: ModelCapability) -> bool {
        match capability {
            ModelCapability::Chat => self.chat,
            ModelCapability::Coding => self.coding,
            ModelCapability::Reasoning => self.reasoning,
            ModelCapability::Planning => self.planning,
            ModelCapability::Tools => self.tools,
            ModelCapability::StructuredOutput => self.structured_output,
            ModelCapability::Streaming => self.streaming,
            ModelCapability::Embeddings => self.embeddings,
            ModelCapability::Vision => self.vision,
            ModelCapability::Audio => self.audio,
            ModelCapability::LongContext => self.long_context,
        }
    }

    pub fn required_fit(self, required: &[ModelCapability]) -> f64 {
        if required.is_empty() {
            return 1.0;
        }
        let ok = required.iter().filter(|c| self.supports(**c)).count();
        ok as f64 / required.len() as f64
    }
}
