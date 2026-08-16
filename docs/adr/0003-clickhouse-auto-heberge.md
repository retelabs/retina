# 0003. ClickHouse auto-hébergé, pas de service managé

Date : 2026-08-16 (construit ainsi depuis l'étape 3 du kernel, 2026-08-14 ;
formalisé ici)

Statut : Accepté

## Contexte

Le dossier de conception (section 5) donnait un repère volumétrique pour
arbitrer : stockage managé (BigQuery/ADX) gagnant en coût total sous
~100k requêtes/jour, auto-hébergé intéressant au-dessus de ~1M/jour si la
bande passante ops suit. Le kernel a été construit et validé dès l'étape 3
contre un ClickHouse auto-hébergé local (`docker/docker-compose.clickhouse.yml`),
sans jamais remettre ce choix en question au fil du projet — les étapes 3 à
7, la rétention (ADR implicite dans `docs/interfaces/clickhouse-retention.md`)
et le control plane (`crates/orchestrator`) sont tous construits dessus.

## Décision

ClickHouse auto-hébergé (conteneur Docker géré par nous, orchestré par
`crates/orchestrator`) — pas ClickHouse Cloud, BigQuery, ni ADX. Rétention
gérée nous-mêmes (TTL 90 jours, migration versionnée,
`docs/interfaces/clickhouse-retention.md`).

## Conséquences

`crates/cost-model` a depuis chiffré ce choix pour de vrai, pas seulement
en théorie : au repère du dossier (100k spans/jour), le stockage accumulé
sur la fenêtre de rétention (90 jours) reste sous le palier gratuit d'un
stockage objet de secours (`docs/cost-model.md`) — largement en dessous de
ce qu'un service managé facturerait à l'instance, quel que soit l'usage
réel. Le repère volumétrique du dossier reste valable en théorie, mais le
critère qui tranche réellement est différent (ADR-0002) : un service
managé facture une capacité provisionnée même à faible usage, ce que le
projet évite désormais par principe — pas seulement parce que le volume
actuel est sous un seuil.

Revers assumé : pas de scaling/réplication géré, pas de support
fournisseur, montée en charge à gérer nous-mêmes si le volume dépasse un
jour ce qu'une seule instance encaisse (repère du dossier : au-dessus de
~1M/jour, à revisiter).

## Alternatives considérées

- **BigQuery / ADX** (managé) — écartés pour le critère de coût à usage
  zéro (ADR-0002) : facturation à l'instance ou au slot réservé selon le
  produit, pas au strict usage.
- **ClickHouse Cloud** — même raison, plus une dépendance directe à un
  fournisseur précis, contraire à l'esprit de l'ADR-0002.
