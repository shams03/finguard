.PHONY: dev-db db-down migrate backend frontend test test-live lint reset-demo
dev-db:
	docker compose up -d --wait postgres
db-down:
	docker compose down
migrate:
	cargo run -q -p finguard-api -- migrate
backend:
	cargo run -p finguard-api
frontend:
	cd apps/web && npm run dev
test:
	cargo test --workspace
test-live:
	cargo test -p finguard-jev -p finguard-groq -- --ignored --nocapture
lint:
	cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings
reset-demo:
	curl -s -X POST localhost:8080/api/v1/demo/reset
