.PHONY: setup install build clean dev stop kill-port cross-build cross-list

# Source cargo env for all Rust commands
export PATH := $(HOME)/.cargo/bin:$(PATH)
CARGO_ENV := . $(HOME)/.cargo/env &&

# Setup: install all dependencies for frontend and backend
setup:
	@echo "=== Setting up ntd ==="
	@echo ""
	@echo "[1/4] Checking Rust toolchain..."
	@which rustc > /dev/null 2>&1 || (echo "Installing Rust..." && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y)
	@$(CARGO_ENV) echo "  Rust: $$(rustc --version 2>/dev/null || echo 'NOT FOUND')"
	@echo ""
	@echo "[2/4] Checking Node.js..."
	@echo "  Node: $$(node --version 2>/dev/null || echo 'NOT FOUND')"
	@echo "  npm:  $$(npm --version 2>/dev/null || echo 'NOT FOUND')"
	@echo ""
	@echo "[3/4] Installing frontend dependencies..."
	cd frontend && npm install --legacy-peer-deps
	@echo ""
	@echo "[4/4] Pre-compiling Rust backend (downloads deps)..."
	cd backend && $(CARGO_ENV) cargo fetch
	@echo ""
	@echo "[OPT] Installing cross-build tool (cross)..."
	@$(CARGO_ENV) which cross > /dev/null 2>&1 || $(CARGO_ENV) cargo install cross --locked
	@echo ""
	@echo "=== Setup complete! ==="
	@echo "Run 'make install'   to build and install binary to ~/.local/bin"
	@echo "Run 'make dev'       to start development (frontend + backend, port 18088)"
	@echo "Run 'make stop'      to stop dev instance"
	@echo "Run 'make build'     to build for production"
	@echo "Run 'make cross-build' to build for win/mac/linux x86+arm"

# Install the built binary to ~/.local/bin
install:  build
	@mkdir -p $$HOME/.local/bin
	@rm -f $$HOME/.local/bin/ntd
	@cp backend/target/release/ntd $$HOME/.local/bin/
	@echo "Installed to $$HOME/.local/bin/ntd"

# Build frontend and embed into Rust binary
build:
	cd frontend && npm run build
	cd backend && $(CARGO_ENV) cargo build --release

# Clean all build artifacts
clean:
	rm -rf frontend/dist
	# workspace 化后所有 cargo 产物在根 target/；
	# 兼容老布局，backend/target 也清掉。
	rm -rf target backend/target

# Detect OS once for all targets
_UNAME := $(shell uname -s)

# Kill processes on port 18088 (Linux fuser / macOS lsof)
_KILL_PORT_18088 = fuser -k 18088/tcp 2>/dev/null || true
ifeq ($(_UNAME),Darwin)
  _KILL_PORT_18088 = lsof -ti:18088 | xargs kill -9 2>/dev/null || true
endif

kill-port:
	@$(_KILL_PORT_18088)

# Stop the dev instance
stop:
	-@if [ -f ~/.ntd/dev.pid ]; then \
		pid=$$(cat ~/.ntd/dev.pid); \
		kill -9 $$pid 2>/dev/null && echo "Killed dev process $$pid" || echo "Dev process $$pid not running"; \
		rm -f ~/.ntd/dev.pid; \
	fi
	@$(_KILL_PORT_18088)
	@sleep 1

# Development mode - build frontend + build backend + run on port 18088
dev: stop
	@echo "[1/2] Building frontend..."
	cd frontend && npm run build
	@echo "[2/2] Building & running backend..."
	cd backend && $(CARGO_ENV) NTD_MODE=dev RUST_BACKTRACE=1 RUST_LOG=info cargo run -- server start 2>&1 | tee ../backend.dev.log &
	@mkdir -p $$HOME/.ntd && echo $$! > $$HOME/.ntd/dev.pid
	@echo "==========================================="
	@echo "  Dev mode: http://localhost:18088"
	@echo "==========================================="
	@echo "Backend logs: tail -f backend.dev.log"
	@echo ""
	@echo "Press Ctrl+C to stop"

# Cross-build for Windows (x86_64 + i686), macOS (x86_64 + aarch64), Linux (x86_64 + aarch64)
cross-build:
	@echo "=== Cross-building ntd for win/mac/linux x86+arm ==="
	@mkdir -p backend/target/cross
	@echo ""
	@echo "[1/6] Building: x86_64-pc-windows-gnu"
	@cd backend && $(CARGO_ENV) cross build --release --bin ntd --target x86_64-pc-windows-gnu --force-non-host
	@mv backend/target/x86_64-pc-windows-gnu/release/ntd.exe backend/target/cross/ntd-x86_64-pc-windows-gnu.exe
	@echo ""
	@echo "[2/6] Building: i686-pc-windows-gnu"
	@cd backend && $(CARGO_ENV) cross build --release --bin ntd --target i686-pc-windows-gnu --force-non-host
	@mv backend/target/i686-pc-windows-gnu/release/ntd.exe backend/target/cross/ntd-i686-pc-windows-gnu.exe
	@echo ""
	@echo "[3/6] Building: x86_64-apple-darwin"
	@cd backend && $(CARGO_ENV) cross build --release --bin ntd --target x86_64-apple-darwin
	@mv backend/target/x86_64-apple-darwin/release/ntd backend/target/cross/ntd-x86_64-apple-darwin
	@echo ""
	@echo "[4/6] Building: aarch64-apple-darwin"
	@cd backend && $(CARGO_ENV) cross build --release --bin ntd --target aarch64-apple-darwin
	@mv backend/target/aarch64-apple-darwin/release/ntd backend/target/cross/ntd-aarch64-apple-darwin
	@echo ""
	@echo "[5/6] Building: x86_64-unknown-linux-gnu"
	@cd backend && $(CARGO_ENV) cross build --release --bin ntd --target x86_64-unknown-linux-gnu
	@mv backend/target/x86_64-unknown-linux-gnu/release/ntd backend/target/cross/ntd-x86_64-unknown-linux-gnu
	@echo ""
	@echo "[6/6] Building: aarch64-unknown-linux-gnu"
	@cd backend && $(CARGO_ENV) cross build --release --bin ntd --target aarch64-unknown-linux-gnu
	@mv backend/target/aarch64-unknown-linux-gnu/release/ntd backend/target/cross/ntd-aarch64-unknown-linux-gnu
	@echo ""
	@echo "=== Cross-build complete ==="
	@ls -lh backend/target/cross/

# List cross-build targets
cross-list:
	@echo "Cross-build targets:"
	@echo "  Windows:  x86_64-pc-windows-gnu, i686-pc-windows-gnu"
	@echo "  macOS:    x86_64-apple-darwin, aarch64-apple-darwin"
	@echo "  Linux:    x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu"
	@echo ""
	@echo "Built binaries: backend/target/cross/"
