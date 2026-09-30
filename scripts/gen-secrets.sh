#!/usr/bin/env bash
# Generates real tokens and a password for docker/.env.prod: never reuse
# the dev-* values of docker-compose.stack.yml in production
# (see docs/interfaces/kernel-auth.md).
# Usage: scripts/gen-secrets.sh
set -euo pipefail

echo "KERNEL_API_KEY=$(openssl rand -hex 32)"
echo "QUERY_API_KEY=$(openssl rand -hex 32)"
echo "CLICKHOUSE_PASSWORD=$(openssl rand -hex 24)"
