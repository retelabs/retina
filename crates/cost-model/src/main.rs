//! A real cost model (design dossier section 5, criterion refined with the
//! owner on 2026-08-15: compare by **cost at zero usage**, not only at the
//! expected volume). The second "cloud learning" piece of work, after
//! `crates/orchestrator`, in the same spirit: cost things ourselves, from
//! inputs that are measured (`src/measure.rs`, against a real ClickHouse) or
//! checked against real sources (`src/pricing.rs`), not invented.
//!
//! Compares two options that share the same core (VM + `orchestrator` +
//! ClickHouse/kernel/query-api, see the "Venice Deployment" artefact):
//! "all self-built" (nothing else) and "hybrid" (+ object storage for
//! backups, the only one of the three hybrid additions whose cost depends on
//! volume; the container registry and the CDN are already at €0 at any
//! realistic volume for this project, see `docs/cost-model.md`).
//!
//! Usage: `cargo run -p cost-model [-- --volume=N] [--bytes-per-span=N]`
//!   --volume=N            print a single volume (spans a day) instead of the
//!                         default reference points (0, 1k, 100k, 1M a day).
//!   --bytes-per-span=N    skip the real ClickHouse measurement and use this
//!                         value, useful without a local instance running.

use clickhouse::Client;
use cost_model::measure::measure_bytes_per_span;
use cost_model::pricing::{BACKBLAZE_B2, HETZNER_CX23};
use cost_model::report::{CostReport, compute};

/// `crates/clickhouse-sink/migrations/0002_spans_retention_ttl.sql`: the
/// real retention window, not a separate assumption.
const RETENTION_DAYS: u32 = 90;
const DEFAULT_VOLUMES: [u64; 4] = [0, 1_000, 100_000, 1_000_000];

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn flag_value<'a>(args: &'a [String], prefix: &str) -> Option<&'a str> {
    args.iter().find_map(|a| a.strip_prefix(prefix))
}

async fn resolve_bytes_per_span(args: &[String]) -> f64 {
    if let Some(v) = flag_value(args, "--bytes-per-span=") {
        return v.parse().expect("--bytes-per-span must be a number");
    }

    let client = Client::default()
        .with_url(env_or("CLICKHOUSE_URL", "http://localhost:8123"))
        .with_user(env_or("CLICKHOUSE_USER", "dev"))
        .with_password(env_or("CLICKHOUSE_PASSWORD", "dev"))
        .with_database(env_or("CLICKHOUSE_DATABASE", "observability"));

    match measure_bytes_per_span(&client, "observability", "spans")
        .await
        .expect(
            "the system.parts query failed: is ClickHouse running? (scripts/dev-clickhouse.sh up)",
        ) {
        Some(bytes) => bytes,
        None => panic!(
            "the spans table is empty: replay at least one real fixture \
             (fraudos-replay/oncology-replay) before measuring, or pass \
             --bytes-per-span=N to skip the measurement"
        ),
    }
}

fn print_report(report: &CostReport) {
    println!(
        "\n{} spans a day (retention {} days → {} spans stored at steady state, {:.3} GB)",
        report.spans_per_day, report.retention_days, report.stored_spans, report.stored_gb
    );
    println!(
        "  VM (all self-built AND hybrid): €{:.2} a month, {}",
        report.vm_monthly_eur, HETZNER_CX23.label
    );
    match report.days_until_disk_full {
        None => println!(
            "  included disk ({:.0} GB): never touched at volume 0",
            HETZNER_CX23.included_disk_gb
        ),
        Some(days) if report.steady_state_exceeds_disk => println!(
            "  included disk ({:.0} GB): full after {days:.0} days (before retention caps the growth)",
            HETZNER_CX23.included_disk_gb
        ),
        Some(_) => println!(
            "  included disk ({:.0} GB): enough for the whole retention window",
            HETZNER_CX23.included_disk_gb
        ),
    }
    println!(
        "  + object storage ({}): ${:.4} a month",
        BACKBLAZE_B2.label, report.object_storage_monthly_usd
    );
    println!("  + CDN/edge: €{:.2} a month", report.cdn_monthly_eur);
    println!(
        "  + container registry: €{:.2} a month",
        report.registry_monthly_eur
    );
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bytes_per_span = resolve_bytes_per_span(&args).await;

    println!("measured bytes per span (compressed, spans table): {bytes_per_span:.1}");

    let volumes: Vec<u64> = match flag_value(&args, "--volume=") {
        Some(v) => vec![v.parse().expect("--volume must be an integer")],
        None => DEFAULT_VOLUMES.to_vec(),
    };

    for spans_per_day in volumes {
        let report = compute(
            spans_per_day,
            bytes_per_span,
            RETENTION_DAYS,
            &HETZNER_CX23,
            &BACKBLAZE_B2,
        );
        print_report(&report);
    }
}
