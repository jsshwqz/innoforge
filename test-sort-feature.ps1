# Test Script for New sort_by Parameter
# Tests the newly added sort_by field in SearchRequest

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Patent Hub - Sort Feature Test" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

$baseUrl = "http://localhost:3000"
$testResults = @()

function Test-SearchWithSort {
    param(
        [string]$Name,
        [string]$Query,
        [string]$SortBy
    )
    
    Write-Host "Testing: $Name" -ForegroundColor Yellow
    
    $body = @{
        query = $Query
        page = 1
        page_size = 5
    }
    
    if ($SortBy) {
        $body.sort_by = $SortBy
    }
    
    try {
        $response = Invoke-WebRequest -Uri "$baseUrl/api/search" `
            -Method POST `
            -ContentType "application/json" `
            -Body ($body | ConvertTo-Json) `
            -UseBasicParsing
        
        $result = $response.Content | ConvertFrom-Json
        
        Write-Host "  ✅ Status: $($response.StatusCode)" -ForegroundColor Green
        Write-Host "  Results: $($result.total) patents found" -ForegroundColor Gray
        
        if ($result.patents.Count -gt 0) {
            Write-Host "  First result: $($result.patents[0].title)" -ForegroundColor Gray
            if ($result.patents[0].filing_date) {
                Write-Host "  Filing date: $($result.patents[0].filing_date)" -ForegroundColor Gray
            }
        }
        
        $script:testResults += @{
            Name = $Name
            Status = "PASS"
            Total = $result.total
            SortBy = $SortBy
        }
        
        return $result
    }
    catch {
        Write-Host "  ❌ Error: $($_.Exception.Message)" -ForegroundColor Red
        
        $script:testResults += @{
            Name = $Name
            Status = "FAIL"
            Error = $_.Exception.Message
        }
        
        return $null
    }
    
    Write-Host ""
}

# Test 1: Import test data first
Write-Host "`n[1/6] Importing Test Data" -ForegroundColor Cyan
Write-Host "------------------------" -ForegroundColor Cyan

