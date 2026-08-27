use std::sync::Mutex;

use async_trait::async_trait;

use binder_llm::{
    ChatRequest, ChatResponse, ChatStream, EmbeddingRequest, EmbeddingResponse, LlmError,
    LlmProvider, ModelInfo,
};

/// Queued LLM for tests. Each `chat` pops the next response.
#[derive(Debug, Default)]
pub struct ScriptedProvider {
    inner: Mutex<ScriptedInner>,
}

#[derive(Debug, Default)]
struct ScriptedInner {
    chats: Vec<std::result::Result<ChatResponse, LlmError>>,
    embeddings: Option<EmbeddingResponse>,
    models: Vec<ModelInfo>,
    chat_calls: u32,
}

impl ScriptedProvider {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_chat(self, response: ChatResponse) -> Self {
        self.inner
            .lock()
            .expect("scripted mutex")
            .chats
            .push(Ok(response));
        self
    }

    pub fn push_chat_err(self, error: LlmError) -> Self {
        self.inner
            .lock()
            .expect("scripted mutex")
            .chats
            .push(Err(error));
        self
    }

    pub fn with_embeddings(self, response: EmbeddingResponse) -> Self {
        self.inner.lock().expect("scripted mutex").embeddings = Some(response);
        self
    }

    pub fn chat_calls(&self) -> u32 {
        self.inner.lock().expect("scripted mutex").chat_calls
    }
}

#[async_trait]
impl LlmProvider for ScriptedProvider {
    async fn chat(&self, _request: ChatRequest) -> binder_llm::Result<ChatResponse> {
        let mut inner = self.inner.lock().expect("scripted mutex");
        inner.chat_calls += 1;
        if inner.chats.is_empty() {
            return Err(LlmError::invalid(
                "ScriptedProvider has no chat responses left",
            ));
        }
        inner.chats.remove(0)
    }

    async fn stream(&self, _request: ChatRequest) -> binder_llm::Result<ChatStream> {
        Ok(ChatStream::empty())
    }

    async fn embeddings(
        &self,
        _request: EmbeddingRequest,
    ) -> binder_llm::Result<EmbeddingResponse> {
        self.inner
            .lock()
            .expect("scripted mutex")
            .embeddings
            .clone()
            .ok_or_else(|| LlmError::invalid("ScriptedProvider has no embeddings"))
    }

    async fn list_models(&self) -> binder_llm::Result<Vec<ModelInfo>> {
        Ok(self.inner.lock().expect("scripted mutex").models.clone())
    }
}

/// Router that always returns the same target (tests).
#[derive(Debug, Clone)]
pub struct MockRouter {
    pub target: crate::routing::ModelTarget,
}

impl MockRouter {
    pub fn new(target: crate::routing::ModelTarget) -> Self {
        Self { target }
    }
}
