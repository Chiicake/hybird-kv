//! # Redis CLI Integration Tests
//!
//! Purpose: Verify RESP compatibility using the real `redis-cli` binary when
//! available on the host.
//!
//! ## Design Principles
//!
//! 1. **End-to-End**: Exercise the TCP server through the Redis CLI.
//! 2. **Fail-Open**: Skip tests when `redis-cli` is unavailable.
//! 3. **Stable Outputs**: Validate trimmed stdout for predictable assertions.
//! 4. **Isolated Server**: Bind to an ephemeral port per test.

use std::net::SocketAddr;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::sync::oneshot;

use hkv_engine::MemoryEngine;
use hkv_server::metrics::Metrics;
use hkv_server::server;

fn redis_cli_available() -> bool {
    Command::new("redis-cli").arg("--version").output().is_ok()
}

fn run_redis_cli(port: u16, args: &[&str]) -> std::io::Result<String> {
    let output = Command::new("redis-cli")
        .arg("-p")
        .arg(port.to_string())
        .args(args)
        .output()?;

    assert!(
        output.status.success(),
        "redis-cli failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

async fn spawn_test_server() -> std::io::Result<(SocketAddr, oneshot::Sender<()>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let engine = Arc::new(MemoryEngine::new());
    let metrics = Arc::new(Metrics::new());
    let expirer = engine.start_expirer(Duration::from_millis(50));

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    tokio::spawn(async move {
        let mut expirer = Some(expirer);
        let _ = server::serve_with_shutdown(listener, engine, metrics, async {
            let _ = shutdown_rx.await;
        })
        .await;

        if let Some(handle) = expirer.take() {
            handle.stop();
        }
    });

    Ok((addr, shutdown_tx))
}

#[tokio::test]
async fn redis_cli_basic_commands() {
    if !redis_cli_available() {
        eprintln!("redis-cli not found; skipping integration test");
        return;
    }

    let (addr, shutdown) = spawn_test_server().await.unwrap();
    let port = addr.port();

    let pong = run_redis_cli(port, &["PING"]).unwrap();
    assert_eq!(pong, "PONG");

    let ok = run_redis_cli(port, &["SET", "key", "value"]).unwrap();
    assert_eq!(ok, "OK");

    let value = run_redis_cli(port, &["GET", "key"]).unwrap();
    assert_eq!(value, "value");

    let ttl = run_redis_cli(port, &["TTL", "key"]).unwrap();
    assert_eq!(ttl, "-1");

    let set_with_ttl = run_redis_cli(port, &["SET", "ttl-key", "value", "EX", "1"]).unwrap();
    assert_eq!(set_with_ttl, "OK");

    let ttl = run_redis_cli(port, &["TTL", "ttl-key"]).unwrap();
    let ttl: i64 = ttl.parse().unwrap();
    assert!(ttl >= 0);

    let expire = run_redis_cli(port, &["EXPIRE", "key", "1"]).unwrap();
    assert_eq!(expire, "1");

    std::thread::sleep(Duration::from_millis(1100));
    let missing = run_redis_cli(port, &["GET", "key"]).unwrap();
    assert_eq!(missing, "(nil)");

    let ttl = run_redis_cli(port, &["TTL", "key"]).unwrap();
    assert_eq!(ttl, "-2");

    let ttl_key_value = run_redis_cli(port, &["GET", "ttl-key"]).unwrap();
    assert_eq!(ttl_key_value, "(nil)");

    let removed = run_redis_cli(port, &["DEL", "key"]).unwrap();
    assert_eq!(removed, "0");

    let info = run_redis_cli(port, &["INFO"]).unwrap();
    assert!(info.contains("engine:hybridkv"));

    let _ = shutdown.send(());
}
