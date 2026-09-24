use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::de::DeserializeOwned;

use crate::builder::AiClientBuilder;
use crate::cache::{AiCache, NS_EMBEDDING, NS_PROMPT, NS_RESPONSE};
use crate::config::{AiConfig, BudgetLedger};
use crate::context::ContextBuilder;
use crate::conversation::{Conversation, ConversationStore};
use crate::error::{AiError, Result};
use crate::models::{Availability, ExecutionMode, ModelCatalog, ModelId, ModelRequirements};
use crate::prompt::manager::PromptManager;
use crate::prompt::PromptRef;
use crate::providers::LlmFactory;
use crate::request::AiRequest;
use crate::response::{AiResponse, AiUsage, ExecutionTrace};
use crate::routing::{ModelTarget, Router};
use crate::session::{SessionId, UserId, WorkflowId};
use crate::task::{Task, TaskKind};
use crate::telemetry::{estimate_cost_for, trace_err, trace_ok, trace_start, AiMetrics};
use crate::tools::{ToolExecutor, ToolRegistry};
use binder_llm::{
    ChatImage, ChatMessage, ChatRequest, ChatResponse, EmbeddingInput, EmbeddingRequest,
    EmbeddingResponse, FinishReason, Provider, Usage,
};

/// Binder applications construct this and nothing else for AI work.
#[derive(Clone)]
pub struct AiClient {
    pub(crate) config: AiConfig,
    pub(crate) router: Router,
    pub(crate) prompts: PromptManager,
    pub(crate) tools: ToolRegistry,
    pub(crate) executor: Arc<dyn ToolExecutor>,
    pub(crate) store: Arc<dyn ConversationStore>,
    pub(crate) cache: Option<Arc<dyn AiCache>>,
    pub(crate) factory: Arc<dyn LlmFactory>,
    pub(crate) available_providers: Option<Vec<Provider>>,
    pub(crate) metrics: Arc<Mutex<AiMetrics>>,
    pub(crate) budgets: Arc<Mutex<BudgetLedger>>,
}

impl AiClient {
    pub fn builder() -> AiClientBuilder {
        AiClientBuilder::new()
    }

    pub fn config(&self) -> &AiConfig {
        &self.config
    }

    pub fn metrics(&self) -> AiMetrics {
        self.metrics.lock().map(|m| m.clone()).unwrap_or_default()
    }

    pub fn catalog(&self) -> ModelCatalog {
        self.router.catalog()
    }

    pub async fn chat(&self, request: impl Into<AiRequest>) -> Result<AiResponse> {
        self.dispatch(request.into()).await
    }

    pub async fn chat_json<T: DeserializeOwned>(&self, mut request: AiRequest) -> Result<T> {
        request.json_mode = true;
        let response = self.dispatch(request).await?;
        parse_assistant_json(&response.content)
    }

    pub async fn execute(&self, request: impl Into<AiRequest>) -> Result<AiResponse> {
        self.dispatch(request.into()).await
    }

    pub async fn summarize(&self, document: impl Into<String>) -> Result<AiResponse> {
        self.execute(Task::Summarization {
            document: document.into(),
        })
        .await
    }

    pub async fn classify(&self, text: impl Into<String>) -> Result<AiResponse> {
        self.execute(Task::Classification {
            text: text.into(),
            labels: Vec::new(),
        })
        .await
    }

    pub async fn extract(&self, text: impl Into<String>) -> Result<AiResponse> {
        self.execute(Task::Extraction {
            text: text.into(),
            schema_hint: None,
        })
        .await
    }

    pub async fn complete(&self, prompt: impl Into<String>) -> Result<AiResponse> {
        let mut request = AiRequest::new(TaskKind::Conversation, prompt.into());
        request.prompt = Some(PromptRef::latest("complete"));
        request.json_mode = false;
        request.use_tools = false;
        self.dispatch(request).await
    }

    pub async fn plan(&self, goal: impl Into<String>) -> Result<AiResponse> {
        self.execute(Task::Planning { goal: goal.into() }).await
    }

