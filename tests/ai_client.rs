use std::sync::Arc;

use binder_ai::{
    AiClient, AiRequest, ConversationStore, MockToolExecutor, RoutingPolicy, ScriptedProvider,
    Task, TaskKind, TokenBudget, ToolKind,
};
use binder_llm::{
    ChatMessage, ChatResponse, EmbeddingResponse, FinishReason, LlmClient, LlmError, MockProvider,
    ModelInfo, Provider, ToolCall, ToolResult, Usage,
};

fn llm(provider: impl binder_llm::LlmProvider + 'static, kind: Provider, model: &str) -> LlmClient {
    let mut builder = LlmClient::builder().provider(kind).model(model);
    if kind.requires_api_key() {
        builder = builder.api_key("test-key");
    }
    let config = builder.build_config().expect("config");
    LlmClient::from_provider(Arc::new(provider), config)
}

fn assistant(text: &str) -> ChatResponse {
    ChatResponse::new(Provider::Ollama, "mock", ChatMessage::assistant(text))
}

#[tokio::test]
async fn summarize_uses_injected_llm() {
    let ai = AiClient::builder()
        .llm(llm(
            MockProvider::new().with_chat(assistant("short")),
            Provider::Ollama,
            "mock",
        ))
        .build();
    let out = ai.summarize("a very long document").await.unwrap();
    assert_eq!(out.content, "short");
    assert_eq!(out.task, TaskKind::Summarization);
    assert_eq!(out.prompt_name, "summarization");
}

#[tokio::test]
async fn chat_json_parses_fenced_struct() {
    #[derive(serde::Deserialize, Debug, PartialEq)]
    struct Label {
        label: String,
    }

    let ai = AiClient::builder()
        .llm(llm(
            MockProvider::new().with_chat(assistant("```json\n{\"label\":\"ok\"}\n```")),
            Provider::Ollama,
            "mock",
        ))
        .build();
    let parsed: Label = ai
        .chat_json(AiRequest::new(TaskKind::Classification, "text"))
        .await
        .unwrap();
    assert_eq!(parsed.label, "ok");
}

