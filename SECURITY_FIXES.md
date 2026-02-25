# Security Fixes for Settings API

## Overview
The current implementation has several security concerns that should be addressed before production use.

## Fix 1: Mask API Keys in GET Response

### Current Code (Insecure):
```rust
pub async fn api_get_settings() -> Json<serde_json::Value> {
    Json(json!({
        "serpapi_key": std::env::var("SERPAPI_KEY").unwrap_or_default(),
        "ai_api_key": std::env::var("AI_API_KEY").unwrap_or_default(),
        // ...
    }))
}
```

### Proposed Fix:
```rust
pub async fn api_get_settings() -> Json<serde_json::Value> {
    fn mask_api_key(key: &str) -> String {
        if key.is_empty() || key == "your-serpapi-key-here" {
            String::new()
        } else if key.len() <= 8 {
            "****".to_string()
        } else {
            format!("{}****{}", &key[..4], &key[key.len()-4..])
        }
    }
    
    let serpapi_key = std::env::var("SERPAPI_KEY").unwrap_or_default();
    let ai_api_key = std::env::var("AI_API_KEY").unwrap_or_default();
    
    Json(json!({
        "serpapi_key": mask_api_key(&serpapi_key),
        "serpapi_key_configured": !serpapi_key.is_empty() && serpapi_key != "your-serpapi-key-here",
        "ai_base_url": std::env::var("AI_BASE_URL").unwrap_or_default(),
        "ai_api_key": mask_api_key(&ai_api_key),
        "ai_api_key_configured": !ai_api_key.is_empty(),
        "ai_model": std::env::var("AI_MODEL").unwrap_or_default()
    }))
}
```

**Benefits**:
- API keys shown as `abcd****xyz9` instead of full key
- Frontend can check `*_configured` flags to show status
- Keys not exposed in browser DevTools or logs

## Fix 2: Add Input Validation

### Add to Cargo.toml:
```toml
[dependencies]
url = "2.5"  # For URL validation
```

### Proposed Fix for api_save_ai:
```rust
pub async fn api_save_ai(Json(req): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let base_url = req["base_url"].as_str().unwrap_or("").trim();
    let api_key = req["api_key"].as_str().unwrap_or("").trim();
    let model = req["model"].as_str().unwrap_or("").trim();
    
    // Validate all fields present
    if base_url.is_empty() || api_key.is_empty() || model.is_empty() {
        return Json(json!({
            "status": "error",
            "message": "All fields are required"
        }));
    }
    
    // Validate URL format
    if let Err(_) = url::Url::parse(base_url) {
        return Json(json!({
            "status": "error",
            "message": "Invalid URL format. Must start with http:// or https://"
        }));
    }
    
    // Validate URL scheme
    if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
        return Json(json!({
            "status": "error",
            "message": "URL must use HTTP or HTTPS protocol"
        }));
    }
    
    // Validate API key length (reasonable bounds)
    if api_key.len() < 10 || api_key.len() > 200 {
        return Json(json!({
            "status": "error",
            "message": "API key length must be between 10 and 200 characters"
        }));
    }
    
    // Validate model name
    if model.len() < 2 || model.len() > 100 {
        return Json(json!({
            "status": "error",
            "message": "Model name must be between 2 and 100 characters"
        }));
    }
    
    // Validate model name characters (alphanumeric, dash, underscore, dot)
    if !model.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.') {
        return Json(json!({
            "status": "error",
            "message": "Model name contains invalid characters"
        }));
    }
    
    // Save if validation passes
    if let Err(e) = update_env_file("AI_BASE_URL", base_url) {
        return Json(json!({"status": "error", "message": e.to_string()}));
    }
    if let Err(e) = update_env_file("AI_API_KEY", api_key) {
        return Json(json!({"status": "error", "message": e.to_string()}));
    }
    if let Err(e) = update_env_file("AI_MODEL", model) {
        return Json(json!({"status": "error", "message": e.to_string()}));
    }
    
    std::env::set_var("AI_BASE_URL", base_url);
    std::env::set_var("AI_API_KEY", api_key);
    std::env::set_var("AI_MODEL", model);
    
    Json(json!({"status": "ok"}))
}
```

### Proposed Fix for api_save_serpapi:
```rust
pub async fn api_save_serpapi(Json(req): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let api_key = req["api_key"].as_str().unwrap_or("").trim();
    
    if api_key.is_empty() {
        return Json(json!({
            "status": "error",
            "message": "API key is required"
        }));
    }
    
    // Validate key length
    if api_key.len() < 20 || api_key.len() > 200 {
        return Json(json!({
            "status": "error",
            "message": "Invalid API key format"
        }));
    }
    
    // Validate key characters (alphanumeric and common special chars)
    if !api_key.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Json(json!({
            "status": "error",
            "message": "API key contains invalid characters"
        }));
    }
    
    if let Err(e) = update_env_file("SERPAPI_KEY", api_key) {
        return Json(json!({"status": "error", "message": e.to_string()}));
    }
    
    std::env::set_var("SERPAPI_KEY", api_key);
    Json(json!({"status": "ok"}))
}
```

