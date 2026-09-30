//! Schema evolution: numbered `.sql` files in `migrations/`, applied in
//! order and tracked in a `schema_migrations` table so each one runs
//! exactly once — not relied on to stay idempotent forever the way
//! migration 0001's `CREATE TABLE IF NOT EXISTS` happened to be (a future
//! `ALTER TABLE ... ADD COLUMN` wouldn't survive being re-run). See
//! docs/interfaces/clickhouse-retention.md.
//!
//! Single-instance assumption (dossier section 4, outside the MVP: no
//! high availability): concurrent kernels racing to apply the
//! same migration isn't handled — there's exactly one kernel process today.

use clickhouse::Client;

struct Migration {
    version: u32,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "create_spans",
        sql: include_str!("../migrations/0001_create_spans.sql"),
    },
    Migration {
        version: 2,
        name: "spans_retention_ttl",
        sql: include_str!("../migrations/0002_spans_retention_ttl.sql"),
    },
    Migration {
        version: 3,
        name: "add_cost_usd",
        sql: include_str!("../migrations/0003_add_cost_usd.sql"),
    },
];

/// Applies every migration newer than what's already recorded, in order.
/// Safe to call on every startup (`crates/kernel` does, every integration
/// test suite does): a fresh database runs everything from version 1; an
/// up-to-date one costs a single `SELECT max(version)` and nothing else.
pub async fn run_migrations(client: &Client) -> clickhouse::error::Result<()> {
    client
        .query(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version UInt32,
                name String,
                applied_at DateTime DEFAULT now()
            ) ENGINE = MergeTree ORDER BY version",
        )
        .execute()
        .await?;

    // 0 on an empty table — verified against a real server: ClickHouse's
    // aggregate functions return the column type's default on an empty
    // input, not NULL, unless using the `-OrNull` combinator — which is
    // exactly "apply everything" for 1-indexed migration versions.
    let current_version: u32 = client
        .query("SELECT max(version) FROM schema_migrations")
        .fetch_one()
        .await?;

    for migration in MIGRATIONS.iter().filter(|m| m.version > current_version) {
        client.query(migration.sql).execute().await?;
        client
            .query("INSERT INTO schema_migrations (version, name) VALUES (?, ?)")
            .bind(migration.version)
            .bind(migration.name)
            .execute()
            .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_numbered_sequentially_from_one() {
        // Catches a future authoring mistake (wrong/duplicate/out-of-order
        // version) before it ever reaches a real database — the runner
        // above trusts this ordering without re-checking it at runtime.
        let versions: Vec<u32> = MIGRATIONS.iter().map(|m| m.version).collect();
        let expected: Vec<u32> = (1..=MIGRATIONS.len() as u32).collect();
        assert_eq!(versions, expected);
    }
}
