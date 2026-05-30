# genATP preflight — run before every commit/push to match CI gates locally.
# Usage:  .\scripts\preflight.ps1
#         .\scripts\preflight.ps1 -Quick    # skip integration tests (no Chrome)
#
# Exit code 0 = safe to commit. Non-zero = fix reported failures first.

param(
    [switch]$Quick
)

$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $Root

# Ensure cargo is on PATH (Windows Git hooks often omit ~/.cargo/bin).
$CargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if (Test-Path $CargoBin) {
    $env:Path = "$CargoBin;$env:Path"
}

function Step($Name, [scriptblock]$Action) {
    Write-Host ""
    Write-Host "==> $Name" -ForegroundColor Cyan
    # Cargo writes progress to stderr; do not treat that as a PowerShell error.
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    & $Action 2>&1 | ForEach-Object { $_ }
    $code = if ($null -ne $LASTEXITCODE) { $LASTEXITCODE } else { 0 }
    $ErrorActionPreference = $prevEap
    if ($code -ne 0) {
        Write-Host "FAILED: $Name (exit $code)" -ForegroundColor Red
        exit $code
    }
    Write-Host "OK: $Name" -ForegroundColor Green
}

Step "cargo fmt (apply)" { cargo fmt --all }
Step "cargo fmt --check" { cargo fmt --all -- --check }
Step "cargo clippy" {
    $env:RUSTFLAGS = "-D warnings"
    cargo clippy --workspace --exclude genatp-python --all-targets -- -D warnings
}
if (-not $Quick) {
    Step "cargo test --workspace" { cargo test --workspace --exclude genatp-python }
} else {
    Step "cargo test (unit only, -Quick)" { cargo test --workspace --exclude genatp-python --lib }
}
Step "cargo deny check" { cargo deny check }

Write-Host ""
Write-Host "Preflight passed. Safe to commit and push." -ForegroundColor Green
