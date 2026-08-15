-- Rétention (docs/interfaces/clickhouse-retention.md) : 90 jours, choisis
-- avec l'utilisateur — assez pour investiguer un incident a posteriori sans
-- accumuler indéfiniment. `partition by toYYYYMMDD(start_time)` était déjà
-- en place depuis la migration 0001 précisément pour que ceci ne coûte
-- qu'une commande : ClickHouse peut supprimer des partitions entières une
-- fois expirées, pas ligne par ligne (vérifié contre la doc ClickHouse
-- réelle avant d'écrire cette migration).
--
-- La suppression n'est pas instantanée à l'expiration : ClickHouse purge les
-- lignes expirées lors des merges en arrière-plan, pas de façon synchrone.
ALTER TABLE spans
    MODIFY TTL start_time + INTERVAL 90 DAY DELETE;
