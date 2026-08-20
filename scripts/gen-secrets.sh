#!/usr/bin/env bash
# Génère des jetons/mot de passe réels pour docker/.env.prod — jamais
# réutiliser les valeurs dev-* de docker-compose.stack.yml en production
# (voir CLAUDE.md, section authentification).
# Usage : scripts/gen-secrets.sh
set -euo pipefail

echo "KERNEL_API_KEY=$(openssl rand -hex 32)"
echo "QUERY_API_KEY=$(openssl rand -hex 32)"
echo "CLICKHOUSE_PASSWORD=$(openssl rand -hex 24)"
