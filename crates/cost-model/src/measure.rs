//! Mesure les octets/span réels contre un vrai ClickHouse — pas une
//! estimation analytique depuis les types de colonnes
//! (`docs/interfaces/clickhouse-schema.md`), qui ignorerait la compression
//! réelle (`LowCardinality`, compression de colonne) et surestimerait
//! largement. `system.parts` donne la taille compressée réellement sur
//! disque — vérifié en lisant `DESCRIBE TABLE system.parts` contre un vrai
//! serveur avant d'écrire cette requête (`rows`, `data_compressed_bytes`,
//! `database`, `table`, `active` confirmés là, pas devinés).

use clickhouse::Client;
use clickhouse::Row;
use serde::Deserialize;

#[derive(Row, Deserialize)]
struct PartsAggregate {
    rows: u64,
    compressed_bytes: u64,
}

/// `None` si la table n'a aucune ligne (rien à mesurer) plutôt qu'une
/// division par zéro déguisée en `0.0` trompeur.
pub async fn measure_bytes_per_span(
    client: &Client,
    database: &str,
    table: &str,
) -> clickhouse::error::Result<Option<f64>> {
    let agg: PartsAggregate = client
        .query(
            "SELECT sum(rows) AS rows, sum(data_compressed_bytes) AS compressed_bytes \
             FROM system.parts WHERE database = ? AND table = ? AND active = 1",
        )
        .bind(database)
        .bind(table)
        .fetch_one()
        .await?;

    if agg.rows == 0 {
        return Ok(None);
    }
    Ok(Some(agg.compressed_bytes as f64 / agg.rows as f64))
}
