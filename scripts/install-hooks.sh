#!/usr/bin/env bash
set -euo pipefail

# Installs the pre-commit Git hook for MailPurge
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOOK_DIR="${REPO_ROOT}/.git/hooks"
PRE_COMMIT="${HOOK_DIR}/pre-commit"

if [ ! -d "${HOOK_DIR}" ]; then
    echo "❌ Error: .git/hooks directory not found. Are you in the root of the Git repository?"
    exit 1
fi

echo "Installing pre-commit hook into ${PRE_COMMIT}..."

cat > "${PRE_COMMIT}" << 'EOF'
#!/usr/bin/env bash
set -euo pipefail

echo "🔍 Running pre-commit Quality Gate..."
REPO_ROOT="$(git rev-parse --show-toplevel)"

if command -v just >/dev/null 2>&1; then
    just qa
elif [ -f "${REPO_ROOT}/scripts/qa.sh" ]; then
    "${REPO_ROOT}/scripts/qa.sh"
else
    echo "Running fallback QA checks..."
    cargo fmt --all -- --check
    cargo clippy --all-targets --all-features -- -D warnings
    cargo test --all-targets
fi

echo "✅ Pre-commit hook passed."
EOF

chmod +x "${PRE_COMMIT}"
chmod +x "${REPO_ROOT}/scripts/qa.sh"

echo "✅ Pre-commit hook successfully installed and configured!"