    pub async fn embeddings(&self, input: impl Into<EmbeddingInput>) -> Result<EmbeddingResponse> {
        let input = input.into();
        let cache_key = format!("{:?}", input.as_texts());
        if let Some(cache) = &self.cache {
            if let Some(hit) = cache.get(NS_EMBEDDING, &cache_key) {
                if let Ok(parsed) = serde_json::from_str::<Vec<Vec<f32>>>(&hit) {
                    return Ok(EmbeddingResponse {
                        embeddings: parsed,
                        model: "cache".into(),
                        provider: Provider::Ollama,
                        usage: Usage::default(),
                        latency: std::time::Duration::ZERO,
                        retries: 0,
                    });
                }
            }
        }
        let request = AiRequest::new(TaskKind::Embedding, cache_key.clone());
        let mut requirements = request.requirements(&self.router.catalog());
        if request.policy.is_none() {
            requirements.execution_policy = self.config.routing_policy;
        }
        self.apply_config_preferences(&mut requirements, TaskKind::Embedding);
        let chain = self.router.route(TaskKind::Embedding, &requirements)?;
        let mut last_err = AiError::routing("embedding failed");
        for target in chain.iter() {
            match self.embed_on(target, &input).await {
                Ok(resp) => {
                    if let Some(cache) = &self.cache {
                        if let Ok(json) = serde_json::to_string(&resp.embeddings) {
                            cache.put(NS_EMBEDDING, &cache_key, json);
                        }
                    }
                    return Ok(resp);
                }
                Err(err) if err.is_fallback_candidate() => last_err = err,
                Err(err) => return Err(err),
            }
        }
        let _ = request;
        Err(last_err)
    }

    async fn embed_on(
        &self,
        target: &ModelTarget,
        input: &EmbeddingInput,
    ) -> Result<EmbeddingResponse> {
        let client = self.factory.client_for(target)?;
        Ok(client
            .embeddings(EmbeddingRequest::new(input.clone()).with_model(&target.model))
            .await?)
    }

    fn question_message(request: &AiRequest) -> ChatMessage {
        if request.images.is_empty() {
            return ChatMessage::user(&request.input);
        }
        let images = request
            .images
            .iter()
            .map(|image| ChatImage {
                media_type: image.media_type.clone(),
                bytes: image.bytes.clone(),
            })
            .collect();
        ChatMessage::user_with_images(&request.input, images)
    }

    async fn dispatch(&self, request: AiRequest) -> Result<AiResponse> {
        let started = Instant::now();
        let prompt_ref = request
            .prompt
            .clone()
            .unwrap_or_else(|| PromptRef::latest(request.task.prompt_name()));
        let variables = vec![("input".to_string(), request.input.clone())];
        let (prompt, rendered) = self.prompts.resolve(&prompt_ref, &variables)?;

        if let Some(cache) = &self.cache {
            cache.put(NS_PROMPT, &prompt.id(), rendered.clone());
            let key = format!("{}:{}:{}", prompt.id(), request.task, request.input.len());
            if let Some(hit) = cache.get(NS_RESPONSE, &key) {
                return Ok(cached_response(
                    request.task,
                    hit,
                    prompt.name.clone(),
                    prompt.version,
                ));
            }
        }

        self.budgets
            .lock()
            .map_err(|_| AiError::budget("budget ledger poisoned"))?
            .check_request(&self.config.token_budget, request.input.len() as u32 / 4)?;

        let mut requirements = request.requirements(&self.router.catalog());
        if request.policy.is_none() {
            requirements.execution_policy = self.config.routing_policy;
        }
        self.apply_config_preferences(&mut requirements, request.task);
        let policy = requirements.execution_policy;
        let chain = self.router.route(request.task, &requirements)?;

        let primary = chain.primary();
        trace_start(
            request.task,
            policy.as_str(),
            primary.id.as_str(),
            primary.provider,
            ExecutionMode::from_provider(primary.provider),
        );

        let history = self.load_history(&request)?;
        let context = ContextBuilder::new()
            .extend(request.context.clone())
            .build();
        let mut messages = Vec::new();
        messages.push(ChatMessage::system(&rendered));
        messages.extend(history);
        if !context.is_empty() {
            messages.push(ChatMessage::user(format!("Context:\n{context}")));
        }
        messages.push(Self::question_message(&request));

        let mut last_err = AiError::routing("no provider succeeded");
        let mut fallbacks = 0;
        for target in chain.iter() {
            match self
                .run_target(&request, target, messages.clone(), prompt.version)
                .await
            {
                Ok(mut response) => {
                    response.fallbacks = fallbacks;
                    response.prompt_name = prompt.name.clone();
                    response.prompt_version = prompt.version;
                    response.execution.policy = policy.as_str().to_string();
                    self.persist_conversation(&request, &response.content)?;
                    self.record_success(&request, &response)?;
                    if let Some(cache) = &self.cache {
                        let key =
                            format!("{}:{}:{}", prompt.id(), request.task, request.input.len());
                        cache.put(NS_RESPONSE, &key, response.content.clone());
                    }
                    trace_ok(
                        request.task,
                        response.execution.provider,
                        &response.execution.model_id,
                        response.execution.execution_mode,
                        &response.execution.policy,
                        started.elapsed().as_millis(),
                        response.fallbacks,
                        response.tool_rounds,
                        response.usage,
                        response.cost.usd,
                    );
                    return Ok(response);
                }
                Err(err) if err.is_context_too_large() => {
                    let mut shrunk = messages.clone();
                    shrink_history(&mut shrunk);
                    match self
                        .run_target(&request, target, shrunk, prompt.version)
                        .await
                    {
                        Ok(mut response) => {
                            response.fallbacks = fallbacks;
                            response.prompt_name = prompt.name.clone();
                            response.prompt_version = prompt.version;
                            self.record_success(&request, &response)?;
                            return Ok(response);
                        }
                        Err(retry_err) => {
                            last_err = retry_err;
                            fallbacks += 1;
                        }
                    }
                }
                Err(err) if err.is_local_model_missing() => {
                    tracing::info!(
                        selected_model = %target.id,
                        fallback_count = fallbacks + 1,
                        reason = "model_not_found",
                        "routing.fallback"
                    );
                    self.router.mark_unavailable(&target.id);
                    last_err = AiError::ModelUnavailable {
                        message: target.id.to_string(),
                    };
                    fallbacks += 1;
                }
                Err(err) if err.is_fallback_candidate() => {
                    last_err = err;
                    fallbacks += 1;
                }
                Err(err) => {
                    trace_err(request.task, err.kind());
                    self.record_failure();
                    return Err(err);
                }
            }
        }
        trace_err(request.task, last_err.kind());
        self.record_failure();
        Err(last_err)
    }

