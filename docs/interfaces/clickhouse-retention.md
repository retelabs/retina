# clickhouse-retention — rétention et évolution de schéma

- Source faisant autorité : doc ClickHouse réelle sur les TTL de table
  (https://clickhouse.com/docs/engines/table-engines/mergetree-family/mergetree),
  vérifiée le 2026-08-15 avant d'écrire la migration — syntaxe `TTL expr
  DELETE` à la création, `ALTER TABLE ... MODIFY TTL expr` sur une table
  existante. Comportement de `max()` sur une table vide (`0`, pas `NULL`,
  pour une colonne `UInt32`) vérifié empiriquement contre un vrai serveur
  local (pas trouvé de réponse ferme dans la doc elle-même).
- Durée de rétention (90 jours) : tranchée avec l'utilisateur — assez pour
  investiguer un incident a posteriori sans accumuler indéfiniment.
- Date de vérification : 2026-08-15

## Rétention : TTL sur `spans`

`crates/clickhouse-sink/migrations/0002_spans_retention_ttl.sql` :

```sql
ALTER TABLE spans
    MODIFY TTL start_time + INTERVAL 90 DAY DELETE;
```

Peu coûteux précisément parce que `PARTITION BY toYYYYMMDD(start_time)`
existait déjà depuis la migration 0001 (`docs/interfaces/clickhouse-schema.md`
l'anticipait explicitement) : ClickHouse peut supprimer des partitions
entières une fois expirées plutôt que ligne par ligne.

**Ce que le TTL ne garantit pas** : la suppression n'est pas synchrone à
l'expiration — "Data with an expired TTL is removed when ClickHouse merges
data parts" (doc officielle). Une ligne expirée peut rester visible jusqu'au
prochain merge en arrière-plan (réglable via `merge_with_ttl_timeout`, pas
retouché ici — comportement par défaut du serveur). `OPTIMIZE TABLE spans
FINAL` forcerait une purge immédiate si jamais nécessaire en opération, mais
n'est pas automatisé.

## Évolution de schéma : `schema_migrations` + fichiers numérotés

Avant cette étape, il n'existait qu'un seul fichier de migration, appliqué à
chaque démarrage via un `CREATE TABLE IF NOT EXISTS` — idempotent par
chance, pas par conception. Ça ne tenait plus dès qu'une deuxième migration
(`ALTER TABLE ... MODIFY TTL`, puis potentiellement `ADD COLUMN` un jour) devait
être appliquée exactement une fois, pas rejouée sans discernement à chaque
démarrage.

`crates/clickhouse-sink/src/migrate.rs` (`run_migrations`, exportée par le
crate) :
- Table `schema_migrations (version UInt32, name String, applied_at DateTime
  DEFAULT now()) ENGINE = MergeTree ORDER BY version`, créée si absente.
- `SELECT max(version)` (→ `0` sur une table neuve) donne la version
  courante ; chaque migration dont la version est supérieure est appliquée
  dans l'ordre puis enregistrée.
- Migrations elles-mêmes définies comme une liste statique
  `(version, name, sql)` dans `migrate.rs`, `sql` chargé via `include_str!`
  depuis `migrations/NNNN_*.sql` — un seul endroit qui connaît l'ordre, plus
  de duplication de `include_str!` à travers `crates/kernel` et 3 suites de
  tests d'intégration (c'était le cas avant cette étape).
- Un test unitaire (`migrate::tests::migrations_are_numbered_sequentially_from_one`)
  vérifie que la liste reste `1, 2, 3, ...` sans trou ni doublon — erreur
  d'auteur détectée avant tout déploiement, pas seulement en production.

**Hypothèse mono-instance, assumée** : deux processus qui appliqueraient la
même migration en même temps ne sont pas gérés (pas de verrou distribué) —
cohérent avec le dossier section 4 (pas de haute disponibilité au MVP). Il
n'existe qu'un seul processus kernel aujourd'hui.

## Vérifié comment

Contre un vrai ClickHouse local (`scripts/dev-clickhouse.sh up`), pas
seulement en lisant la doc :
- `SELECT max(version) FROM <table vide>` → confirmé `0`, pas `NULL`.
- Base entièrement fraîche (`spans`/`schema_migrations` supprimées) :
  `cargo test --workspace -- --ignored` au vert, les deux migrations
  appliquées et enregistrées (`schema_migrations` contient les versions 1
  et 2), `system.tables.engine_full` confirme
  `TTL start_time + toIntervalDay(90)` sur `spans`.
- **Scénario de mise à niveau réel** : `spans` recréée sans TTL et
  `schema_migrations` supprimée (simule un déploiement antérieur à cette
  fonctionnalité), puis `cargo run -p kernel` réellement lancé contre cette
  base — la migration 2 s'applique automatiquement au démarrage sans
  intervention manuelle, `system.tables` confirme le TTL présent après coup.
