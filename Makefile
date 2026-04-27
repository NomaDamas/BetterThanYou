.PHONY: help install build run check clean clean-cache size deploy-worker install-shell-hook

# Redirect Rust build artifacts OUTSIDE the project so the project dir stays
# slim regardless of how often you build. All cargo invocations made through
# `make` write into ~/.cache/cargo-target/better-than-you instead of ./target.
# To apply this to plain `cargo` calls too, run: `make install-shell-hook`
# (adds the env var to ~/.zshrc) — or set it manually:
#     export CARGO_TARGET_DIR="$HOME/.cache/cargo-target"
export CARGO_TARGET_DIR := $(HOME)/.cache/cargo-target/better-than-you

help:
	@echo "Targets:"
	@echo "  install             cargo install --path . — drops binary into ~/.cargo/bin"
	@echo "  build               cargo build --release — artifacts go to \$$CARGO_TARGET_DIR"
	@echo "  run                 cargo run --release"
	@echo "  check               cargo check"
	@echo "  clean               cargo clean (wipes \$$CARGO_TARGET_DIR for this project)"
	@echo "  clean-cache         full disk reclaim — target/, node_modules/, .omx/, old reports"
	@echo "  size                show project disk usage"
	@echo "  deploy-worker       push the Cloudflare Worker"
	@echo "  install-shell-hook  add CARGO_TARGET_DIR export to ~/.zshrc (one-time, applies globally)"
	@echo ""
	@echo "Build cache lives at: \$$CARGO_TARGET_DIR=$(CARGO_TARGET_DIR)"

install:
	cargo install --path . --force

build:
	cargo build --release

run:
	cargo run --release

check:
	cargo check

clean:
	cargo clean
	@# Belt-and-suspenders: also drop any project-local target/ left from
	@# direct cargo invocations that bypassed CARGO_TARGET_DIR.
	@rm -rf ./target

clean-cache:
	bash scripts/clean-cache.sh

size:
	@du -sh . 2>/dev/null
	@du -sh ./* ./.* 2>/dev/null | sort -hr | head -8

deploy-worker:
	cd infra/cloudflare && npm install --no-audit --no-fund && npx wrangler deploy

install-shell-hook:
	@if grep -q 'CARGO_TARGET_DIR' ~/.zshrc 2>/dev/null; then \
		echo "CARGO_TARGET_DIR already set in ~/.zshrc — leaving it alone."; \
	else \
		echo '' >> ~/.zshrc; \
		echo '# BetterThanYou: keep cargo build artifacts out of project dirs' >> ~/.zshrc; \
		echo 'export CARGO_TARGET_DIR="$$HOME/.cache/cargo-target"' >> ~/.zshrc; \
		echo "Added CARGO_TARGET_DIR export to ~/.zshrc."; \
		echo "Run \`source ~/.zshrc\` (or open a new terminal) to apply."; \
	fi
