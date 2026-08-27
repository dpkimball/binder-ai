use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::error::{AiError, Result};
use crate::session::SessionId;
use binder_llm::ChatMessage;

/// Persistent conversation identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConversationId(pub String);

impl ConversationId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ConversationId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for ConversationId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Display for ConversationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Conversation record. Storage is abstract; this crate ships in-memory only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conversation {
    pub id: ConversationId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl Conversation {
    pub fn new(id: ConversationId) -> Self {
        Self {
            id,
            session_id: None,
            messages: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Abstract conversation persistence.
pub trait ConversationStore: Send + Sync {
    fn get(&self, id: &ConversationId) -> Result<Option<Conversation>>;
    fn upsert(&self, conversation: Conversation) -> Result<()>;
}

/// Process-local store for tests and single-process apps.
#[derive(Debug, Default)]
pub struct InMemoryConversationStore {
    inner: Mutex<HashMap<String, Conversation>>,
}

impl InMemoryConversationStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ConversationStore for InMemoryConversationStore {
    fn get(&self, id: &ConversationId) -> Result<Option<Conversation>> {
        let map = self
            .inner
            .lock()
            .map_err(|_| AiError::conversation("conversation store poisoned"))?;
        Ok(map.get(id.as_str()).cloned())
    }

    fn upsert(&self, conversation: Conversation) -> Result<()> {
        let mut map = self
            .inner
            .lock()
            .map_err(|_| AiError::conversation("conversation store poisoned"))?;
        map.insert(conversation.id.as_str().to_string(), conversation);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let store = InMemoryConversationStore::new();
        let id = ConversationId::new("c1");
        let mut conv = Conversation::new(id.clone());
        conv.messages.push(ChatMessage::user("hi"));
        store.upsert(conv).unwrap();
        assert_eq!(store.get(&id).unwrap().unwrap().messages.len(), 1);
    }
}
