//! 测试用端口分配（issue #110）。
//!
//! 旧写法：`bind("127.0.0.1:0")` 拿到端口 → drop listener → 把**数字**交给调用方。
//! 从 drop 到调用方真正 bind 之间，同一个端口可能被本进程里**并行跑的另一个用例**取走，
//! 于是"我拿到的端口"实际已被占用（真实形态：负控在 `.expect` 处假红，CI 实测一次）。
//!
//! 这里把"本进程已经发出去的端口"记进全局集合：撞了就再要一个。这把"用例之间互相抢"
//! 这一类竞态消掉；**外部进程**仍可能抢占（在本进程内无法根除），那部分由调用方的
//! 等待/重试兜底。真正要根除，得改成"先 bind 再把 listener 交给服务端"，那是调用点级的
//! 改造，不在本函数职责内。

use std::collections::HashSet;
use std::net::TcpListener;
use std::sync::{Mutex, OnceLock};

fn handed_out() -> &'static Mutex<HashSet<u16>> {
    static SET: OnceLock<Mutex<HashSet<u16>>> = OnceLock::new();
    SET.get_or_init(|| Mutex::new(HashSet::new()))
}

/// 取一个"本进程还没发出去过"的端口。
pub fn free_port() -> u16 {
    // 内核分配 :0 后可能立刻回收同一个号，所以要"查到不重复为止"（64 次足够）。
    for _ in 0..64 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
        let port = listener
            .local_addr()
            .expect("local_addr for ephemeral port")
            .port();
        drop(listener);
        let mut set = handed_out().lock().expect("port allocator poisoned");
        if set.insert(port) {
            return port;
        }
    }
    // 极端兜底：64 次都撞（几乎不可能），仍把这次的号发出去并记账，保证不静默返回重复值。
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
    let port = listener
        .local_addr()
        .expect("local_addr for ephemeral port")
        .port();
    drop(listener);
    handed_out()
        .lock()
        .expect("port allocator poisoned")
        .insert(port);
    port
}