    async fn run_target(
        &self,
        request: &AiRequest,
        target: &ModelTarget,
        mut messages: Vec<ChatMessage>,
        _prompt_version: u32,
    ) -> Result<AiResponse> {
        let mode = ExecutionMode::from_provider(target.provider);
        if request
            .policy
            .unwrap_or(self.config.routing_policy)
            .is_private()
            && mode == ExecutionMode::Cloud
        {
            return Err(AiError::LocalExecutionRequired);
        }
        let client = self.factory.client_for(target)?;
        let mut tool_rounds = 0;
        let mut usage = Usage::default();
        let mut last: Option<ChatResponse> = None;
        let json_attempts = if request.json_mode {
            self.config.semantic_retries.max(1)
        } else {
            1
        };

        for attempt in 0..json_attempts {
            loop {
                let mut chat_req = ChatRequest::new(messages.clone())
                    .with_model(&target.model)
                    .with_json_mode(request.json_mode);
                chat_req.json_schema = request.json_schema.clone();
                if request.use_tools && !self.tools.is_empty() {
                    chat_req.tools = self.tools.definitions().to_vec();
                }
                let response = client.chat(chat_req).await?;
                usage.prompt_tokens = usage
                    .prompt_tokens
                    .saturating_add(response.usage.prompt_tokens);
                usage.completion_tokens = usage
                    .completion_tokens
                    .saturating_add(response.usage.completion_tokens);
                usage.total_tokens = usage.prompt_tokens.saturating_add(usage.completion_tokens);

                if response.finish_reason == FinishReason::ToolCalls
                    || !response.message.tool_calls().is_empty()
                {
                    if tool_rounds >= self.config.max_tool_rounds {
                        return Err(AiError::tool("max tool rounds exceeded"));
                    }
                    messages.push(response.message.clone());
                    for call in response.message.tool_calls() {
                        let result = match self.executor.execute(call).await {
                            Ok(result) => result,
                            Err(err) => binder_llm::ToolResult {
                                tool_call_id: call.id.clone(),
                                content: format!("tool failure: {}", err),
                                is_error: true,
                            },
                        };
                        messages.push(result.into_message());
                        tool_rounds += 1;
                    }
                    continue;
                }

                last = Some(response);
                break;
            }

            let Some(response) = last.as_ref() else {
                return Err(AiError::routing("empty model response"));
            };
            if !request.json_mode {
                break;
            }
            let content = response.message.content();
            if json_value_from_assistant(content).is_some() {
                break;
            }
            if attempt + 1 >= json_attempts {
                return Err(AiError::json(
                    "assistant content is not valid JSON",
                    json_attempts,
                ));
            }
            messages.push(response.message.clone());
            messages.push(ChatMessage::user(
                "Your previous reply was not valid JSON. Reply with JSON only.",
            ));
        }

        let response = last.ok_or_else(|| AiError::routing("empty model response"))?;
        let pricing = self.router.catalog().get(&target.id).map(|m| m.pricing);
        let cost = estimate_cost_for(target.provider, usage, pricing);
        let mode = ExecutionMode::from_provider(target.provider);
        Ok(AiResponse {
            content: response.message.content().to_string(),
            task: request.task,
            usage,
            cost,
            latency: response.latency,
            finish_reason: response.finish_reason,
            fallbacks: 0,
            tool_rounds,
            prompt_name: String::new(),
            prompt_version: 0,
            execution: ExecutionTrace {
                provider: target.provider,
                model: target.model.clone(),
                model_id: target.id.to_string(),
                policy: String::new(),
                execution_mode: mode,
            },
            ai_usage: AiUsage {
                provider: target.provider,
                model: target.id.clone(),
                prompt_tokens: Some(u64::from(usage.prompt_tokens)),
                completion_tokens: Some(u64::from(usage.completion_tokens)),
                estimated_cost: Some(cost.usd),
                execution_mode: mode,
                latency: response.latency,
            },
        })
    }

