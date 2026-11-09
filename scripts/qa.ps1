# MailPurge - Quality Gate Script (PowerShell)
# Requires strict error handling
$ErrorActionPreference = "Stop"

Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "🛡️  Running MailPurge Quality Gate" -ForegroundColor Cyan
Write-Host "==========================================" -ForegroundColor Cyan

# 1. Format check
Write-Host "`n▶ [1/4] Checking code formatting (rustfmt)..." -ForegroundColor Yellow
cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Host "✓ Formatting is clean." -ForegroundColor Green

# 2. Strict static analysis (Clippy Pedantic)
Write-Host "`n▶ [2/4] Running Clippy analysis (-D warnings)..." -ForegroundColor Yellow
cargo clippy --all-targets --all-features -- -D warnings
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Host "✓ Clippy passed without warnings or errors." -ForegroundColor Green

# 3. Unit, integration & doc tests
Write-Host "`n▶ [3/4] Running tests..." -ForegroundColor Yellow
if (Get-Command cargo-nextest -ErrorAction SilentlyContinue) {
    Write-Host "  -> Using cargo-nextest..."
    $env:CI = "1"
    cargo nextest run
} else {
    Write-Host "  -> Using cargo test..."
    cargo test --all-targets
}
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "  -> Running doc tests..."
cargo test --doc
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Host "✓ All tests passed." -ForegroundColor Green

# 4. Security audit & unused dependencies
Write-Host "`n▶ [4/4] Security audit & unused dependencies..." -ForegroundColor Yellow
if (Get-Command cargo-deny -ErrorAction SilentlyContinue) {
    Write-Host "  -> Running cargo-deny..."
    cargo deny --offline check
} elseif (Get-Command cargo-audit -ErrorAction SilentlyContinue) {
    Write-Host "  -> Running cargo-audit..."
    cargo audit --no-fetch
}

if (Get-Command cargo-machete -ErrorAction SilentlyContinue) {
    Write-Host "  -> Running cargo-machete..."
    cargo machete
}
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "`n==========================================" -ForegroundColor Green
Write-Host "✅ Quality Gate 100% Passed!" -ForegroundColor Green
Write-Host "==========================================" -ForegroundColor Green
