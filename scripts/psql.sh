#!/usr/bin/env bash
# Open psql inside the local Docker Postgres.
exec docker compose exec postgres psql -U finguard -d finguard "$@"