    fn apply_config_preferences(&self, req: &mut ModelRequirements, task: TaskKind) {
        if let Some(model) = self.config.preferred_model(task) {
            req.preferred_models.push(ModelId::from(model));
        }
    }

    /// Refresh local Ollama availability via `binder-llm` `list_models`. No HTTP here.
    pub async fn refresh_catalog(&self) -> Result<()> {
        if self
            .available_providers
            .as_ref()
            .is_some_and(|providers| !providers.contains(&Provider::Ollama))
        {
            return Ok(());
        }
        let probe = ModelTarget {
            provider: Provider::Ollama,
            model: "probe".into(),
            id: ModelId::new(Provider::Ollama, "probe"),
        };
        let client = self.factory.client_for(&probe)?;
        match client.list_models().await {
            Ok(infos) => {
                let installed: Vec<String> = infos.iter().map(|info| info.id.clone()).collect();
                tracing::info!(installed = installed.join(","), "catalog.discovered");
                self.router.ingest_discovered(Provider::Ollama, &infos);
                let catalog = self.router.catalog();
                tracing::info!(
                    available = catalog.ids_with(Availability::Available).join(","),
                    unavailable = catalog.ids_with(Availability::Unavailable).join(","),
                    "catalog.effective"
                );
                Ok(())
            }
            Err(err) => {
                tracing::warn!(error_kind = err.kind(), "catalog.discovery_failed");
                self.router
                    .mark_providers(&[Provider::Ollama], Availability::Unavailable);
                Err(err.into())
            }
        }
    }

    fn load_history(&self, request: &AiRequest) -> Result<Vec<ChatMessage>> {
        let Some(id) = &request.conversation_id else {
            return Ok(Vec::new());
        };
        Ok(self.store.get(id)?.map(|c| c.messages).unwrap_or_default())
    }

    fn persist_conversation(&self, request: &AiRequest, assistant: &str) -> Result<()> {
        let Some(id) = &request.conversation_id else {
            return Ok(());
        };
        let mut conv = self
            .store
            .get(id)?
            .unwrap_or_else(|| Conversation::new(id.clone()));
        conv.session_id = request.session_id.clone();
        conv.messages.push(ChatMessage::user(&request.input));
        conv.messages.push(ChatMessage::assistant(assistant));
        self.store.upsert(conv)
    }

