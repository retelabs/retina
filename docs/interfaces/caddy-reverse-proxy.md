# Contrat : Caddy comme reverse proxy TLS devant kernel/query-api

**Source** : documentation officielle Caddy (caddyserver.com/docs), consultée
le 2026-08-20 — `reverse_proxy` (syntaxe Caddyfile), `automatic-https`,
`caddyfile/concepts` (substitution de variables d'environnement, bloc
d'options globales). Pas depuis la mémoire — première fois que ce projet
touche Caddy.

## Image Docker

`caddy:2.11.4` — épinglée sur une version exacte, jamais `:latest` en
production (recommandation officielle du Docker Hub).

## `reverse_proxy` : HTTP vs gRPC cleartext (h2c)

- `query-api` (HTTP JSON classique) : `reverse_proxy query-api:8080` suffit.
- `kernel` (gRPC/OTLP, servi par `tonic` sans TLS côté conteneur — TLS est
  terminé par Caddy, pas par `tonic`) : nécessite explicitement le schéma
  `h2c://` — `reverse_proxy h2c://kernel:4317`. Sans ce préfixe, Caddy
  parlerait HTTP/1.1 en amont et casserait gRPC (qui exige HTTP/2). Vérifié
  dans la doc `reverse_proxy` : `h2c://` déclenche explicitement le
  transport HTTP avec HTTP/2 en clair autorisé vers l'amont.

## HTTPS automatique

Se déclenche implicitement dès qu'un bloc de site a un vrai nom de domaine
comme adresse (pas besoin de directive explicite). Prérequis en production :
- DNS du domaine pointé vers l'IP du VPS **avant** le premier démarrage de
  Caddy (sinon le challenge ACME échoue) ;
- ports 80 **et** 443 atteignables depuis l'extérieur (80 sert le challenge
  HTTP-01 puis redirige vers 443) ;
- `/data` du conteneur monté sur un volume persistant — c'est là que vivent
  les certificats obtenus ; sans volume, un simple restart de conteneur
  reperdrait les certs et re-déclencherait un nouveau challenge ACME (rate
  limit Let's Encrypt à surveiller si ça arrive trop souvent).

## Variables d'environnement dans le Caddyfile

Syntaxe réelle vérifiée : `{$VARIABLE}` ou `{$VARIABLE:valeur_par_défaut}` —
substituées avant même le parsing du Caddyfile, aucun flag requis. Fonctionne
directement comme adresse de bloc de site (`{$API_HOST} { ... }`), ce qui
permet de garder le nom de domaine hors du Caddyfile lui-même (dans
`.env`, comme `POSTGRES_PASSWORD` chez client-project).

## Bloc d'options globales

Doit être le tout premier bloc du fichier, adresse vide (`{ ... }`) — sert à
poser `email {$ACME_EMAIL}` une seule fois pour tout le fichier (l'email de
contact que Let's Encrypt utilise pour prévenir d'une expiration de compte
ACME, pas un canal exploité autrement).

## Décision : pas encore intégré à `crates/orchestrator`

Contrairement à `clickhouse`/`kernel`/`query-api` (gérés par
`docker_client::deploy_all`), Caddy est ajouté via un compose file séparé
(`docker/docker-compose.prod.yml`), pas comme 4e `ManagedService`. Raison :
`ManagedService` ne modélise aujourd'hui ni volumes ni variables
d'environnement par service (seulement `ports`/`depends_on`/`image_source`) —
l'étendre pour Caddy seul aurait été plus de travail que la valeur
immédiate, et client-project (Traefik) suit déjà ce même schéma "compose
séparé pour la couche edge/TLS". Question ouverte, pas tranchée ici : si
`crates/orchestrator` prend en charge un futur 4e service côté kernel MVP,
généraliser `ManagedService` à ce moment-là plutôt que d'anticiper.
