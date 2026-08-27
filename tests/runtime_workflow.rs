use std::path::Path;
use std::sync::Arc;

use binder_ai::{
    AiClient, AiRequest, Command, CommandResult, GitDiff, GitFileStatus, GitStatus, GitStatusEntry,
    MemoryRuntime, Runtime, SearchRequest, TaskKind,
};
use binder_llm::{ChatMessage, ChatResponse, LlmClient, MockProvider, Provider};

fn llm() -> LlmClient {
    let provider = MockProvider::new().with_chat(ChatResponse::new(
        Provider::Ollama,
        "mock",
        ChatMessage::assistant("pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n"),
    ));
    LlmClient::from_provider(
        Arc::new(provider),
        LlmClient::builder()
            .provider(Provider::Ollama)
            .model("mock")
            .build_config()
            .expect("config"),
    )
}

/// Search → read → binder-ai execute → write → cargo fmt/test → git diff.
/// The workflow uses only [`Runtime`] + [`AiClient`], not fs/Command/git2.
#[tokio::test]
async fn issue_workflow_uses_only_runtime_and_ai() {
    let runtime = MemoryRuntime::new()
        .with_file("src/lib.rs", "pub fn add(a: i32, b: i32) -> i32 { a-b }\n")
        .script(Command::new("cargo").arg("fmt"), CommandResult::success(""))
        .script(
            Command::new("cargo").arg("test"),
            CommandResult::success("test result: ok"),
        )
        .with_git_status(GitStatus {
            branch: "feature/runtime-abstraction".into(),
            entries: vec![GitStatusEntry {
                path: "src/lib.rs".into(),
                status: GitFileStatus::Modified,
            }],
        })
        .with_git_diff(GitDiff {
            branch: "feature/runtime-abstraction".into(),
            patch: "diff --git a/src/lib.rs b/src/lib.rs\n".into(),
        });

    let hits = runtime
        .search(SearchRequest::content("fn add").extension("rs"))
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    let source = runtime.read_file(Path::new(&hits[0].path)).await.unwrap();
    assert!(source.contains("a-b"));

    let ai = AiClient::builder().llm(llm()).build();
    let generated = ai
        .execute(
            AiRequest::builder()
                .task(TaskKind::CodeGeneration)
                .input(format!("Fix the add function:\n{source}"))
                .build(),
        )
        .await
        .unwrap();
    runtime
        .write_file(Path::new("src/lib.rs"), &generated.content)
        .await
        .unwrap();
    assert!(runtime
        .execute(Command::new("cargo").arg("fmt"))
        .await
        .unwrap()
        .is_success());
    assert!(runtime
        .execute(Command::new("cargo").arg("test"))
        .await
        .unwrap()
        .is_success());
    let diff = runtime.git_diff().await.unwrap();
    assert!(diff.patch.contains("src/lib.rs"));
    assert_eq!(
        runtime.files()[&Path::new("src/lib.rs").to_path_buf()],
        generated.content
    );
}
