//! Fake AI provider stub for issue #17 CI testing.
//! Returns controlled fixed responses for all AI endpoints.
//! Listens on a configurable port; intended to be used as
//! AI_BASE_URL in test runs so AI chains execute without
//! any external network requests.

use std::io::{Read, Write};
use std::net::TcpListener;

/// Start the fake AI stub on an OS-assigned ephemeral port. Returns
/// `(port, handle)` immediately; the listener runs in a background thread.
///
/// 端口由 `bind("127.0.0.1:0")` 在**起线程之前**分配并已进入 listen，所以调用方拿到
/// port 就能连——不要退回"先 free_port() 探端口、再在线程内 bind"：空窗期端口会被
/// 同二进制并行用例抢走，桩线程 bind 失败静默 return，客户端 Connection refused
/// （CI run 40630679117 实测）。
pub fn start_fake_ai_stub() -> (u16, std::thread::JoinHandle<()>) {
    let listener =
        TcpListener::bind("127.0.0.1:0").expect("fake AI stub must bind an ephemeral port");
    let port = listener
        .local_addr()
        .expect("fake AI stub local_addr")
        .port();
    let handle = std::thread::spawn(move || {
        for stream in listener.incoming() {
            if let Ok(mut stream) = stream {
                handle_request(&mut stream);
            }
        }
    });
    (port, handle)
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
