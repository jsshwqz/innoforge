$baseUrl = "http://localhost:3000"
$results = @()

function Test-EP {
    param([string]$Name, [string]$Method, [string]$Url, [string]$Body)
    try {
        $p = @{ Uri = $Url; Method = $Method; TimeoutSec = 15; UseBasicParsing = $true }
        if ($Body) { $p.ContentType = "application/json; charset=utf-8"; $p.Body = [System.Text.Encoding]::UTF8.GetBytes($Body) }
        $r = Invoke-WebRequest @p
        $code = $r.StatusCode
        $preview = if ($r.Content.Length -gt 300) { $r.Content.Substring(0,300) + "..." } else { $r.Content }
        Write-Host "  PASS [$code] $Name" -ForegroundColor Green
        $script:results += @{ Name=$Name; Status="PASS"; Code=$code; Preview=$preview }
    } catch {
        $code = 0
        if ($_.Exception.Response) { $code = [int]$_.Exception.Response.StatusCode }
        Write-Host "  FAIL [$code] $Name - $($_.Exception.Message)" -ForegroundColor Red
        $script:results += @{ Name=$Name; Status="FAIL"; Code=$code; Error=$_.Exception.Message }
    }
}

Write-Host "`n=== Patent Hub API Collection Run ===" -ForegroundColor Cyan
Write-Host ""

Write-Host "[Pages]" -ForegroundColor Yellow
Test-EP "Home Page" GET "$baseUrl/"
Test-EP "Search Page" GET "$baseUrl/search"
Test-EP "AI Page" GET "$baseUrl/ai"
Test-EP "Compare Page" GET "$baseUrl/compare"
Test-EP "Settings Page" GET "$baseUrl/settings"
Test-EP "Test Page" GET "$baseUrl/test"
Test-EP "Import Page" GET "$baseUrl/import"

Write-Host "`n[Settings API]" -ForegroundColor Yellow
Test-EP "Get Settings" GET "$baseUrl/api/settings"
Test-EP "Save SerpAPI Key" POST "$baseUrl/api/settings/serpapi" '{"api_key":"test-serpapi-key-12345"}'
Test-EP "Save AI Config" POST "$baseUrl/api/settings/ai" '{"base_url":"https://open.bigmodel.cn/api/paas/v4","api_key":"test-ai-key","model":"glm-4-flash"}'

Write-Host "`n[Patent Search]" -ForegroundColor Yellow
Test-EP "Local Search" POST "$baseUrl/api/search" '{"query":"人工智能","page":1,"page_size":5}'
Test-EP "Local Search Sort New" POST "$baseUrl/api/search" '{"query":"人工智能","page":1,"page_size":5,"sort_by":"new"}'
Test-EP "Local Search Sort Old" POST "$baseUrl/api/search" '{"query":"人工智能","page":1,"page_size":5,"sort_by":"old"}'
Test-EP "Online Search" POST "$baseUrl/api/search/online" '{"query":"artificial intelligence","page":1,"page_size":3}'
Test-EP "Search Stats" POST "$baseUrl/api/search/stats" '{"query":"人工智能","page":1,"page_size":20}'
Test-EP "Export CSV" POST "$baseUrl/api/search/export" '{"query":"test","page":1,"page_size":20}'

Write-Host "`n[Patent Operations]" -ForegroundColor Yellow
$importJson = '{"patents":[{"id":"test-001","patent_number":"CN999999A","title":"API测试专利","abstract_text":"测试摘要","description":"描述","claims":"权利要求","applicant":"测试公司","inventor":"张三","filing_date":"2024-01-01","publication_date":"2024-06-01","grant_date":null,"ipc_codes":"G06N","cpc_codes":"G06N","priority_date":"2024-01-01","country":"CN","kind_code":"A","family_id":null,"legal_status":"pending","citations":"","cited_by":"","source":"api_test","raw_json":"{}","created_at":"2024-01-01T00:00:00Z"}]}'
Test-EP "Import Patents" POST "$baseUrl/api/patents/import" $importJson
Test-EP "Fetch Patent (EPO)" POST "$baseUrl/api/patent/fetch" '{"patent_number":"EP1234567","source":"epo"}'
Test-EP "Enrich Patent" GET "$baseUrl/api/patent/enrich/test-001"
Test-EP "Similar Patents" GET "$baseUrl/api/patent/similar/test-001"
Test-EP "Patent Detail Page" GET "$baseUrl/patent/test-001"

Write-Host "`n[AI Features]" -ForegroundColor Yellow
Test-EP "AI Chat" POST "$baseUrl/api/ai/chat" '{"message":"分析这个专利","patent_id":"test-001"}'
Test-EP "AI Summarize" POST "$baseUrl/api/ai/summarize" '{"patent_number":"CN999999A"}'
Test-EP "AI Compare" POST "$baseUrl/api/ai/compare" '{"patent_id1":"test-001","patent_id2":"test-002"}'
$analyzeBody = @{
    query = "AI"
    patents = @(
        @{ title = "AI Image Recognition"; abstract_text = "Deep learning based image recognition method"; applicant = "Company A" },
        @{ title = "NLP Method"; abstract_text = "Transformer based NLP approach"; applicant = "Company B" }
    )
} | ConvertTo-Json -Depth 5 -Compress
Test-EP "AI Analyze Results (NEW)" POST "$baseUrl/api/search/analyze" $analyzeBody

Write-Host "`n=== Summary ===" -ForegroundColor Cyan
$pass = ($results | Where-Object { $_.Status -eq "PASS" }).Count
$fail = ($results | Where-Object { $_.Status -eq "FAIL" }).Count
Write-Host "Total: $($results.Count)  Pass: $pass  Fail: $fail" -ForegroundColor White
if ($fail -gt 0) {
    Write-Host "`nFailed endpoints:" -ForegroundColor Red
    $results | Where-Object { $_.Status -eq "FAIL" } | ForEach-Object {
        Write-Host "  [$($_.Code)] $($_.Name): $($_.Error)" -ForegroundColor Red
    }
}
Write-Host ""