#[tokio::test]
async fn chat_json_parses_struct() {
    #[derive(serde::Deserialize, Debug, PartialEq)]
    struct Label {
        label: String,
    }

    let ai = AiClient::builder()
        .llm(llm(
            MockProvider::new().with_chat(assistant(r#"{"label":"ok"}"#)),
            Provider::Ollama,
            "mock",
        ))
        .build();
    let parsed: Label = ai
        .chat_json(AiRequest::new(TaskKind::Classification, "text"))
        .await
        .unwrap();
    assert_eq!(parsed.label, "ok");
}

#[tokio::test]
async fn json_retries_then_succeeds() {
    #[derive(serde::Deserialize, Debug, PartialEq)]
    struct Out {
        ok: bool,
    }

    let scripted = ScriptedProvider::new()
        .push_chat(assistant("not-json"))
        .push_chat(assistant(r#"{"ok":true}"#));
    let ai = AiClient::builder()
        .semantic_retries(2)
        .llm(llm(scripted, Provider::Ollama, "qwen3-coder:30b"))
        .build();
    let parsed: Out = ai
        .chat_json(AiRequest::new(TaskKind::Extraction, "blob"))
        .await
        .unwrap();
    assert!(parsed.ok);
}

#[tokio::test]
async fn local_only_routes_to_ollama() {
    let ai = AiClient::builder()
        .routing_policy(RoutingPolicy::LocalOnly)
        .available_providers(vec![Provider::Ollama, Provider::Anthropic])
        .llm(llm(
            MockProvider::new().with_chat(assistant(r#"{"steps":["ship"]}"#)),
            Provider::Ollama,
            "qwen3-coder:30b",
        ))
        .build();
    let out = ai
        .execute(Task::Planning {
            goal: "ship binder-ai".into(),
        })
        .await
        .unwrap();
    assert_eq!(out.execution.provider, Provider::Ollama);
}

#[tokio::test]
async fn fallback_when_primary_unavailable() {
    let failing = llm(
        ScriptedProvider::new().push_chat_err(LlmError::connection("claude down")),
        Provider::Anthropic,
        "claude-sonnet-4-5",
    );
    let ok = llm(
        MockProvider::new().with_chat(ChatResponse::new(
            Provider::Ollama,
            "qwen3-coder:30b",
            ChatMessage::assistant(r#"{"steps":["plan"]}"#),
        )),
        Provider::Ollama,
        "qwen3-coder:30b",
    );
    let factory = binder_ai::StaticLlmFactory::new(ok.clone())
        .with_provider(Provider::Anthropic, failing)
        .with_provider(Provider::Ollama, ok);
    let ai = AiClient::builder()
        .routing_policy(RoutingPolicy::BestQuality)
        .available_providers(vec![Provider::Anthropic, Provider::Ollama])
        .llm_factory(Arc::new(factory))
        .build();
    let out = ai.plan("do the thing").await.unwrap();
    assert_eq!(out.content, r#"{"steps":["plan"]}"#);
    assert_eq!(out.execution.provider, Provider::Ollama);
    assert!(out.fallbacks >= 1);
}

#[tokio::test]
async fn tool_loop_executes_registry() {
    let scripted = ScriptedProvider::new()
        .push_chat({
            let mut resp = ChatResponse::new(
                Provider::Ollama,
                "mock",
                ChatMessage::assistant_with_tools(
                    "",
                    vec![ToolCall::new("1", ToolKind::GitStatus.as_str(), "{}")],
                ),
            );
            resp.finish_reason = FinishReason::ToolCalls;
            resp
        })
        .push_chat(assistant("clean tree"));
    let executor = Arc::new(
        MockToolExecutor::new()
            .with_result(ToolKind::GitStatus.as_str(), ToolResult::new("1", "ok")),
    );
    let ai = AiClient::builder()
        .llm(llm(scripted, Provider::Ollama, "qwen3-coder:30b"))
        .tool_executor(executor.clone())
        .build();
    let mut request = AiRequest::from_task(Task::ArchitectureReview {
        repository: "dpkimball/binder-ai".into(),
        pull_request: None,
    });
    request.use_tools = true;
    let out = ai.chat(request).await.unwrap();
    assert_eq!(out.content, "clean tree");
    assert_eq!(out.tool_rounds, 1);
    assert_eq!(executor.calls().len(), 1);
}

#[tokio::test]
async fn budget_rejects_large_request() {
    let ai = AiClient::builder()
        .token_budget(TokenBudget {
            per_request: Some(1),
            ..TokenBudget::unlimited()
        })
        .llm(llm(
            MockProvider::new().with_chat(assistant("x")),
            Provider::Ollama,
            "mock",
        ))
        .build();
    let err = ai
        .summarize("this is definitely more than four characters")
        .await
        .unwrap_err();
    assert_eq!(err.kind(), "budget");
}

#[tokio::test]
async fn embeddings_use_mock() {
    let ai = AiClient::builder()
        .llm(llm(
            MockProvider::new().with_embeddings(EmbeddingResponse {
                embeddings: vec![vec![0.1, 0.2]],
                model: "nomic-embed-text".into(),
                provider: Provider::Ollama,
                usage: Usage::default(),
                latency: std::time::Duration::ZERO,
                retries: 0,
            }),
            Provider::Ollama,
            "nomic-embed-text",
        ))
        .routing_policy(RoutingPolicy::LocalOnly)
        .build();
    let out = ai.embeddings("hello").await.unwrap();
    assert_eq!(out.embeddings[0].len(), 2);
}

#[tokio::test]
async fn config_debug_redacts_keys() {
    let ai = AiClient::builder()
        .provider_settings(
            Provider::Openai,
            binder_ai::ProviderSettings {
                api_key: Some("sk-secret-value".into()),
                api_key_env: None,
                base_url: None,
                default_model: None,
            },
        )
        .llm(llm(
            MockProvider::new().with_chat(assistant("x")),
            Provider::Ollama,
            "mock",
        ))
        .build();
    let rendered = format!("{:?}", ai.config());
    assert!(!rendered.contains("sk-secret-value"));
    assert!(rendered.contains("[redacted]"));
}

#[tokio::test]
async fn conversation_persists_turns() {
    let store = Arc::new(binder_ai::InMemoryConversationStore::new());
    let id = binder_ai::ConversationId::new("c1");
    let ai = AiClient::builder()
        .conversation_store(store.clone())
        .llm(llm(
            MockProvider::new().with_chat(assistant("pong")),
            Provider::Ollama,
            "mock",
        ))
        .build();
    let request = AiRequest::new(TaskKind::Conversation, "ping").with_conversation(id.clone());
    ai.chat(request).await.unwrap();
    let conv = store.get(&id).unwrap().unwrap();
    assert_eq!(conv.messages.len(), 2);
}

#[tokio::test]
async fn missing_local_model_falls_back() {
    let scripted = ScriptedProvider::new()
        .with_models(vec![
            ModelInfo::new(Provider::Ollama, "deepseek-r1:32b"),
            ModelInfo::new(Provider::Ollama, "qwen3-coder:30b"),
        ])
        .push_chat_err(LlmError::invalid("model 'deepseek-r1:32b' not found"))
        .push_chat(assistant(r#"{"steps":["ok"]}"#));
    let ai = AiClient::builder()
        .routing_policy(RoutingPolicy::LocalOnly)
        .available_providers(vec![Provider::Ollama])
        .llm(llm(scripted, Provider::Ollama, "probe"))
        .build();
    let out = ai
        .plan("do the thing")
        .await
        .expect("fallback should succeed");
    assert!(out.fallbacks >= 1);
    assert_eq!(out.content, r#"{"steps":["ok"]}"#);
    assert_eq!(out.execution.provider, Provider::Ollama);
}

#[tokio::test]
async fn empty_discovered_catalog_is_typed_error() {
    let scripted = ScriptedProvider::new().with_models(Vec::new());
    let ai = AiClient::builder()
        .routing_policy(RoutingPolicy::LocalOnly)
        .available_providers(vec![Provider::Ollama])
        .llm(llm(scripted, Provider::Ollama, "probe"))
        .build();
    let err = ai
        .plan("do the thing")
        .await
        .expect_err("no installed local models");
    assert_eq!(err.kind(), "no_eligible_model");
}

#[tokio::test]
async fn unknown_installed_model_is_routable() {
    let scripted = ScriptedProvider::new()
        .with_models(vec![ModelInfo::new(Provider::Ollama, "mystery-local")])
        .push_chat(assistant("hello"));
    let ai = AiClient::builder()
        .routing_policy(RoutingPolicy::LocalOnly)
        .available_providers(vec![Provider::Ollama])
        .llm(llm(scripted, Provider::Ollama, "probe"))
        .build();
    let catalog = ai.catalog();
    assert_eq!(
        catalog
            .get(&binder_ai::ModelId::new(Provider::Ollama, "mystery-local"))
            .unwrap()
            .availability,
        binder_ai::Availability::Available
    );
    let out = ai.complete("hi").await.unwrap();
    assert_eq!(out.content, "hello");
    assert_eq!(out.execution.model, "mystery-local");
}
