# Test script for Settings API endpoints
Write-Host "=== Testing Patent Hub Settings API ===" -ForegroundColor Cyan

$baseUrl = "http://localhost:3000"

# Test 1: Get Settings
Write-Host "`n[TEST 1] GET /api/settings" -ForegroundColor Yellow
try {
    $response = Invoke-RestMethod -Uri "$baseUrl/api/settings" -Method GET
    Write-Host "✓ Success" -ForegroundColor Green
    $response | ConvertTo-Json -Depth 3
} catch {
    Write-Host "✗ Failed: $($_.Exception.Message)" -ForegroundColor Red
}

# Test 2: Save SerpAPI Key
Write-Host "`n[TEST 2] POST /api/settings/serpapi" -ForegroundColor Yellow
try {
    $body = @{
        api_key = "test-serpapi-key-12345"
    } | ConvertTo-Json
    
    $response = Invoke-RestMethod -Uri "$baseUrl/api/settings/serpapi" -Method POST -Body $body -ContentType "application/json"
    Write-Host "✓ Success" -ForegroundColor Green
    $response | ConvertTo-Json -Depth 3
} catch {
    Write-Host "✗ Failed: $($_.Exception.Message)" -ForegroundColor Red
}

# Test 3: Save AI Config
Write-Host "`n[TEST 3] POST /api/settings/ai" -ForegroundColor Yellow
try {
    $body = @{
        base_url = "https://open.bigmodel.cn/api/paas/v4"
        api_key = "test-ai-key-12345"
        model = "glm-4-flash"
    } | ConvertTo-Json
    
    $response = Invoke-RestMethod -Uri "$baseUrl/api/settings/ai" -Method POST -Body $body -ContentType "application/json"
    Write-Host "✓ Success" -ForegroundColor Green
    $response | ConvertTo-Json -Depth 3
} catch {
    Write-Host "✗ Failed: $($_.Exception.Message)" -ForegroundColor Red
}

# Test 4: Verify Settings Were Saved
Write-Host "`n[TEST 4] Verify settings were saved" -ForegroundColor Yellow
try {
    $response = Invoke-RestMethod -Uri "$baseUrl/api/settings" -Method GET
    Write-Host "✓ Success" -ForegroundColor Green
    Write-Host "Current Settings:" -ForegroundColor Cyan
    $response | ConvertTo-Json -Depth 3
} catch {
    Write-Host "✗ Failed: $($_.Exception.Message)" -ForegroundColor Red
}

Write-Host "`n=== Test Complete ===" -ForegroundColor Cyan