## Fix 3: Add File Locking

### Add to Cargo.toml:
```toml
[dependencies]
fs2 = "0.4"  # For file locking
```

### Proposed Fix for update_env_file:
```rust
use fs2::FileExt;
use std::fs::OpenOptions;
use std::io::{Read, Write, Seek, SeekFrom};

fn update_env_file(key: &str, value: &str) -> Result<(), String> {
    let env_path = ".env";
    
    // Open file with read/write access, create if doesn't exist
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(env_path)
        .map_err(|e| format!("Failed to open .env file: {}", e))?;
    
    // Lock file for exclusive access (prevents concurrent writes)
    file.lock_exclusive()
        .map_err(|e| format!("Failed to lock .env file: {}", e))?;
    
    // Read current content
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| format!("Failed to read .env file: {}", e))?;
    
    // Parse and update lines
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let mut found = false;
    
    for line in &mut lines {
        if line.starts_with(&format!("{}=", key)) {
            *line = format!("{}={}", key, value);
            found = true;
            break;
        }
    }
    
    if !found {
        lines.push(format!("{}={}", key, value));
    }
    
    // Write back to file
    file.set_len(0)
        .map_err(|e| format!("Failed to truncate .env file: {}", e))?;
    file.seek(SeekFrom::Start(0))
        .map_err(|e| format!("Failed to seek .env file: {}", e))?;
    file.write_all(lines.join("\n").as_bytes())
        .map_err(|e| format!("Failed to write .env file: {}", e))?;
    
    // Unlock file (happens automatically on drop, but explicit is better)
    file.unlock()
        .map_err(|e| format!("Failed to unlock .env file: {}", e))?;
    
    Ok(())
}
```

## Fix 4: Update Frontend to Handle Masked Keys

### Update settings.html JavaScript:
```javascript
window.onload = async function() {
    try {
        const res = await fetch('/api/settings');
        if (res.ok) {
            const config = await res.json();
            
            // Show masked keys or placeholder
            document.getElementById('serpapi-key').placeholder = 
                config.serpapi_key_configured ? 
                `Current: ${config.serpapi_key} (enter new to change)` : 
                'Enter your SerpAPI Key';
            
            document.getElementById('ai-api-key').placeholder = 
                config.ai_api_key_configured ? 
                `Current: ${config.ai_api_key} (enter new to change)` : 
                'Enter your AI API Key';
            
            // Fill in non-sensitive fields
            document.getElementById('ai-base-url').value = config.ai_base_url || '';
            document.getElementById('ai-model').value = config.ai_model || '';
            
            updateConfigStatus(config);
        }
    } catch (e) {
        console.error('Failed to load config:', e);
    }
};

function updateConfigStatus(config) {
    const status = [];
    if (config.serpapi_key_configured) {
        status.push('✅ SerpAPI configured');
    } else {
        status.push('❌ SerpAPI not configured');
    }
    
    if (config.ai_api_key_configured) {
        status.push('✅ AI service configured');
    } else {
        status.push('❌ AI service not configured');
    }
    
    document.getElementById('config-status').innerHTML = status.join('<br>');
}
```

## Implementation Priority

1. **High Priority** (Security):
   - ✅ Mask API keys in GET response
   - ✅ Add input validation

2. **Medium Priority** (Reliability):
   - ✅ Add file locking
   - ✅ Update frontend to handle masked keys

3. **Low Priority** (Future):
   - Add authentication middleware
   - Add audit logging
   - Add rate limiting

## Testing After Fixes

```powershell
# Test masked keys
curl http://localhost:3000/api/settings
# Should show: "serpapi_key": "abcd****xyz9"

# Test validation
curl -X POST http://localhost:3000/api/settings/ai \
  -H "Content-Type: application/json" \
  -d '{"base_url":"invalid","api_key":"short","model":"x"}'
# Should return validation errors

# Test concurrent writes (run simultaneously in 2 terminals)
curl -X POST http://localhost:3000/api/settings/serpapi \
  -H "Content-Type: application/json" \
  -d '{"api_key":"test-key-1"}'
# Both should succeed without corrupting .env
```

## Summary

These fixes address:
- ✅ **Security**: API keys no longer exposed in plain text
- ✅ **Reliability**: File locking prevents corruption
- ✅ **Robustness**: Input validation prevents invalid configs
- ✅ **UX**: Better error messages and status indicators

**Estimated Implementation Time**: 1-2 hours  
**Risk Level**: Low (backward compatible changes)  
**Testing Required**: Yes (run test suite after implementation)
