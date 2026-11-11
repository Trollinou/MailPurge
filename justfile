# Justfile - MailPurge Quality Gate automation

# Default recipe: runs full QA validation
default: qa

# Format check across all crates
fmt:
    cargo fmt --all -- --check

# Strict Clippy lint check with warnings treated as errors
lint:
    cargo clippy --all-targets --all-features -- -D warnings

# Run all unit/integration tests with cargo-nextest (falling back to cargo test) followed by doc-tests
test:
    #!/usr/bin/env bash
    set -euo pipefail
    if command -v cargo-nextest >/dev/null 2>&1; then \
        echo "==> Running tests with cargo-nextest..." ; \
        CI=1 cargo nextest run ; \
    else \
        echo "==> cargo-nextest not found, falling back to cargo test..." ; \
        cargo test --all-targets ; \
    fi
    echo "==> Running doc tests..."
    cargo test --doc

# Security audit and unused dependencies check
audit:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "==> Running cargo-deny / cargo-audit..."
    if command -v cargo-deny >/dev/null 2>&1; then \
        cargo deny --offline check || cargo deny check ; \
    elif command -v cargo-audit >/dev/null 2>&1; then \
        cargo audit --no-fetch || cargo audit ; \
    fi
    if command -v cargo-audit >/dev/null 2>&1; then \
        cargo audit --no-fetch || cargo audit ; \
    fi
    echo "==> Checking unused dependencies with cargo-machete..."
    cargo machete

# Full Quality Gate: sequential execution of fmt, lint, test, audit
qa: fmt lint test audit
    @echo ""
    @echo "✅ Quality Gate 100% Passed!"

# Synchronize version from Cargo.toml to package.json and tauri.conf.json
sync-version version="":
    node scripts/sync-version.mjs {{version}}

# Code coverage report generation
coverage:
    cargo llvm-cov --html

# Mutation testing targeting domain modules
mutants:
    cargo mutants --package mailpurge-core --file crates/mailpurge-core/src/domain/retention.rs

# Clean compilation artifacts older than N days (default 7)
sweep days="7":
    @echo "==> Sweeping old compilation artifacts (older than {{days}} days)..."
    cargo sweep --time {{days}}

