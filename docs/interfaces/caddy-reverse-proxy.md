# Contract: Caddy as the TLS reverse proxy in front of kernel/query-api

**Source**: the official Caddy documentation (caddyserver.com/docs), read on
2026-08-20: `reverse_proxy` (Caddyfile syntax), `automatic-https`,
`caddyfile/concepts` (environment variable substitution, global options block).
Not from memory: this was the project's first contact with Caddy.

## Docker image

`caddy:2.11.4`, pinned to an exact version, never `:latest` in production (the
official Docker Hub recommendation).

## `reverse_proxy`: HTTP vs cleartext gRPC (h2c)

- `query-api` (plain HTTP JSON): `reverse_proxy query-api:8080` is enough.
- `kernel` (gRPC/OTLP, served by `tonic` without TLS inside the container; TLS is
  terminated by Caddy, not by `tonic`) explicitly needs the `h2c://` scheme:
  `reverse_proxy h2c://kernel:4317`. Without that prefix Caddy would talk
  HTTP/1.1 upstream and break gRPC (which requires HTTP/2). Checked in the
  `reverse_proxy` docs: `h2c://` explicitly enables the HTTP transport with
  cleartext HTTP/2 allowed upstream.

## Automatic HTTPS

Triggers implicitly as soon as a site block has a real domain name as its
address (no explicit directive needed). Production prerequisites:
- the domain's DNS pointed at the VPS's IP **before** Caddy first starts
  (otherwise the ACME challenge fails);
- ports 80 **and** 443 reachable from outside (80 serves the HTTP-01 challenge,
  then redirects to 443);
- the container's `/data` mounted on a persistent volume: that is where the
  certificates live. Without a volume, a simple container restart would lose the
  certificates and trigger a new ACME challenge (watch the Let's Encrypt rate
  limit if that happens too often).

## Environment variables in the Caddyfile

Verified syntax: `{$VARIABLE}` or `{$VARIABLE:default}`, substituted before the
Caddyfile is even parsed, no flag required. It works directly as a site block
address (`{$API_HOST} { ... }`), which keeps the domain name out of the
Caddyfile itself (in `.env`, like the other secrets).

## Global options block

Must be the very first block of the file, with an empty address (`{ ... }`). It
sets `email {$ACME_EMAIL}` once for the whole file (the contact address Let's
Encrypt uses to warn about an ACME account expiring, not used for anything
else).

## Decision: not yet part of `crates/orchestrator`

Unlike `clickhouse`/`kernel`/`query-api` (managed by
`docker_client::deploy_all`), Caddy is added through a separate compose file
(`docker/docker-compose.prod.yml`), not as a fourth `ManagedService`. Reason:
`ManagedService` does not model volumes or per-service environment variables
today (only `ports`/`depends_on`/`image_source`). Extending it for Caddy alone
would have cost more than its immediate value, and a separate compose file for
the edge/TLS layer is a common pattern. An open question, not settled here: if
`crates/orchestrator` takes on a future fourth service, generalise
`ManagedService` then rather than in advance.
