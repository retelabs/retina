/// `gen_ai.operation.name` (vendor/semconv-genai @ 30182acd, `model/gen-ai/registry.yaml`).
///
/// The spec defines this as a closed `members` enum, but the repo is
/// Development-status and adds operations without a stable version bump (see
/// docs/interfaces/semconv-genai.md). `Other` keeps unrecognized values instead
/// of failing to parse them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationName {
    Chat,
    GenerateContent,
    TextCompletion,
    Embeddings,
    Retrieval,
    FetchResponse,
    CreateAgent,
    InvokeAgent,
    ExecuteTool,
    InvokeWorkflow,
    Plan,
    SearchMemory,
    CreateMemory,
    UpdateMemory,
    UpsertMemory,
    DeleteMemory,
    CreateMemoryStore,
    DeleteMemoryStore,
    Other(String),
}

impl OperationName {
    pub fn as_str(&self) -> &str {
        match self {
            OperationName::Chat => "chat",
            OperationName::GenerateContent => "generate_content",
            OperationName::TextCompletion => "text_completion",
            OperationName::Embeddings => "embeddings",
            OperationName::Retrieval => "retrieval",
            OperationName::FetchResponse => "fetch_response",
            OperationName::CreateAgent => "create_agent",
            OperationName::InvokeAgent => "invoke_agent",
            OperationName::ExecuteTool => "execute_tool",
            OperationName::InvokeWorkflow => "invoke_workflow",
            OperationName::Plan => "plan",
            OperationName::SearchMemory => "search_memory",
            OperationName::CreateMemory => "create_memory",
            OperationName::UpdateMemory => "update_memory",
            OperationName::UpsertMemory => "upsert_memory",
            OperationName::DeleteMemory => "delete_memory",
            OperationName::CreateMemoryStore => "create_memory_store",
            OperationName::DeleteMemoryStore => "delete_memory_store",
            OperationName::Other(s) => s,
        }
    }
}

impl From<&str> for OperationName {
    fn from(s: &str) -> Self {
        match s {
            "chat" => OperationName::Chat,
            "generate_content" => OperationName::GenerateContent,
            "text_completion" => OperationName::TextCompletion,
            "embeddings" => OperationName::Embeddings,
            "retrieval" => OperationName::Retrieval,
            "fetch_response" => OperationName::FetchResponse,
            "create_agent" => OperationName::CreateAgent,
            "invoke_agent" => OperationName::InvokeAgent,
            "execute_tool" => OperationName::ExecuteTool,
            "invoke_workflow" => OperationName::InvokeWorkflow,
            "plan" => OperationName::Plan,
            "search_memory" => OperationName::SearchMemory,
            "create_memory" => OperationName::CreateMemory,
            "update_memory" => OperationName::UpdateMemory,
            "upsert_memory" => OperationName::UpsertMemory,
            "delete_memory" => OperationName::DeleteMemory,
            "create_memory_store" => OperationName::CreateMemoryStore,
            "delete_memory_store" => OperationName::DeleteMemoryStore,
            other => OperationName::Other(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_known_values() {
        assert_eq!(
            OperationName::from("execute_tool"),
            OperationName::ExecuteTool
        );
        assert_eq!(OperationName::ExecuteTool.as_str(), "execute_tool");
    }

    #[test]
    fn keeps_unknown_values_instead_of_failing() {
        let op = OperationName::from("summarize_thread");
        assert_eq!(op, OperationName::Other("summarize_thread".to_string()));
        assert_eq!(op.as_str(), "summarize_thread");
    }
}
