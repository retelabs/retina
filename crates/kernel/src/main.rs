//! Wires the OTLP/traces receiver (crates/otlp-receiver) through the plugin
//! layer (crates/plugin-sink) to the ClickHouse sink (crates/clickhouse-sink)
//! and actually runs it as a gRPC server.
//!
//! Neither of those crates could depend on the other's binary shape without
//! a cycle (`clickhouse-sink` already depends on `otlp-receiver` for
//! `ConvertedEvent`/`SpanSink`), so this is where "the kernel" becomes a
//! process you can run, rather than a library you can only unit-test — the
//! thing dossier étape 7 needs ("faire tourner le kernel contre un vrai flux
//! de télémétrie").
//!
//! `PluginSink` wraps `ClickHouseSink`: this is the first time any plugin
//! actually runs as part of ingestion, not just in isolated crate tests —
//! see docs/interfaces/oncology-governance.md for why this insertion point
//! was chosen.

use clickhouse::Client;
use clickhouse_sink::ClickHouseSink;
use otlp_receiver::{ApiKeyInterceptor, Receiver, TraceServiceServer};
use plugin_api::Plugin;
use plugin_fraudos::FraudosPlugin;
use plugin_medical::MedicalPlugin;
use plugin_sink::PluginSink;
use tonic::transport::Server;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let clickhouse_url = env_or("CLICKHOUSE_URL", "http://localhost:8123");
    let clickhouse_user = env_or("CLICKHOUSE_USER", "dev");
    let clickhouse_password = env_or("CLICKHOUSE_PASSWORD", "dev");
    let clickhouse_database = env_or("CLICKHOUSE_DATABASE", "observability");
    let bind_addr: std::net::SocketAddr = env_or("KERNEL_BIND", "0.0.0.0:4317").parse()?;
    let table = env_or("SPANS_TABLE", "spans");
    // Fails closed (docs/interfaces/kernel-auth.md): no "auth disabled"
    // fallback, an unset key must stop the process rather than start it
    // unauthenticated.
    let api_key = std::env::var("KERNEL_API_KEY")
        .expect("KERNEL_API_KEY must be set — see docs/interfaces/kernel-auth.md");

    let client = Client::default()
        .with_url(clickhouse_url)
        .with_user(clickhouse_user)
        .with_password(clickhouse_password)
        .with_database(clickhouse_database);

    // Applies the same migration clickhouse-sink's own tests apply — a
    // freshly started kernel against an empty database should just work,
    // not require a separate manual migration step for local/dev use.
    client
        .query(include_str!(
            "../../clickhouse-sink/migrations/0001_create_spans.sql"
        ))
        .execute()
        .await?;

    let clickhouse_sink = ClickHouseSink::new(client, table);
    let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(FraudosPlugin), Box::new(MedicalPlugin)];
    let sink = PluginSink::new(clickhouse_sink, plugins);
    let receiver = Receiver::new(sink);

    eprintln!("kernel (otlp-receiver + clickhouse-sink) listening on {bind_addr}");
    let interceptor = ApiKeyInterceptor::new(api_key);
    Server::builder()
        .add_service(TraceServiceServer::with_interceptor(receiver, interceptor))
        .serve(bind_addr)
        .await?;

    Ok(())
}
