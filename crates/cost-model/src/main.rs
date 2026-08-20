//! Modèle de coût réel (dossier section 5, critère précisé avec
//! l'utilisateur le 2026-08-15 : comparer par le **coût à usage zéro**, pas
//! seulement à volume attendu). Deuxième chantier "apprentissage cloud",
//! après `crates/orchestrator` — même esprit : chiffrer nous-mêmes, à
//! partir d'entrées mesurées (`src/measure.rs`, contre un vrai ClickHouse)
//! ou vérifiées contre des sources réelles (`src/pricing.rs`), pas
//! inventées.
//!
//! Compare deux options qui partagent le même cœur (VM + `orchestrator` +
//! ClickHouse/kernel/query-api, voir l'artefact "Venice Deployment") :
//! "100% perso" (rien d'autre) et "hybride" (+ stockage objet pour les
//! backups, le seul des 3 ajouts hybrides dont le coût dépend du volume —
//! le registre de conteneurs et le CDN sont déjà à 0€ à tout volume
//! réaliste pour ce projet, voir `docs/cost-model.md`).
//!
//! Usage : `cargo run -p cost-model [-- --volume=N] [--bytes-per-span=N]`
//!   --volume=N            n'affiche qu'un seul volume (spans/jour) au
//!                         lieu des repères par défaut (0, 1k, 100k, 1M/j).
//!   --bytes-per-span=N    saute la mesure ClickHouse réelle, utilise cette
//!                         valeur — utile sans instance locale en marche.

use clickhouse::Client;
use cost_model::measure::measure_bytes_per_span;
use cost_model::pricing::{BACKBLAZE_B2, HETZNER_CX23};
use cost_model::report::{CostReport, compute};

/// `crates/clickhouse-sink/migrations/0002_spans_retention_ttl.sql` — la
/// vraie fenêtre de rétention, pas une hypothèse séparée.
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
        return v.parse().expect("--bytes-per-span doit être un nombre");
    }

    let client = Client::default()
        .with_url(env_or("CLICKHOUSE_URL", "http://localhost:8123"))
        .with_user(env_or("CLICKHOUSE_USER", "dev"))
        .with_password(env_or("CLICKHOUSE_PASSWORD", "dev"))
        .with_database(env_or("CLICKHOUSE_DATABASE", "observability"));

    match measure_bytes_per_span(&client, "observability", "spans")
        .await
        .expect("échec de la requête system.parts — ClickHouse tourne-t-il ? (scripts/dev-clickhouse.sh up)")
    {
        Some(bytes) => bytes,
        None => panic!(
            "la table spans est vide — rejoue au moins un fixture réel \
             (fraudos-replay/oncology-replay) avant de mesurer, ou passe \
             --bytes-per-span=N pour sauter la mesure"
        ),
    }
}

fn print_report(report: &CostReport) {
    println!(
        "\n{} spans/jour (rétention {} j → {} spans stockés à l'état stationnaire, {:.3} Go)",
        report.spans_per_day, report.retention_days, report.stored_spans, report.stored_gb
    );
    println!(
        "  VM (100% perso ET hybride) : {:.2} €/mois — {}",
        report.vm_monthly_eur, HETZNER_CX23.label
    );
    match report.days_until_disk_full {
        None => println!(
            "  disque inclus ({:.0} Go) : jamais entamé à volume 0",
            HETZNER_CX23.included_disk_gb
        ),
        Some(days) if report.steady_state_exceeds_disk => println!(
            "  disque inclus ({:.0} Go) : dépassé après {days:.0} jours (avant que la rétention ne plafonne la croissance)",
            HETZNER_CX23.included_disk_gb
        ),
        Some(_) => println!(
            "  disque inclus ({:.0} Go) : suffisant pour toute la fenêtre de rétention",
            HETZNER_CX23.included_disk_gb
        ),
    }
    println!(
        "  + stockage objet ({}) : {:.4} $/mois",
        BACKBLAZE_B2.label, report.object_storage_monthly_usd
    );
    println!("  + CDN/edge : {:.2} €/mois", report.cdn_monthly_eur);
    println!(
        "  + registre de conteneurs : {:.2} €/mois",
        report.registry_monthly_eur
    );
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bytes_per_span = resolve_bytes_per_span(&args).await;

    println!("octets/span mesurés (compressés, table spans) : {bytes_per_span:.1}");

    let volumes: Vec<u64> = match flag_value(&args, "--volume=") {
        Some(v) => vec![v.parse().expect("--volume doit être un entier")],
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
