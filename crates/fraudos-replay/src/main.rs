//! Usage: `cargo run -p fraudos-replay -- [fixture.json]`
//!
//! Loads an `AgentSpan` JSON fixture (default: `fixtures/fraud_investigator_confirmed.json`
//! relative to this crate), converts it to OTLP, and sends it via real gRPC
//! to a running kernel (`KERNEL_ADDR`, default `http://localhost:4317`) —
//! see docs/interfaces/fraudos-agentspan.md.

use fraudos_replay::{AgentSpan, convert};
use otlp_receiver::{TraceServiceClient, bearer_metadata_value};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fixture_path = std::env::args().nth(1).unwrap_or_else(|| {
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/fraud_investigator_confirmed.json"
        )
        .to_string()
    });
    let kernel_addr =
        std::env::var("KERNEL_ADDR").unwrap_or_else(|_| "http://localhost:4317".to_string());
    // Must match the running kernel's KERNEL_API_KEY (docs/interfaces/kernel-auth.md).
    let api_key = std::env::var("KERNEL_API_KEY")
        .map_err(|_| "KERNEL_API_KEY must be set — see docs/interfaces/kernel-auth.md")?;

    let raw = std::fs::read_to_string(&fixture_path)
        .map_err(|e| format!("reading {fixture_path}: {e}"))?;
    let agent_span: AgentSpan =
        serde_json::from_str(&raw).map_err(|e| format!("parsing {fixture_path}: {e}"))?;

    let request = convert(&agent_span)?;
    let span_count: usize = request
        .resource_spans
        .iter()
        .map(|rs| {
            rs.scope_spans
                .iter()
                .map(|ss| ss.spans.len())
                .sum::<usize>()
        })
        .sum();
    eprintln!(
        "converted {fixture_path} (session_id={}) into {span_count} OTLP span(s)",
        agent_span.session_id
    );

    let mut client = TraceServiceClient::connect(kernel_addr.clone())
        .await
        .map_err(|e| {
            format!(
                "connecting to kernel at {kernel_addr} (is `cargo run -p kernel` running?): {e}"
            )
        })?;

    let mut request = tonic::Request::new(request);
    request
        .metadata_mut()
        .insert("authorization", bearer_metadata_value(&api_key)?);
    let response = client.export(request).await?.into_inner();
    match response.partial_success {
        Some(p) if p.rejected_spans > 0 => {
            eprintln!(
                "kernel rejected {} span(s): {}",
                p.rejected_spans, p.error_message
            );
        }
        _ => eprintln!("kernel accepted all {span_count} span(s)"),
    }

    Ok(())
}