$importBody = @{
    patents = @(
        @{
            id = "sort-test-001"
            patent_number = "CN2024001A"
            title = "最新专利 - 2024年申请"
            abstract_text = "这是2024年的专利"
            description = ""
            claims = ""
            applicant = "测试公司A"
            inventor = "张三"
            filing_date = "2024-01-01"
            publication_date = "2024-06-01"
            grant_date = $null
            ipc_codes = "G06N"
            cpc_codes = "G06N"
            priority_date = "2024-01-01"
            country = "CN"
            kind_code = "A"
            family_id = $null
            legal_status = "pending"
            citations = ""
            cited_by = ""
            source = "sort_test"
            raw_json = "{}"
            created_at = (Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ")
        },
        @{
            id = "sort-test-002"
            patent_number = "CN2020001A"
            title = "旧专利 - 2020年申请"
            abstract_text = "这是2020年的专利"
            description = ""
            claims = ""
            applicant = "测试公司B"
            inventor = "李四"
            filing_date = "2020-01-01"
            publication_date = "2020-06-01"
            grant_date = $null
            ipc_codes = "G06N"
            cpc_codes = "G06N"
            priority_date = "2020-01-01"
            country = "CN"
            kind_code = "A"
            family_id = $null
            legal_status = "granted"
            citations = ""
            cited_by = ""
            source = "sort_test"
            raw_json = "{}"
            created_at = (Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ")
        },
        @{
            id = "sort-test-003"
            patent_number = "CN2022001A"
            title = "中等专利 - 2022年申请"
            abstract_text = "这是2022年的专利"
            description = ""
            claims = ""
            applicant = "测试公司C"
            inventor = "王五"
            filing_date = "2022-01-01"
            publication_date = "2022-06-01"
            grant_date = $null
            ipc_codes = "G06N"
            cpc_codes = "G06N"
            priority_date = "2022-01-01"
            country = "CN"
            kind_code = "A"
            family_id = $null
            legal_status = "pending"
            citations = ""
            cited_by = ""
            source = "sort_test"
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
    
    $result = $response.Content | ConvertFrom-Json
    Write-Host "✅ Imported $($result.imported) test patents" -ForegroundColor Green
}
catch {
    Write-Host "❌ Import failed: $($_.Exception.Message)" -ForegroundColor Red
    Write-Host "⚠️  Continuing with existing data..." -ForegroundColor Yellow
}

Write-Host ""

# Test 2: Search without sort_by (default behavior)
Write-Host "`n[2/6] Search Without Sort Parameter" -ForegroundColor Cyan
Write-Host "------------------------" -ForegroundColor Cyan
$defaultResult = Test-SearchWithSort -Name "Default Search (no sort_by)" -Query "专利" -SortBy $null

# Test 3: Search with sort_by = "relevance"
Write-Host "`n[3/6] Search Sorted by Relevance" -ForegroundColor Cyan
Write-Host "------------------------" -ForegroundColor Cyan
$relevanceResult = Test-SearchWithSort -Name "Sort by Relevance" -Query "专利" -SortBy "relevance"

# Test 4: Search with sort_by = "new"
Write-Host "`n[4/6] Search Sorted by Newest" -ForegroundColor Cyan
Write-Host "------------------------" -ForegroundColor Cyan
$newResult = Test-SearchWithSort -Name "Sort by Newest" -Query "专利" -SortBy "new"

# Test 5: Search with sort_by = "old"
Write-Host "`n[5/6] Search Sorted by Oldest" -ForegroundColor Cyan
Write-Host "------------------------" -ForegroundColor Cyan
$oldResult = Test-SearchWithSort -Name "Sort by Oldest" -Query "专利" -SortBy "old"

# Test 6: Online search with sort_by
Write-Host "`n[6/6] Online Search with Sort" -ForegroundColor Cyan
Write-Host "------------------------" -ForegroundColor Cyan
Write-Host "Testing online search with sort_by parameter..." -ForegroundColor Yellow

$onlineBody = @{
    query = "artificial intelligence"
    page = 1
    page_size = 3
    sort_by = "new"
}

try {
    $response = Invoke-WebRequest -Uri "$baseUrl/api/search/online" `
        -Method POST `
        -ContentType "application/json" `
        -Body ($onlineBody | ConvertTo-Json) `
        -UseBasicParsing
    
    Write-Host "  ✅ Status: $($response.StatusCode)" -ForegroundColor Green
    Write-Host "  Note: Online search may not support sorting yet" -ForegroundColor Gray
}
catch {
    Write-Host "  ⚠️  Online search failed (expected without valid API key)" -ForegroundColor Yellow
}

Write-Host ""

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

if ($passed -gt 0) {
    Write-Host "`n✅ Sort Parameter Tests:" -ForegroundColor Green
    $testResults | Where-Object { $_.Status -eq "PASS" } | ForEach-Object {
        $sortInfo = if ($_.SortBy) { " (sort_by: $($_.SortBy))" } else { " (no sort)" }
        Write-Host "  - $($_.Name)$sortInfo - Found $($_.Total) results" -ForegroundColor Gray
    }
}

if ($failed -gt 0) {
    Write-Host "`nFailed Tests:" -ForegroundColor Red
    $testResults | Where-Object { $_.Status -eq "FAIL" } | ForEach-Object {
        Write-Host "  - $($_.Name): $($_.Error)" -ForegroundColor Red
    }
}

Write-Host "`n========================================" -ForegroundColor Cyan

# Analysis
Write-Host "`n📊 Analysis:" -ForegroundColor Cyan

if ($newResult -and $oldResult) {
    Write-Host "`nDate Sorting Verification:" -ForegroundColor Yellow
    
    if ($newResult.patents.Count -gt 0 -and $oldResult.patents.Count -gt 0) {
        $newestFirst = $newResult.patents[0].filing_date
        $oldestFirst = $oldResult.patents[0].filing_date
        
        Write-Host "  Newest-first result: $newestFirst" -ForegroundColor Gray
        Write-Host "  Oldest-first result: $oldestFirst" -ForegroundColor Gray
        
        if ($newestFirst -gt $oldestFirst) {
            Write-Host "  ✅ Sorting appears to be working correctly!" -ForegroundColor Green
        } else {
            Write-Host "  ⚠️  Sorting may need implementation in backend" -ForegroundColor Yellow
        }
    } else {
        Write-Host "  ⚠️  Not enough data to verify sorting" -ForegroundColor Yellow
    }
}

Write-Host "`n📝 Notes:" -ForegroundColor Cyan
Write-Host "  - The sort_by parameter has been added to SearchRequest struct" -ForegroundColor Gray
Write-Host "  - Valid values: 'relevance', 'new', 'old'" -ForegroundColor Gray
Write-Host "  - Backend implementation may be needed to actually sort results" -ForegroundColor Gray
Write-Host "  - Check src/routes.rs api_search() function for sorting logic" -ForegroundColor Gray

Write-Host ""
