use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::de::DeserializeOwned;

use crate::builder::AiClientBuilder;
use crate::cache::{AiCache, NS_EMBEDDING, NS_PROMPT, NS_RESPONSE};
use crate::config::{AiConfig, BudgetLedger};
use crate::context::ContextBuilder;
use crate::conversation::{Conversation, ConversationStore};
use crate::error::{AiError, Result};
use crate::prompt::manager::PromptManager;
use crate::prompt::PromptRef;
use crate::providers::LlmFactory;
use crate::request::AiRequest;
use crate::response::{AiResponse, ExecutionTrace};
use crate::routing::{ModelTarget, Router};
use crate::session::{SessionId, UserId, WorkflowId};
use crate::task::{Task, TaskKind};
use crate::telemetry::{estimate_cost, trace_err, trace_ok, trace_start, AiMetrics};
use crate::tools::{ToolExecutor, ToolRegistry};
use binder_llm::{
    ChatMessage, ChatRequest, ChatResponse, EmbeddingInput, EmbeddingRequest, EmbeddingResponse,
    FinishReason, Provider, Usage,
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

    pub async fn chat(&self, request: AiRequest) -> Result<AiResponse> {
        self.dispatch(request).await
    }

    pub async fn chat_json<T: DeserializeOwned>(&self, mut request: AiRequest) -> Result<T> {
        request.json_mode = true;
        let response = self.dispatch(request).await?;
        serde_json::from_str(response.content.trim())
            .map_err(|e| AiError::json(format!("assistant content is not valid JSON: {e}"), 1))
    }

    pub async fn execute(&self, task: Task) -> Result<AiResponse> {
        self.dispatch(AiRequest::from_task(task)).await
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
        let available = self.available();
        let chain = self.router.route(
            TaskKind::Embedding,
            None,
            self.config.preferred_provider,
            self.config.preferred_model(TaskKind::Embedding),
            &available,
        )?;
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

    async fn dispatch(&self, request: AiRequest) -> Result<AiResponse> {
        let started = Instant::now();
        let policy = request.policy.unwrap_or(self.config.routing_policy);
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

        let available = self.available();
        let override_model = request
            .override_model
            .as_deref()
            .or_else(|| self.config.preferred_model(request.task));
        let chain = self.router.route(
            request.task,
            request.policy,
            request.override_provider.or(self.config.preferred_provider),
            override_model,
            &available,
        )?;

        trace_start(request.task, policy.as_str(), &chain.primary().model);

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
        messages.push(ChatMessage::user(&request.input));

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
                        &response.execution.model,
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
            let content = response.message.content().trim();
            if serde_json::from_str::<serde_json::Value>(content).is_ok() {
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
        let cost = estimate_cost(target.provider, usage);
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
                policy: String::new(),
            },
        })
    }

    fn available(&self) -> Vec<Provider> {
        if let Some(forced) = &self.available_providers {
            return forced.clone();
        }
        let mut providers = self.config.available_providers();
        if !self.config.fallback_order.is_empty() {
            providers.retain(|p| self.config.fallback_order.contains(p));
            let mut ordered = Vec::new();
            for p in &self.config.fallback_order {
                if providers.contains(p) {
                    ordered.push(*p);
                }
            }
            return ordered;
        }
        providers
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
            policy: "cache".into(),
        },
    }
}