    fn record_success(&self, request: &AiRequest, response: &AiResponse) -> Result<()> {
        let tokens = response.usage.total_tokens;
        let session = request.session_id.as_ref().map(SessionId::as_str);
        {
            let mut ledger = self
                .budgets
                .lock()
                .map_err(|_| AiError::budget("budget ledger poisoned"))?;
            ledger.check_scope(
                &self.config.token_budget,
                session,
                None::<&UserId>,
                None::<&WorkflowId>,
                tokens,
            )?;
            if let Some(limit) = self.config.cost_limit_usd {
                if ledger.spent_usd + response.cost.usd > limit {
                    return Err(AiError::budget("cost limit exceeded"));
                }
            }
            ledger.record(session, None, None, tokens, response.cost.usd);
        }
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.record_success(
                response.latency,
                response.fallbacks,
                response.tool_rounds,
                response.cost,
            );
        }
        Ok(())
    }

    fn record_failure(&self) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.record_failure();
        }
    }
}

/// Deserialize assistant content as JSON. Accepts a raw object/array, or the
/// same JSON wrapped in markdown fences / surrounding prose.
pub(crate) fn parse_assistant_json<T: DeserializeOwned>(content: &str) -> Result<T> {
    let payload = json_payload(content)
        .ok_or_else(|| AiError::json("assistant content is not valid JSON".to_string(), 1))?;
    serde_json::from_str(payload)
        .map_err(|e| AiError::json(format!("assistant content is not valid JSON: {e}"), 1))
}

pub(crate) fn json_value_from_assistant(content: &str) -> Option<serde_json::Value> {
    let payload = json_payload(content)?;
    serde_json::from_str(payload).ok()
}

fn json_payload(content: &str) -> Option<&str> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    if (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    {
        return Some(trimmed);
    }
    if let Some(rest) = trimmed
        .split_once("```json")
        .or_else(|| trimmed.split_once("```JSON"))
        .map(|(_, rest)| rest)
    {
        let rest = rest.trim_start_matches('\n');
        if let Some((inner, _)) = rest.split_once("```") {
            let inner = inner.trim();
            if !inner.is_empty() {
                return Some(inner);
            }
        }
    }
    if let Some(fence) = trimmed.find("```") {
        let rest = trimmed[fence + 3..].trim_start();
        let rest = rest
            .strip_prefix("json")
            .or_else(|| rest.strip_prefix("JSON"))
            .unwrap_or(rest)
            .trim_start();
        if let Some((inner, _)) = rest.split_once("```") {
            let inner = inner.trim();
            if inner.starts_with('{') || inner.starts_with('[') {
                return Some(inner);
            }
        }
    }
    let start = trimmed.find('{').or_else(|| trimmed.find('['))?;
    let end = if trimmed.as_bytes().get(start) == Some(&b'{') {
        trimmed.rfind('}')
    } else {
        trimmed.rfind(']')
    }?;
    if end > start {
        Some(&trimmed[start..=end])
    } else {
        None
    }
}

fn shrink_history(messages: &mut Vec<ChatMessage>) {
    let mut idx = None;
    for (i, msg) in messages.iter().enumerate() {
        if !matches!(msg, ChatMessage::System { .. }) {
            idx = Some(i);
            break;
        }
    }
    if let Some(i) = idx {
        if messages.len() > i + 1 {
            messages.remove(i);
        }
    }
}

fn cached_response(task: TaskKind, content: String, name: String, version: u32) -> AiResponse {
    AiResponse {
        content,
        task,
        usage: Usage::default(),
        cost: crate::telemetry::CostEstimate::default(),
        latency: std::time::Duration::ZERO,
        finish_reason: FinishReason::Stop,
        fallbacks: 0,
        tool_rounds: 0,
        prompt_name: name,
        prompt_version: version,
        execution: ExecutionTrace {
            provider: Provider::Ollama,
            model: "cache".into(),
            model_id: "cache".into(),
            policy: "cache".into(),
            execution_mode: ExecutionMode::Local,
        },
        ai_usage: AiUsage::default(),
    }
}

#[cfg(test)]
mod json_payload_tests {
    use super::{json_payload, parse_assistant_json};
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Label {
        label: String,
    }

    #[test]
    fn parses_fenced_json() {
        let raw = "Sure.\n```json\n{\"label\":\"ok\"}\n```\n";
        let parsed: Label = parse_assistant_json(raw).unwrap();
        assert_eq!(parsed.label, "ok");
    }

