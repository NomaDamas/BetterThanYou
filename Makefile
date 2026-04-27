.PHONY: help install build run check clean clean-cache size deploy-worker

help:
	@echo "Targets:"
	@echo "  install        cargo install --path . (recommended; builds in temp dir, no project pollution)"
	@echo "  build          cargo build --release (devs only; fills target/ in project)"
	@echo "  run            cargo run --release"
	@echo "  check          cargo check"
	@echo "  clean          cargo clean (removes target/)"
	@echo "  clean-cache    full disk reclaim — target/, node_modules/, .omx/, old reports"
	@echo "  size           show project disk usage"
	@echo "  deploy-worker  push the Cloudflare Worker (cd infra/cloudflare && wrangler deploy)"

install:
	cargo install --path .

build:
	cargo build --release

run:
	cargo run --release

check:
	cargo check

clean:
	cargo clean

clean-cache:
	bash scripts/clean-cache.sh

size:
	@du -sh . 2>/dev/null
	@du -sh ./* ./.* 2>/dev/null | sort -hr | head -8

deploy-worker:
	cd infra/cloudflare && npm install --no-audit --no-fund && npx wrangler deploy
