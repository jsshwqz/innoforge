# Patent Hub Self-Review Script
# Auto-runs after idle (no git commits for 30+ min)
# Usage: powershell -ExecutionPolicy Bypass -File scripts\self-review.ps1

param(
    [switch]$Force,
    [switch]$SkipBuild,
    [int]$IdleMinutes = 30
)

$ProjectRoot = Split-Path $PSScriptRoot -Parent
$ResultsFile = Join-Path $ProjectRoot "docs\self-review\results.md"
$LogFile     = Join-Path $ProjectRoot "docs\self-review\last-run.log"
$ServerPort  = 3000
$Timestamp   = Get-Date -Format "yyyy-MM-dd HH:mm:ss"

Set-Location $ProjectRoot

# 1. Idle Detection
if (-not $Force) {
    $lastCommitTs = git log -1 --format="%ct" 2>$null
    if ($lastCommitTs) {
        $nowTs = [int](Get-Date -UFormat %s)
        $minutesAgo = [math]::Floor(($nowTs - [int]$lastCommitTs) / 60)
        if ($minutesAgo -lt $IdleMinutes) {
            Write-Host "[$Timestamp] Git activity ${minutesAgo}min ago (threshold: ${IdleMinutes}min). Skipping."
            exit 0
        }
    }
    Write-Host "[$Timestamp] Idle for 30+ min. Starting self-review..."
}

# 2. Collect results
$lines = [System.Collections.Generic.List[string]]::new()
$passCount = 0; $warnCount = 0; $failCount = 0
$issueLines = [System.Collections.Generic.List[string]]::new()

function Write-Row {
    param([string]$Label, [string]$Status, [string]$Note = "")
    $script:lines.Add("| $Label | $Status | $Note |")
    if ($Status -match "OK")   { $script:passCount++ }
    if ($Status -match "WARN") { $script:warnCount++ }
    if ($Status -match "FAIL") { $script:failCount++ }
}

$lines.Add("# Patent Hub Self-Review Report")
$lines.Add("")
$lines.Add("**Time**: $Timestamp")

$cargoVersion = ""
$m = [regex]::Match((Get-Content Cargo.toml -Raw), 'version\s*=\s*"([^"]+)"')
if ($m.Success) { $cargoVersion = $m.Groups[1].Value }
$lines.Add("**Version**: v$cargoVersion")
$lines.Add("")
$lines.Add("---")
$lines.Add("")
$lines.Add("## Results")
$lines.Add("")
$lines.Add("| Check | Status | Notes |")
$lines.Add("|-------|--------|-------|")

# 3. Build check
if (-not $SkipBuild) {
    Write-Host "[1/6] cargo check..." -NoNewline
    $out = cargo check 2>&1
    if ($LASTEXITCODE -eq 0) {
        Write-Row "Build (cargo check)" "[OK]"
        Write-Host " OK"
    } else {
        $n = ($out | Where-Object { $_ -match "^error" }).Count
        Write-Row "Build (cargo check)" "[FAIL]" "$n errors"
        $issueLines.Add("### FAIL: cargo check ($n errors)")
        $issueLines.Add('```')
        $out | Select-Object -First 20 | ForEach-Object { $issueLines.Add($_) }
        $issueLines.Add('```')
        Write-Host " FAIL ($n errors)"
    }

    Write-Host "[2/6] cargo clippy..." -NoNewline
    $out2 = cargo clippy -- -D warnings 2>&1
    if ($LASTEXITCODE -eq 0) {
        Write-Row "Linting (cargo clippy)" "[OK]" "Zero warnings"
        Write-Host " OK"
    } else {
        $n = ($out2 | Where-Object { $_ -match "^warning" }).Count
        Write-Row "Linting (cargo clippy)" "[WARN]" "$n warnings"
        $issueLines.Add("### WARN: clippy ($n warnings)")
        $out2 | Where-Object { $_ -match "^warning" } | Select-Object -First 10 | ForEach-Object { $issueLines.Add("- $_") }
        Write-Host " WARN ($n)"
    }

    Write-Host "[3/6] cargo test..." -NoNewline
    $out3 = cargo test 2>&1
    $passed = ($out3 | Where-Object { $_ -match " ok$" }).Count
    $failed = ($out3 | Where-Object { $_ -match "FAILED" }).Count
    if ($LASTEXITCODE -eq 0) {
        Write-Row "Tests (cargo test)" "[OK]" "$passed passed"
        Write-Host " OK ($passed passed)"
    } else {
        Write-Row "Tests (cargo test)" "[FAIL]" "$failed failed / $passed passed"
        $issueLines.Add("### FAIL: tests ($failed failed)")
        $out3 | Where-Object { $_ -match "FAILED" } | ForEach-Object { $issueLines.Add("- $_") }
        Write-Host " FAIL ($failed)"
    }
} else {
    Write-Row "Build/Test" "[SKIP]" "-SkipBuild flag set"
    Write-Host "[1-3/6] Skipped (SkipBuild)"
}

