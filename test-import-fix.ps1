# Test Patent Import API with correct payload

$baseUrl = "http://localhost:3000"

Write-Host "Testing Patent Import API with complete payload..." -ForegroundColor Cyan

$importBody = @{
    patents = @(
        @{
            id = "test-$(Get-Random -Maximum 9999)"
            patent_number = "CN999999A"
            title = "API测试专利 - 人工智能图像识别系统"
            abstract_text = "这是通过API导入的测试专利，用于验证导入功能"
            description = "详细描述：本发明涉及一种基于深度学习的图像识别系统"
            claims = "权利要求：1. 一种图像识别方法..."
            applicant = "测试科技有限公司"
            inventor = "张三;李四"
            filing_date = "2024-01-01"
            publication_date = "2024-06-01"
            grant_date = $null
            ipc_codes = "G06N3/08"
            cpc_codes = "G06N3/08"
            priority_date = "2024-01-01"
            country = "CN"
            kind_code = "A"
            family_id = $null
            legal_status = "pending"
            citations = ""
            cited_by = ""
            source = "api_test"
            raw_json = "{}"
            created_at = (Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ")
        }
    )
}

try {
    $response = Invoke-WebRequest -Uri "$baseUrl/api/patents/import" `
        -Method POST `
        -ContentType "application/json" `
        -Body ($importBody | ConvertTo-Json -Depth 10) `
        -UseBasicParsing
    
    Write-Host "✅ Success! Status: $($response.StatusCode)" -ForegroundColor Green
    Write-Host "Response:" -ForegroundColor Yellow
    $response.Content | ConvertFrom-Json | ConvertTo-Json -Depth 10
    
    Write-Host "`n✅ Patent import is working correctly!" -ForegroundColor Green
}
catch {
    Write-Host "❌ Error: $($_.Exception.Message)" -ForegroundColor Red
    if ($_.Exception.Response) {
        $reader = [System.IO.StreamReader]::new($_.Exception.Response.GetResponseStream())
        $responseBody = $reader.ReadToEnd()
        Write-Host "Response body: $responseBody" -ForegroundColor Yellow
    }
}

# Now test if we can search for it
Write-Host "`nTesting if imported patent is searchable..." -ForegroundColor Cyan

$searchBody = @{
    query = "API测试专利"
    page = 1
    page_size = 5
}

try {
    $response = Invoke-WebRequest -Uri "$baseUrl/api/search" `
        -Method POST `
        -ContentType "application/json" `
        -Body ($searchBody | ConvertTo-Json) `
        -UseBasicParsing
    
    $result = $response.Content | ConvertFrom-Json
    
    if ($result.patents.Count -gt 0) {
        Write-Host "✅ Found $($result.patents.Count) patent(s)" -ForegroundColor Green
        Write-Host "First result: $($result.patents[0].title)" -ForegroundColor Yellow
    } else {
        Write-Host "⚠️  No patents found (may need to wait for indexing)" -ForegroundColor Yellow
    }
}
catch {
    Write-Host "❌ Search failed: $($_.Exception.Message)" -ForegroundColor Red
}
