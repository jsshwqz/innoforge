param(
    [string]$Owner = "jsshwqz",
    [string]$Repo = "innoforge",
    [string]$Tag = "",
    [switch]$RequireForgeRecord,
    [string]$ForgeRecord = ""
)

$ErrorActionPreference = "Stop"

function Write-Check([string]$name, [bool]$ok, [string]$detail) {
    $flag = if ($ok) { "PASS" } else { "FAIL" }
    Write-Host ("[{0}] {1} - {2}" -f $flag, $name, $detail)
}

function Test-ForgeRecord([string]$RecordFile) {
    if (!(Test-Path $RecordFile)) {
        Write-Check "forge record exists" $false $RecordFile
        return $false
    }

    $content = Get-Content -Raw -Encoding UTF8 $RecordFile
    $requiredHeaders = @(
        "## Task Goal",
        "## Forge Calls",
        "## Execution Decision",
        "## Verification"
    )
    foreach ($h in $requiredHeaders) {
        if ($content -notmatch [regex]::Escape($h)) {
            Write-Check "forge header" $false $h
            return $false
        }
    }

    $forgeSection = ($content -split "## Forge Calls", 2)[1]
    if ([string]::IsNullOrWhiteSpace($forgeSection)) {
        Write-Check "forge calls section" $false "empty"
        return $false
    }
    if ($forgeSection -notmatch "Output Summary") {
        Write-Check "forge output summary" $false "missing Output Summary"
        return $false
    }

    Write-Check "forge record" $true $RecordFile
    return $true
}

if ($RequireForgeRecord) {
    Write-Host "== Forge 输出门禁检查 =="
    if ([string]::IsNullOrWhiteSpace($ForgeRecord)) {
        Write-Check "ForgeRecord param" $false "missing -ForgeRecord"
        exit 1
    }
    $ok = Test-ForgeRecord $ForgeRecord
    if (-not $ok) { exit 1 }
    Write-Host ""
}

Write-Host "== 本地静态检查 =="
cargo fmt --all
Write-Check "cargo fmt" ($LASTEXITCODE -eq 0) "格式检查"
if ($LASTEXITCODE -ne 0) { exit 1 }

cargo test --tests
Write-Check "cargo test --tests" ($LASTEXITCODE -eq 0) "测试检查"
if ($LASTEXITCODE -ne 0) { exit 1 }

cargo clippy --tests -- -D warnings
Write-Check "cargo clippy" ($LASTEXITCODE -eq 0) "静态检查"
if ($LASTEXITCODE -ne 0) { exit 1 }

Write-Host ""
Write-Host "== 双端一致性检查 =="
$head = (git rev-parse HEAD).Trim()
$gh = (git ls-remote origin refs/heads/main).Split("`t")[0]
$ge = (git ls-remote gitee refs/heads/main).Split("`t")[0]
Write-Check "HEAD=origin/main" ($head -eq $gh) "$head"
Write-Check "HEAD=gitee/main" ($head -eq $ge) "$head"

if ($Tag -ne "") {
    Write-Host ""
    Write-Host "== Release 资产检查 =="
    $url = "https://api.github.com/repos/$Owner/$Repo/releases/tags/$Tag"
    $rel = Invoke-RestMethod -Uri $url
    $assetCount = @($rel.assets).Count
    Write-Check "GitHub Release Assets" ($assetCount -ge 5) "tag=$Tag, assets=$assetCount"
}

Write-Host ""
Write-Host "发布前自动核验完成。"
