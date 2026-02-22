default:
    @just --list

# Build release binaries
build:
    bash -lc 'source ./scripts/common.sh; rust_chatbot_build_release "$PWD"'
    bash -lc 'source ./scripts/common.sh; rust_chatbot_maybe_build_forge_compat_release'

# Build ship (fully optimized) binaries
ship:
    bash -lc 'source ./scripts/common.sh; CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$(rust_chatbot_target_dir "$PWD")}" rust_chatbot_repo_cargo "$PWD" build --profile ship'

# Install binaries system-wide
install:
    bash -lc 'source ./scripts/common.sh; rust_chatbot_maybe_build_forge_compat_release'
    bash -lc 'source ./scripts/common.sh; CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$(rust_chatbot_target_dir "$PWD")}" rust_chatbot_build_release "$PWD" && rust_chatbot_install_release_binaries "$PWD" "${CARGO_INSTALL_ROOT:-$HOME/.local}"'

# Run tests
test:
    bash -lc 'source ./scripts/common.sh; rust_chatbot_repo_cargo "$PWD" test --lib -- --test-threads=1'

# Run tests with coverage
coverage:
    bash -lc 'source ./scripts/common.sh; rust_chatbot_repo_cargo "$PWD" llvm-cov --lib --lcov -- --test-threads=1'

# Check coverage meets threshold (80%)
coverage-check:
    bash -lc 'source ./scripts/common.sh; rust_chatbot_repo_cargo "$PWD" llvm-cov --lib --summary-only -- --test-threads=1'

# Launch a provider (default: claude)
launch provider="claude":
    ./scripts/launch.sh {{provider}}

# Run a single provider instance (provider: claude/gemini/codex, instance: 1+)
run provider="claude" instance="1":
    ./scripts/run_one.sh {{provider}} {{instance}}

# Build and start default multi-provider profile (claude:1, codex:1, gemini:1)
start:
    ./scripts/start-all.sh

# Build and start a single provider instance (provider: claude/gemini/codex/minimax, instance: 1+)
start-one provider="claude" instance="1":
    bash -lc 'source ./scripts/common.sh; CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$(rust_chatbot_target_dir "$PWD")}" rust_chatbot_repo_cargo "$PWD" build --release' && ./scripts/run_one.sh {{provider}} {{instance}}

# Build and start default multi-provider profile (claude:1, codex:1, gemini:1)
start-all:
    ./scripts/start-all.sh

# Stop all running chatbot instances and broker
stop-all:
    ./scripts/stop-all.sh

# Restart all running instances
restart-all:
    ./scripts/restart-all.sh

# Restart a single provider instance (provider: claude/gemini/codex/minimax)
restart provider="claude":
    ./scripts/restart-provider.sh {{provider}}

# Back up LLM data, logs, and sessions
backup:
    ./scripts/backup-llm-data.sh

# Cycle codex provider
cycle:
    ./scripts/cycle.sh

# Test Claude CLI integration
test-claude:
    ./scripts/test-claude.sh
