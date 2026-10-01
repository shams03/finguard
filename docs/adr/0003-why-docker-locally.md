# ADR 3: Why Docker locally

Status: accepted (MVP)

## Context
Contributors must be able to run everything with no cloud account.

## Decision
docker-compose runs postgres:18-alpine. `docker compose up -d` + `make migrate` is the whole setup.

## Consequences
Needs Docker Desktop; local and production use the same SQL schema.
