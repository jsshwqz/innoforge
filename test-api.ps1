# Patent Hub API Test Script
# Tests all API endpoints and reports results

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Patent Hub API Test Suite" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

$baseUrl = "http://localhost:3000"
$testResults = @()

function Test-Endpoint {
    param(
        [string]$Name,
        [string]$Method,
        [string]$Url,
        [object]$Body = $null
    )
    
    Write-Host "Testing: $Name" -ForegroundColor Yellow
    
    try {
        $params = @{
            Uri = $Url
            Method = $Method
            ContentType = "application/json"
            TimeoutSec = 10
        }
        
        if ($Body) {
            $params.Body = ($Body | ConvertTo-Json -Compress)
        }
        
        $response = Invoke-WebRequest @params -UseBasicParsing
        
        Write-Host "  ✅ Status: $($response.StatusCode)" -ForegroundColor Green
        
        if ($response.Content) {
            $content = $response.Content
            if ($content.Length -gt 200) {
                $content = $content.Substring(0, 200) + "..."
            }
            Write-Host "  Response: $content" -ForegroundColor Gray
        }
        
        $script:testResults += @{
            Name = $Name
            Status = "PASS"
            StatusCode = $response.StatusCode
        }
        
        return $true
    }
    catch {
        Write-Host "  ❌ Error: $($_.Exception.Message)" -ForegroundColor Red
        
        $script:testResults += @{
            Name = $Name
            Status = "FAIL"
            Error = $_.Exception.Message
        }
        
        return $false
    }
    
    Write-Host ""
}

# Test 1: Health Check - Test Page
Write-Host "`n[1/8] Health Check Tests" -ForegroundColor Cyan
Write-Host "------------------------" -ForegroundColor Cyan
Test-Endpoint -Name "Test Page (/test)" -Method "GET" -Url "$baseUrl/test"
Test-Endpoint -Name "Home Page (/)" -Method "GET" -Url "$baseUrl/"

# Test 2: Settings API - GET
Write-Host "`n[2/8] Settings API - Read" -ForegroundColor Cyan
Write-Host "------------------------" -ForegroundColor Cyan
Test-Endpoint -Name "Get Settings" -Method "GET" -Url "$baseUrl/api/settings"

# Test 3: Settings API - Save SerpAPI
Write-Host "`n[3/8] Settings API - Save SerpAPI" -ForegroundColor Cyan
Write-Host "------------------------" -ForegroundColor Cyan
$serpApiBody = @{
    api_key = "test-serpapi-key-$(Get-Random -Maximum 9999)"
}
Test-Endpoint -Name "Save SerpAPI Key" -Method "POST" -Url "$baseUrl/api/settings/serpapi" -Body $serpApiBody

# Test 4: Settings API - Save AI Config
Write-Host "`n[4/8] Settings API - Save AI Config" -ForegroundColor Cyan
Write-Host "------------------------" -ForegroundColor Cyan
$aiConfigBody = @{
    base_url = "https://open.bigmodel.cn/api/paas/v4"
    api_key = "test-ai-key-$(Get-Random -Maximum 9999)"
    model = "glm-4-flash"
}
Test-Endpoint -Name "Save AI Config" -Method "POST" -Url "$baseUrl/api/settings/ai" -Body $aiConfigBody

# Test 5: Verify Settings Were Saved
Write-Host "`n[5/8] Verify Settings Persistence" -ForegroundColor Cyan
Write-Host "------------------------" -ForegroundColor Cyan
Test-Endpoint -Name "Get Settings (After Save)" -Method "GET" -Url "$baseUrl/api/settings"

# Test 6: Local Search API
Write-Host "`n[6/8] Patent Search - Local" -ForegroundColor Cyan
Write-Host "------------------------" -ForegroundColor Cyan
$searchBody = @{
    query = "人工智能"
    page = 1
    page_size = 5
}
Test-Endpoint -Name "Local Search" -Method "POST" -Url "$baseUrl/api/search" -Body $searchBody

# Test 7: Online Search API (may fail without valid SerpAPI key)
Write-Host "`n[7/8] Patent Search - Online (SerpAPI)" -ForegroundColor Cyan
Write-Host "------------------------" -ForegroundColor Cyan
Write-Host "  Note: This may fail without a valid SerpAPI key" -ForegroundColor Yellow
$onlineSearchBody = @{
    query = "artificial intelligence"
    page = 1
    page_size = 3
}
Test-Endpoint -Name "Online Search" -Method "POST" -Url "$baseUrl/api/search/online" -Body $onlineSearchBody

# Test 8: Page Routes
Write-Host "`n[8/8] Page Routes" -ForegroundColor Cyan
Write-Host "------------------------" -ForegroundColor Cyan
Test-Endpoint -Name "Settings Page" -Method "GET" -Url "$baseUrl/settings"
Test-Endpoint -Name "Search Page" -Method "GET" -Url "$baseUrl/search"
Test-Endpoint -Name "Compare Page" -Method "GET" -Url "$baseUrl/compare"
Test-Endpoint -Name "AI Page" -Method "GET" -Url "$baseUrl/ai"

# Summary
Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host "  Test Summary" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

$passed = ($testResults | Where-Object { $_.Status -eq "PASS" }).Count
$failed = ($testResults | Where-Object { $_.Status -eq "FAIL" }).Count
$total = $testResults.Count

Write-Host "`nTotal Tests: $total" -ForegroundColor White
Write-Host "Passed: $passed" -ForegroundColor Green
Write-Host "Failed: $failed" -ForegroundColor Red

if ($failed -gt 0) {
    Write-Host "`nFailed Tests:" -ForegroundColor Red
    $testResults | Where-Object { $_.Status -eq "FAIL" } | ForEach-Object {
        Write-Host "  - $($_.Name): $($_.Error)" -ForegroundColor Red
    }
}

Write-Host "`n========================================" -ForegroundColor Cyan

# Check if server is running
if ($failed -eq $total) {
    Write-Host "`n⚠️  WARNING: All tests failed!" -ForegroundColor Red
    Write-Host "Is the server running? Try:" -ForegroundColor Yellow
    Write-Host "  cd patent-hub" -ForegroundColor Gray
    Write-Host "  cargo run --release" -ForegroundColor Gray
    Write-Host "  OR" -ForegroundColor Gray
    Write-Host "  .\start.bat" -ForegroundColor Gray
}

Write-Host ""
