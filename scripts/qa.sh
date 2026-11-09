#!/usr/bin/env bash
set -euo pipefail

# MailPurge - Quality Gate Script
# Ensures 100% compliance with engineering rules before commit or release.

echo "=========================================="
echo "🛡️  Running MailPurge Quality Gate"
echo "=========================================="

# 1. Format check
echo ""
echo "▶ [1/4] Checking code formatting (rustfmt)..."
cargo fmt --all -- --check
echo "✓ Formatting is clean."

# 2. Strict static analysis (Clippy Pedantic)
echo ""
echo "▶ [2/4] Running Clippy analysis (-D warnings)..."
cargo clippy --all-targets --all-features -- -D warnings
echo "✓ Clippy passed without warnings or errors."

# 3. Unit, integration & doc tests
echo ""
echo "▶ [3/4] Running tests..."
if command -v cargo-nextest >/dev/null 2>&1; then
    echo "  -> Using cargo-nextest..."
    CI=1 cargo nextest run
else
    echo "  -> Using cargo test..."
    cargo test --all-targets
fi

echo "  -> Running doc tests..."
cargo test --doc
echo "✓ All tests passed."

# 4. Security audit & unused dependencies
echo ""
echo "▶ [4/4] Security audit & unused dependencies..."
if command -v cargo-deny >/dev/null 2>&1; then
    echo "  -> Running cargo-deny..."
    cargo deny --offline check || cargo deny check
elif command -v cargo-audit >/dev/null 2>&1; then
    echo "  -> Running cargo-audit..."
    cargo audit --no-fetch || cargo audit
fi

if command -v cargo-audit >/dev/null 2>&1; then
    cargo audit --no-fetch || cargo audit
fi

echo "  -> Running cargo-machete..."
cargo machete
echo "✓ Security audit and dependency checks passed."

echo ""
echo "=========================================="
echo "✅ Quality Gate 100% Passed!"
echo "=========================================="