    #[test]
    fn parses_prose_wrapped_object() {
        let raw = "Here you go: {\"label\":\"ok\"} thanks.";
        assert_eq!(json_payload(raw), Some(r#"{"label":"ok"}"#));
        let parsed: Label = parse_assistant_json(raw).unwrap();
        assert_eq!(parsed.label, "ok");
    }
}

#[cfg(test)]
mod image_tests {
    use super::AiClient;
    use crate::request::AiRequest;
    use crate::task::TaskKind;

    #[test]
    fn question_message_keeps_image_bytes() {
        let png = vec![0x89, 0x50, 0x4e, 0x47];
        let request = AiRequest::new(TaskKind::Reasoning, "Is this a company logo?")
            .with_image("image/png", png.clone());
        let message = AiClient::question_message(&request);
        assert_eq!(message.images().len(), 1);
        assert_eq!(message.images()[0].bytes, png);
        assert_eq!(message.images()[0].media_type, "image/png");
    }

    /// Set `BINDER_LOGO_VISION=1`. Uses local `ministral-3:3b` (vision).
    #[tokio::test]
    async fn local_vision_model_judges_a_logo_mark() {
        if std::env::var("BINDER_LOGO_VISION").ok().as_deref() != Some("1") {
            return;
        }
        let png = mark_png();
        let mut request = AiRequest::new(
            TaskKind::Reasoning,
            "Is this image a company logo? Answer yes or no, then one short reason.",
        )
        .with_policy(crate::policy::ExecutionPolicy::LocalOnly)
        .with_image("image/png", png);
        request.override_provider = Some(binder_llm::Provider::Ollama);
        request.override_model = Some("ministral-3:3b".into());
        let client = AiClient::builder()
            .routing_policy(crate::policy::ExecutionPolicy::LocalOnly)
            .preferred_provider(binder_llm::Provider::Ollama)
            .build();
        let reply = client.chat(request).await.expect("vision reply");
        eprintln!(
            "logo vision model={} finish={:?} text={}",
            reply.execution.model, reply.finish_reason, reply.content
        );
        let text = reply.content.to_ascii_lowercase();
        assert!(
            text.trim_start().starts_with("no"),
            "reply was {}",
            reply.content
        );
    }

    fn mark_png() -> Vec<u8> {
        const W: usize = 64;
        const H: usize = 64;
        let mut raw = Vec::with_capacity((1 + W * 3) * H);
        for y in 0..H {
            raw.push(0);
            for x in 0..W {
                let dx = x as i32 - 32;
                let dy = y as i32 - 32;
                let inside = dx * dx + dy * dy < 18 * 18;
                if inside {
                    raw.extend_from_slice(&[20, 70, 160]);
                } else {
                    raw.extend_from_slice(&[255, 255, 255]);
                }
            }
        }
        fn chunk(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
            let mut out = (data.len() as u32).to_be_bytes().to_vec();
            out.extend_from_slice(tag);
            out.extend_from_slice(data);
            let mut crc_bytes = tag.to_vec();
            crc_bytes.extend_from_slice(data);
            let crc = crc32(&crc_bytes);
            out.extend_from_slice(&crc.to_be_bytes());
            out
        }
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&(W as u32).to_be_bytes());
        ihdr.extend_from_slice(&(H as u32).to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend(chunk(b"IHDR", &ihdr));
        png.extend(chunk(b"IDAT", &deflate_stored(&raw)));
        png.extend(chunk(b"IEND", &[]));
        png
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xffff_ffffu32;
        for byte in data {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xedb8_8320 & mask);
            }
        }
        !crc
    }

    fn deflate_stored(data: &[u8]) -> Vec<u8> {
        let mut out = vec![0x78, 0x01];
        let mut rest = data;
        while !rest.is_empty() {
            let take = rest.len().min(65535);
            let (block, next) = rest.split_at(take);
            rest = next;
            let last = rest.is_empty();
            out.push(if last { 1 } else { 0 });
            let len = take as u16;
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&(!len).to_le_bytes());
            out.extend_from_slice(block);
        }
        let sum = adler32(data);
        out.extend_from_slice(&sum.to_be_bytes());
        out
    }

    fn adler32(data: &[u8]) -> u32 {
        let mut a = 1u32;
        let mut b = 0u32;
        for byte in data {
            a = (a + u32::from(*byte)) % 65521;
            b = (b + a) % 65521;
        }
        (b << 16) | a
    }
}
