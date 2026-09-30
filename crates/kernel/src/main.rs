//! Wires the OTLP/traces receiver (crates/otlp-receiver) through the plugin
//! layer (crates/plugin-sink) to the ClickHouse sink (crates/clickhouse-sink)
//! and actually runs it as a gRPC server.
//!
//! Neither of those crates could depend on the other's binary shape without
//! a cycle (`clickhouse-sink` already depends on `otlp-receiver` for
//! `ConvertedEvent`/`SpanSink`), so this is where "the kernel" becomes a
//! process you can run, rather than a library you can only unit-test — the
//! thing dossier step 7 needs ("run the kernel against a real telemetry
//! flow").
//!
//! `PluginSink` wraps `ClickHouseSink`: this is the first time any plugin
//! actually runs as part of ingestion, not just in isolated crate tests —
//! see docs/interfaces/oncology-governance.md for why this insertion point
//! was chosen.
//!
//! Which plugins run is chosen by `ENABLED_PLUGINS` (`select_enabled`
//! below), not hardcoded — still every plugin is a Rust type compiled into
//! this binary (no dynamic code loading; that's the WASM question left open
//! in docs/interfaces/plugin-contract-v0.md, deliberately not tackled here).

use clickhouse::Client;
use clickhouse_sink::ClickHouseSink;
use otlp_receiver::{ApiKeyInterceptor, Receiver, TraceServiceServer};
use plugin_api::Plugin;
use plugin_fraudos::FraudosPlugin;
use plugin_medical::MedicalPlugin;
use plugin_sink::PluginSink;
use plugin_triage_eval::{DEFAULT_KNOWN_SERVICES, TriageEvalPlugin};
use tonic::transport::Server;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// `KERNEL_API_KEYS_EXTRA`: comma-separated additional valid tokens, on top
/// of the required `KERNEL_API_KEY` — a second real client (alongside
/// the first client and fraudos-replay) gets its own revocable credential instead
/// of sharing the first one's. `None`/unset means "no extra tokens", same
/// shape as `select_enabled`/`triage_known_services` above: a pure function,
/// testable without touching the environment.
fn parse_extra_tokens(raw: Option<&str>) -> Vec<String> {
    raw.map(|config| {
        config
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default()
}

/// Filters `available` (name, plugin) pairs down to the ones named in
/// `requested` (a comma-separated `ENABLED_PLUGINS` value). `None` (the env
/// var unset) means "run everything" — preserves the behavior before this
/// existed, so `scripts/demo.sh` and existing deployments need no new
/// config to keep working. An explicit empty string is a deliberate "run no
/// plugins", distinct from unset.
///
/// Panics on an unknown name rather than silently ignoring it — a typo in
/// `ENABLED_PLUGINS` that got swallowed would look like the plugin ran and
/// just had nothing to say, not like a misconfiguration.
fn select_enabled<T>(available: Vec<(&'static str, T)>, requested: Option<&str>) -> Vec<T> {
    let Some(config) = requested else {
        return available.into_iter().map(|(_, p)| p).collect();
    };

    let known_names: Vec<&str> = available.iter().map(|(name, _)| *name).collect();
    let requested_names: Vec<&str> = config
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    for name in &requested_names {
        if !known_names.contains(name) {
            panic!(
                "ENABLED_PLUGINS references unknown plugin `{name}` — known plugins: {known_names:?}"
            );
        }
    }

    available
        .into_iter()
        .filter(|(name, _)| requested_names.contains(name))
        .map(|(_, p)| p)
        .collect()
}

/// `TRIAGE_KNOWN_SERVICES`: comma-separated vocabulary override for
/// `TriageEvalPlugin` (docs/interfaces/triage-eval-plugin.md) — `None` means
/// "use the first client's real `Service` list" (`DEFAULT_KNOWN_SERVICES`), not
/// "disable the check" (an empty effective vocabulary would flag every tag
/// as unknown, which is worse than just using real data as the default).
/// Takes `Option<&str>` rather than reading the env var itself — same shape
/// as `select_enabled`, testable without mutating global process state.
fn triage_known_services(requested: Option<&str>) -> Vec<String> {
    match requested {
        Some(config) => config.split(',').map(|s| s.trim().to_string()).collect(),
        None => DEFAULT_KNOWN_SERVICES
            .iter()
            .map(|s| s.to_string())
            .collect(),
    }
}

fn build_plugins() -> Vec<Box<dyn Plugin>> {
    let known_services =
        triage_known_services(std::env::var("TRIAGE_KNOWN_SERVICES").ok().as_deref());
    let available: Vec<(&'static str, Box<dyn Plugin>)> = vec![
        ("fraudos-plugin", Box::new(FraudosPlugin)),
        ("medical-plugin", Box::new(MedicalPlugin)),
        (
            "triage-eval-plugin",
            Box::new(TriageEvalPlugin::new(known_services)),
        ),
    ];
    let requested = std::env::var("ENABLED_PLUGINS").ok();
    select_enabled(available, requested.as_deref())
}

/// `KERNEL_API_KEY` is required and must not be blank: `KERNEL_API_KEY=` (set but empty, a
/// common `.env` slip) would otherwise start the process with the token
/// `""`, i.e. accept `authorization: Bearer ` — fail closed on it exactly
/// like on an unset key. Pure function, testable without the environment.
fn primary_api_key(raw: Option<String>) -> Result<String, String> {
    match raw {
        Some(key) if !key.trim().is_empty() => Ok(key),
        Some(_) => {
            Err("KERNEL_API_KEY is set but blank — see docs/interfaces/kernel-auth.md".to_string())
        }
        None => Err("KERNEL_API_KEY must be set — see docs/interfaces/kernel-auth.md".to_string()),
    }
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
    let api_key =
        primary_api_key(std::env::var("KERNEL_API_KEY").ok()).unwrap_or_else(|e| panic!("{e}"));
    let mut api_keys = vec![api_key];
    api_keys.extend(parse_extra_tokens(
        std::env::var("KERNEL_API_KEYS_EXTRA").ok().as_deref(),
    ));
    // Resolved before touching ClickHouse: a typo in ENABLED_PLUGINS is a
    // config error, same class as a missing KERNEL_API_KEY — fail before
    // any network I/O, not partway through startup.
    let plugins = build_plugins();

    let client = Client::default()
        .with_url(clickhouse_url)
        .with_user(clickhouse_user)
        .with_password(clickhouse_password)
        .with_database(clickhouse_database);

    // Applies every migration newer than what's recorded (docs/interfaces/clickhouse-retention.md)
    // — a freshly started kernel against an empty database should just
    // work, not require a separate manual migration step for local/dev use.
    clickhouse_sink::run_migrations(&client).await?;

    let clickhouse_sink = ClickHouseSink::new(client, table);
    let sink = PluginSink::new(clickhouse_sink, plugins);
    let receiver = Receiver::new(sink);

    eprintln!("kernel (otlp-receiver + clickhouse-sink) listening on {bind_addr}");
    let interceptor = ApiKeyInterceptor::new(api_keys);
    Server::builder()
        .add_service(TraceServiceServer::with_interceptor(receiver, interceptor))
        .serve(bind_addr)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_api_key_refuses_unset_and_blank() {
        assert!(primary_api_key(None).is_err());
        assert!(primary_api_key(Some(String::new())).is_err());
        assert!(primary_api_key(Some("  ".to_string())).is_err());
        assert_eq!(primary_api_key(Some("k".to_string())).unwrap(), "k");
    }

    fn sample() -> Vec<(&'static str, &'static str)> {
        vec![("fraudos-plugin", "fraudos"), ("medical-plugin", "medical")]
    }

    #[test]
    fn unset_enables_everything() {
        let result = select_enabled(sample(), None);
        assert_eq!(result, vec!["fraudos", "medical"]);
    }

    #[test]
    fn triage_known_services_defaults_to_the_real_service_list_when_unset() {
        let result = triage_known_services(None);
        assert_eq!(
            result,
            DEFAULT_KNOWN_SERVICES
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn triage_known_services_splits_and_trims_an_override() {
        let result = triage_known_services(Some(" Cardiologie , Pédiatrie "));
        assert_eq!(result, vec!["Cardiologie", "Pédiatrie"]);
    }

    #[test]
    fn explicit_empty_string_enables_nothing() {
        let result = select_enabled(sample(), Some(""));
        assert!(result.is_empty());
    }

    #[test]
    fn selects_only_the_named_plugins_regardless_of_order() {
        let result = select_enabled(sample(), Some("medical-plugin"));
        assert_eq!(result, vec!["medical"]);
    }

    #[test]
    fn tolerates_whitespace_around_names() {
        let result = select_enabled(sample(), Some(" fraudos-plugin , medical-plugin "));
        assert_eq!(result, vec!["fraudos", "medical"]);
    }

    #[test]
    #[should_panic(expected = "unknown plugin `not-a-real-plugin`")]
    fn panics_on_unknown_plugin_name() {
        select_enabled(sample(), Some("not-a-real-plugin"));
    }

    #[test]
    fn parse_extra_tokens_unset_is_empty() {
        assert!(parse_extra_tokens(None).is_empty());
    }

    #[test]
    fn parse_extra_tokens_explicit_empty_string_is_empty() {
        assert!(parse_extra_tokens(Some("")).is_empty());
    }

    #[test]
    fn parse_extra_tokens_splits_trims_and_drops_blanks() {
        let result = parse_extra_tokens(Some(" tok-a ,tok-b, ,tok-c "));
        assert_eq!(result, vec!["tok-a", "tok-b", "tok-c"]);
    }
}
