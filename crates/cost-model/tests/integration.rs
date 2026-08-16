//! Runs against the real local ClickHouse. Not run by default `cargo
//! test` — requires data already in the `spans` table (rejoue au moins un
//! fixture réel avant, `fraudos-replay`/`oncology-replay`) :
//!
//!   scripts/dev-clickhouse.sh up
//!   cargo run -p fraudos-replay -- crates/fraudos-replay/fixtures/fraud_investigator_confirmed.json
//!   cargo test -p cost-model -- --ignored

use clickhouse::Client;
use cost_model::measure::measure_bytes_per_span;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn test_client() -> Client {
    Client::default()
        .with_url(env_or("CLICKHOUSE_URL", "http://localhost:8123"))
        .with_user(env_or("CLICKHOUSE_USER", "dev"))
        .with_password(env_or("CLICKHOUSE_PASSWORD", "dev"))
        .with_database(env_or("CLICKHOUSE_DATABASE", "observability"))
}

#[tokio::test]
#[ignore = "requires `scripts/dev-clickhouse.sh up` with real replayed data in `spans`"]
async fn measures_a_plausible_positive_bytes_per_span() {
    let client = test_client();

    let bytes_per_span = measure_bytes_per_span(&client, "observability", "spans")
        .await
        .expect("query should succeed — is ClickHouse running?")
        .expect(
            "spans table is empty — replay a real fixture first \
             (fraudos-replay/oncology-replay)",
        );

    // Not a tight bound — the point is proving the query returns a real,
    // sane number (not 0, not implausibly huge from a unit mixup) against
    // an actual server, not that it matches one exact value.
    assert!(
        bytes_per_span > 10.0 && bytes_per_span < 100_000.0,
        "expected a plausible compressed bytes/span, got {bytes_per_span}"
    );
}
