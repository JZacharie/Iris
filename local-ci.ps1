$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
if (-not $env:CARGO_TARGET_DIR) {
    $env:CARGO_TARGET_DIR = "D:\cargo-targets\iris"
}

function Invoke-Step {
    param([string]$Title, [scriptblock]$Cmd)
    Write-Host "==> $Title" -ForegroundColor Cyan
    & $Cmd
    if ($LASTEXITCODE -ne 0) {
        throw "Step failed ($Title) with exit code $LASTEXITCODE"
    }
}

Invoke-Step "[1/4] cargo fmt --check" { cargo fmt --all -- --check }
Invoke-Step "[2/4] cargo clippy (all-targets)" { cargo clippy --workspace --all-targets -- -D warnings }
Invoke-Step "[3/4] cargo clippy (no unwrap/expect)" { cargo clippy --workspace --lib --bins -- -D warnings -D clippy::unwrap_used -D clippy::expect_used }
Invoke-Step "[4/4] cargo test & build" {
    cargo test --workspace
    if ($LASTEXITCODE -eq 0) {
        cargo build --bin iris-server
    }
}

Write-Host "ALL CHECKS PASSED" -ForegroundColor Green