# 4. Version consistency
Write-Host "[4/6] Version check..." -NoNewline
$clContent = Get-Content CHANGELOG.md -Raw
$clMatch = [regex]::Match($clContent, '\[v([^\]]+)\]')
$clVersion = if ($clMatch.Success) { $clMatch.Groups[1].Value } else { "?" }

if ($cargoVersion -eq $clVersion) {
    Write-Row "Version consistency" "[OK]" "v$cargoVersion"
    Write-Host " OK (v$cargoVersion)"
} else {
    Write-Row "Version consistency" "[WARN]" "Cargo=v$cargoVersion CHANGELOG=v$clVersion"
    $issueLines.Add("### WARN: Version mismatch - Cargo.toml=$cargoVersion, CHANGELOG.md=$clVersion")
    Write-Host " WARN"
}

# 5. Route integrity
Write-Host "[5/6] Route integrity..." -NoNewline
$mainContent = Get-Content "src\main.rs" -Raw
$routeFiles  = Get-ChildItem "src\routes\*.rs" -Exclude "mod.rs"
$missing = [System.Collections.Generic.List[string]]::new()

foreach ($f in $routeFiles) {
    $fc = Get-Content $f.FullName -Raw
    $handlers = [regex]::Matches($fc, 'pub async fn (api_\w+)') | ForEach-Object { $_.Groups[1].Value }
    foreach ($h in $handlers) {
        if ($mainContent -notmatch [regex]::Escape($h)) {
            $missing.Add("$h (in $($f.Name))")
        }
    }
}

if ($missing.Count -eq 0) {
    Write-Row "Route integrity" "[OK]"
    Write-Host " OK"
} else {
    Write-Row "Route integrity" "[WARN]" "$($missing.Count) unregistered"
    $issueLines.Add("### WARN: Unregistered route handlers")
    $missing | ForEach-Object { $issueLines.Add("- $_") }
    Write-Host " WARN ($($missing.Count) unregistered)"
}

# 6. Server connectivity
Write-Host "[6/6] Server check..." -NoNewline
$alive = $false
try {
    $r = Invoke-WebRequest -Uri "http://127.0.0.1:$ServerPort/" -TimeoutSec 3 -ErrorAction Stop
    $alive = ($r.StatusCode -eq 200)
} catch {}

if ($alive) {
    Write-Row "Server running" "[OK]" "http://127.0.0.1:$ServerPort"
    Write-Host " OK (running)"

    $apis = @("/api/settings", "/api/idea/list", "/api/ipc/tree")
    foreach ($api in $apis) {
        try {
            $ar = Invoke-WebRequest -Uri "http://127.0.0.1:$ServerPort$api" -TimeoutSec 5 -ErrorAction Stop
            Write-Row "API $api" "[OK]" "HTTP $($ar.StatusCode)"
        } catch {
            Write-Row "API $api" "[WARN]" "No response"
            $issueLines.Add("### WARN: API not responding: $api")
        }
    }
} else {
    Write-Row "Server running" "[INFO]" "Not running - start with start-patent-hub.bat"
    Write-Host " (not running)"
}

# 7. Write report
$lines.Add("")
$lines.Add("---")
$lines.Add("")
$lines.Add("## Summary")
$lines.Add("")
$lines.Add("- **[OK]**: $passCount")
$lines.Add("- **[WARN]**: $warnCount")
$lines.Add("- **[FAIL]**: $failCount")

if ($issueLines.Count -gt 0) {
    $lines.Add("")
    $lines.Add("## Issues Detail")
    $lines.Add("")
    $issueLines | ForEach-Object { $lines.Add($_) }
}

$lines.Add("")
$lines.Add("---")
$lines.Add("*Auto-generated by Patent Hub self-review script*")

$lines | Out-File -FilePath $ResultsFile -Encoding UTF8
"[$Timestamp] Review done: $passCount OK, $warnCount WARN, $failCount FAIL" | Out-File -FilePath $LogFile -Encoding UTF8 -Append

Write-Host ""
Write-Host "================================"
Write-Host " Self-Review Complete"
Write-Host " OK=$passCount  WARN=$warnCount  FAIL=$failCount"
Write-Host " Report: $ResultsFile"
Write-Host "================================"

exit $failCount
