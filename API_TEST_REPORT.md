# Patent Hub API Test Report

## Overview
New settings API endpoints have been added to `src/routes.rs` for managing system configuration.

## New Endpoints

### 1. GET /api/settings
**Purpose**: Retrieve current system settings  
**Response**:
```json
{
  "serpapi_key": "string",
  "ai_base_url": "string", 
  "ai_api_key": "string",
  "ai_model": "string"
}
```

### 2. POST /api/settings/serpapi
**Purpose**: Save SerpAPI key  
**Request Body**:
```json
{
  "api_key": "string"
}
```
**Response**:
```json
{
  "status": "ok" | "error",
  "message": "string (if error)"
}
```

### 3. POST /api/settings/ai
**Purpose**: Save AI configuration  
**Request Body**:
```json
{
  "base_url": "string",
  "api_key": "string",
  "model": "string"
}
```
**Response**:
```json
{
  "status": "ok" | "error",
  "message": "string (if error)"
}
```

## Issues Found

### ❌ Issue 1: Server Needs Restart
**Problem**: The routes are registered in `main.rs` but the running server returns 404  
**Cause**: Server was started before the new code was added  
**Fix**: Restart the server to load the new routes

### ⚠️ Issue 2: Missing Error Handling
**Problem**: The `update_env_file` function doesn't handle concurrent writes  
**Risk**: Race conditions if multiple requests update settings simultaneously  
**Recommendation**: Add file locking or use atomic writes

### ⚠️ Issue 3: Security Concern
**Problem**: API keys are returned in plain text via GET /api/settings  
**Risk**: Sensitive data exposure in logs, browser history, etc.  
**Recommendation**: Mask API keys (show only last 4 characters) or require authentication

### ⚠️ Issue 4: No Input Validation
**Problem**: No validation for URL format, key length, or model names  
**Risk**: Invalid configurations could break the system  
**Recommendation**: Add validation before saving

## Recommended Fixes

### Fix 1: Restart Server
```bash
# Stop current server (Ctrl+C)
# Then restart:
cd patent-hub
cargo run --release
```

### Fix 2: Add File Locking
```rust
use std::fs::OpenOptions;
use std::io::{Read, Write, Seek, SeekFrom};

fn update_env_file(key: &str, value: &str) -> Result<(), String> {
    use fs2::FileExt; // Add fs2 = "0.4" to Cargo.toml
    
    let env_path = ".env";
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(env_path)
        .map_err(|e| e.to_string())?;
    
    // Lock file for exclusive access
    file.lock_exclusive().map_err(|e| e.to_string())?;
    
    let mut content = String::new();
    file.read_to_string(&mut content).map_err(|e| e.to_string())?;
    
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
    
    file.set_len(0).map_err(|e| e.to_string())?;
    file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    file.write_all(lines.join("\n").as_bytes()).map_err(|e| e.to_string())?;
    
    file.unlock().map_err(|e| e.to_string())?;
    Ok(())
}
```

### Fix 3: Mask Sensitive Data
```rust
pub async fn api_get_settings() -> Json<serde_json::Value> {
    fn mask_key(key: &str) -> String {
        if key.len() <= 4 {
            "****".to_string()
        } else {
            format!("{}****{}", &key[..2], &key[key.len()-2..])
        }
    }
    
    let serpapi_key = std::env::var("SERPAPI_KEY").unwrap_or_default();
    let ai_api_key = std::env::var("AI_API_KEY").unwrap_or_default();
    
    Json(json!({
        "serpapi_key": if serpapi_key.is_empty() { "" } else { &mask_key(&serpapi_key) },
        "ai_base_url": std::env::var("AI_BASE_URL").unwrap_or_default(),
        "ai_api_key": if ai_api_key.is_empty() { "" } else { &mask_key(&ai_api_key) },
        "ai_model": std::env::var("AI_MODEL").unwrap_or_default()
    }))
}
```

### Fix 4: Add Input Validation
```rust
pub async fn api_save_ai(Json(req): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let base_url = req["base_url"].as_str().unwrap_or("");
    let api_key = req["api_key"].as_str().unwrap_or("");
    let model = req["model"].as_str().unwrap_or("");
    
    // Validation
    if base_url.is_empty() || api_key.is_empty() || model.is_empty() {
        return Json(json!({"status": "error", "message": "All fields required"}));
    }
    
    // Validate URL format
    if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
        return Json(json!({"status": "error", "message": "Invalid URL format"}));
    }
    
    // Validate API key length
    if api_key.len() < 10 {
        return Json(json!({"status": "error", "message": "API key too short"}));
    }
    
    // Validate model name
    if model.len() < 3 || model.len() > 50 {
        return Json(json!({"status": "error", "message": "Invalid model name"}));
    }
    
    // ... rest of the function
}
```

## Testing Instructions

### Step 1: Restart Server
```bash
cd patent-hub
cargo run --release
```

### Step 2: Run Test Script
```powershell
cd patent-hub
.\test-settings-api.ps1
```

### Step 3: Manual Testing with Postman
1. Import `patent-hub.postman_collection.json`
2. Set base_url variable to `http://localhost:3000`
3. Run the "Settings API" folder tests

## Summary

✅ **Routes Registered**: All 3 new endpoints are properly registered in main.rs  
❌ **Server Status**: Needs restart to load new code  
⚠️ **Security**: API keys exposed in plain text  
⚠️ **Reliability**: No file locking for concurrent writes  
⚠️ **Validation**: Missing input validation  

**Next Steps**:
1. Restart server immediately
2. Test endpoints with provided script
3. Implement security fixes (mask API keys)
4. Add input validation
5. Consider adding file locking for production use
