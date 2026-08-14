/// `gen_ai.provider.name` (vendor/semconv-genai @ 30182acd, `model/gen-ai/registry.yaml`).
///
/// Same rationale as [`crate::operation::OperationName`]: closed `members` list
/// in a Development-status spec, so unrecognized values are kept via `Other`
/// rather than rejected. `AwsBedrock` is the provider used by the fraudos
/// vertical (dossier section 3) — its value must be exactly `"aws.bedrock"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderName {
    OpenAi,
    GcpGenAi,
    GcpVertexAi,
    GcpGemini,
    Anthropic,
    Cohere,
    AzureAiInference,
    AzureAiOpenAi,
    IbmWatsonxAi,
    AwsBedrock,
    Perplexity,
    XAi,
    DeepSeek,
    Groq,
    MistralAi,
    MoonshotAi,
    Other(String),
}

impl ProviderName {
    pub fn as_str(&self) -> &str {
        match self {
            ProviderName::OpenAi => "openai",
            ProviderName::GcpGenAi => "gcp.gen_ai",
            ProviderName::GcpVertexAi => "gcp.vertex_ai",
            ProviderName::GcpGemini => "gcp.gemini",
            ProviderName::Anthropic => "anthropic",
            ProviderName::Cohere => "cohere",
            ProviderName::AzureAiInference => "azure.ai.inference",
            ProviderName::AzureAiOpenAi => "azure.ai.openai",
            ProviderName::IbmWatsonxAi => "ibm.watsonx.ai",
            ProviderName::AwsBedrock => "aws.bedrock",
            ProviderName::Perplexity => "perplexity",
            ProviderName::XAi => "x_ai",
            ProviderName::DeepSeek => "deepseek",
            ProviderName::Groq => "groq",
            ProviderName::MistralAi => "mistral_ai",
            ProviderName::MoonshotAi => "moonshot_ai",
            ProviderName::Other(s) => s,
        }
    }
}

impl From<&str> for ProviderName {
    fn from(s: &str) -> Self {
        match s {
            "openai" => ProviderName::OpenAi,
            "gcp.gen_ai" => ProviderName::GcpGenAi,
            "gcp.vertex_ai" => ProviderName::GcpVertexAi,
            "gcp.gemini" => ProviderName::GcpGemini,
            "anthropic" => ProviderName::Anthropic,
            "cohere" => ProviderName::Cohere,
            "azure.ai.inference" => ProviderName::AzureAiInference,
            "azure.ai.openai" => ProviderName::AzureAiOpenAi,
            "ibm.watsonx.ai" => ProviderName::IbmWatsonxAi,
            "aws.bedrock" => ProviderName::AwsBedrock,
            "perplexity" => ProviderName::Perplexity,
            "x_ai" => ProviderName::XAi,
            "deepseek" => ProviderName::DeepSeek,
            "groq" => ProviderName::Groq,
            "mistral_ai" => ProviderName::MistralAi,
            "moonshot_ai" => ProviderName::MoonshotAi,
            other => ProviderName::Other(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_fraudos_provider() {
        assert_eq!(ProviderName::from("aws.bedrock"), ProviderName::AwsBedrock);
    }

    #[test]
    fn keeps_unknown_providers_instead_of_failing() {
        assert_eq!(
            ProviderName::from("acme.llm"),
            ProviderName::Other("acme.llm".to_string())
        );
    }
}
