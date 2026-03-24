use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream as StdTcpStream};
use std::sync::Arc;
use std::time::Duration;

use hkv_client::KVClient;
use hkv_engine::MemoryEngine;
use hkv_server::metrics::Metrics;
use hkv_server::server;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

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

fn send_raw(addr: SocketAddr, request: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut stream = StdTcpStream::connect(addr)?;
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    stream.write_all(request)?;
    stream.shutdown(Shutdown::Write)?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    Ok(response)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn info_reports_request_error_and_latency_metrics() {
    let (addr, shutdown) = spawn_test_server().await.unwrap();
    let client = KVClient::connect(addr.to_string()).unwrap();

    assert_eq!(client.ping(None).unwrap(), b"PONG");
    client.set(b"metrics:key", b"value").unwrap();

    let response = send_raw(addr, b"*1\r\n$7\r\nUNKNOWN\r\n").unwrap();
    assert_eq!(response, b"-ERR unknown command\r\n");

    let info = String::from_utf8(client.info().unwrap()).unwrap();
    assert!(info.contains("engine:hybridkv"));
    assert!(info.contains("requests_total:4"), "{info}");
    assert!(info.contains("errors_total:1"), "{info}");
    assert!(info.contains("latency_samples:3"), "{info}");
    assert!(info.contains("error_rate:0.250"), "{info}");
    assert!(info.contains("latency_p50_us:"), "{info}");
    assert!(info.contains("latency_p99_us:"), "{info}");
    assert!(info.contains("qps_avg:"), "{info}");

    let _ = shutdown.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn protocol_errors_are_counted_in_metrics() {
    let (addr, shutdown) = spawn_test_server().await.unwrap();

    let response = send_raw(addr, b"*1\r\nnot-a-bulk-len\r\n").unwrap();
    assert_eq!(response, b"-ERR protocol error\r\n");

    let client = KVClient::connect(addr.to_string()).unwrap();
    let info = String::from_utf8(client.info().unwrap()).unwrap();
    assert!(info.contains("requests_total:2"), "{info}");
    assert!(info.contains("errors_total:1"), "{info}");
    assert!(info.contains("latency_samples:1"), "{info}");

    let _ = shutdown.send(());
}
