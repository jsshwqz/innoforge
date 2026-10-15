//! Fake AI provider stub for issue #17 CI testing.
//! Returns controlled fixed responses for all AI endpoints.
//! Listens on a configurable port; intended to be used as
//! AI_BASE_URL in test runs so AI chains execute without
//! any external network requests.

use std::io::{Read, Write};
use std::net::TcpListener;

/// Start the fake AI stub on the given port. Returns immediately;
/// the listener runs in a background thread.
pub fn start_fake_ai_stub(port: u16) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let listener = match TcpListener::bind(format!("127.0.0.1:{}", port)) {
            Ok(l) => l,
            Err(_) => return,
        };
        for stream in listener.incoming() {
            if let Ok(mut stream) = stream {
                handle_request(&mut stream);
            }
        }
    })
}

fn handle_request(stream: &mut std::net::TcpStream) {
    let mut buf = [0u8; 4096];
    let _ = stream.read(&mut buf);
    let request = String::from_utf8_lossy(&buf);

    // Determine response based on the request path
    let response_body = if request.contains("/chat/completions") {
        // Return a minimal chat completion response
        r#"{"choices":[{"message":{"role":"assistant","content":"This is a synthetic AI response for testing purposes. No real AI was called."},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":15,"total_tokens":25}}"#
    } else if request.contains("/embeddings") {
        r#"{"data":[{"embedding":[0.1,0.2,0.3,0.4,0.5]}],"usage":{"prompt_tokens":5,"total_tokens":5}}"#
    } else {
        r#"{"status":"ok","message":"synthetic response"}"#
    };

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        response_body.len(),
        response_body
    );
    let _ = stream.write_all(response.as_bytes());
}
