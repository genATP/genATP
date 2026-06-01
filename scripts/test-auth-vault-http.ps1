# genATP - Phase 6 auth vault smoke test over HTTP MCP (no MCP client).
#
# Usage:
#   Terminal A:  cargo run --release --bin genatp -- serve --port 3000 --headed
#   Terminal B:  .\scripts\test-auth-vault-http.ps1
#
# Optional: copy .env.example to .env, then run with -LoadEnv

param(
    [string]$BaseUrl = "http://127.0.0.1:3000/",
    [string]$LoginUrl = "https://mibbs.digitalfortress.in/login",
    [string]$VerifyUrl = "https://mibbs.digitalfortress.in/",
    [string]$Domain = "mibbs.digitalfortress.in",
    [switch]$LoadEnv,
    [switch]$SkipNavigate,
    [switch]$SaveOnly,
    [switch]$LoadOnly
)

$ErrorActionPreference = "Stop"

function Import-DotEnv {
    param([string]$Path = ".env")
    if (-not (Test-Path $Path)) {
        Write-Warning "No $Path found - skipping env load."
        return
    }
    Get-Content $Path | ForEach-Object {
        $line = $_.Trim()
        if ($line -eq "" -or $line.StartsWith("#")) { return }
        if ($line -match '^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$') {
            $name = $matches[1]
            $val = $matches[2].Trim().Trim('"').Trim("'")
            Set-Item -Path "env:$name" -Value $val
            Write-Host "  env: $name"
        }
    }
}

function Invoke-Mcp {
    param(
        [int]$Id,
        [string]$Method,
        [hashtable]$Params = @{}
    )
    $body = @{
        jsonrpc = "2.0"
        id      = $Id
        method  = $Method
    }
    if ($Params.Count -gt 0) {
        $body.params = $Params
    }
    $json = $body | ConvertTo-Json -Depth 10 -Compress
    $headers = @{ "content-type" = "application/json" }
    $resp = Invoke-RestMethod -Method Post -Uri $BaseUrl -Headers $headers -Body $json
    return $resp
}

function Invoke-Tool {
    param(
        [int]$Id,
        [string]$Name,
        [hashtable]$Arguments = @{}
    )
    $resp = Invoke-Mcp -Id $Id -Method "tools/call" -Params @{
        name      = $Name
        arguments = $Arguments
    }
    $text = $resp.result.content[0].text
    $isErr = $resp.result.isError
    Write-Host "[$Name] isError=$isErr"
    Write-Host $text
    if ($isErr) { throw "tool $Name failed" }
    return $text
}

if ($LoadEnv) {
    Write-Host "Loading .env ..."
    Import-DotEnv
}

Write-Host "Vault path (CLI):"
try {
    & cargo run --quiet --bin genatp -- vault path 2>$null
} catch {
    Write-Warning "Could not run genatp vault path (build first?)"
}

Write-Host ""
Write-Host "--- MCP initialize ---"
$init = Invoke-Mcp -Id 1 -Method "initialize" -Params @{
    protocolVersion = "2024-11-05"
    capabilities    = @{}
    clientInfo      = @{ name = "auth-vault-test"; version = "0.1" }
}
Write-Host "server:" $init.result.serverInfo.name $init.result.serverInfo.version

try {
    $notify = @{ jsonrpc = "2.0"; method = "notifications/initialized"; params = @{} } | ConvertTo-Json -Compress
    Invoke-RestMethod -Method Post -Uri $BaseUrl -Headers @{ "content-type" = "application/json" } -Body $notify | Out-Null
} catch { }

Write-Host ""
Write-Host "--- tools/list ---"
$list = Invoke-Mcp -Id 2 -Method "tools/list"
$tools = $list.result.tools | ForEach-Object { $_.name }
Write-Host "tools:" ($tools -join ", ")
if ("save_auth" -notin $tools) {
    throw "save_auth missing - rebuild genatp (Phase 6)?"
}

if (-not $LoadOnly) {
    if (-not $SkipNavigate) {
        Write-Host ""
        Write-Host "--- navigate (headed browser should open) ---"
        Invoke-Tool -Id 3 -Name "navigate" -Arguments @{ url = $LoginUrl }
        if (-not $SaveOnly) {
            Write-Host ""
            Write-Host "Log in manually in the browser, then press Enter to save_auth..."
            Read-Host
        }
    }
    if ($SaveOnly -or -not $LoadOnly) {
        Write-Host ""
        Write-Host "--- save_auth ---"
        $saved = Invoke-Tool -Id 4 -Name "save_auth" -Arguments @{ domain = $Domain }
        if ($saved -match "saved 0 auth") {
            Write-Warning "save_auth stored 0 entries. Try domain digitalfortress.in, log in first, then: cargo run --bin genatp -- vault list"
        }
    }
}

if ($LoadOnly) {
    Write-Host ""
    Write-Host "--- load_auth (run after restart + navigate) ---"
    if (-not $SkipNavigate) {
        Invoke-Tool -Id 5 -Name "navigate" -Arguments @{ url = $LoginUrl }
    }
    Invoke-Tool -Id 6 -Name "load_auth" -Arguments @{ domain = $Domain }

    # The login page rarely auto-redirects authenticated users.
    # Navigate to a protected URL to confirm the session is restored.
    Write-Host ""
    Write-Host "--- navigate to verify URL ($VerifyUrl) ---"
    Invoke-Tool -Id 7 -Name "navigate" -Arguments @{ url = $VerifyUrl }

    Write-Host ""
    Write-Host "Check the browser at $VerifyUrl - you should be logged in."
    Write-Host "If you still see the login form, see DIAGNOSTICS at the bottom of docs/TESTING.md."
}

Write-Host ""
Write-Host "Done. Verify vault: cargo run --bin genatp -- vault list"
