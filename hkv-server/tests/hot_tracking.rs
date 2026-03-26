use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream as StdTcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hkv_client::KVClient;
use hkv_engine::MemoryEngine;
use hkv_server::metrics::Metrics;
use hkv_server::server;
use hkv_server::tracker::{HotTracker, TrackerConfig};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

#[path = "support/hotness_workload_support.rs"]
mod support;

async fn spawn_test_server()
-> std::io::Result<(std::net::SocketAddr, Arc<Mutex<HotTracker>>, oneshot::Sender<()>)> {
    support::spawn_tracker_server(TrackerConfig {
        candidate_limit: 8,
        max_value_size: 1024,
        registry_capacity: 64,
        max_key_bytes: 256,
        cms_width: 128,
        cms_depth: 4,
        window_duration: Duration::from_secs(30),
        min_recent_accesses: 1,
        min_read_ratio_percent: 0,
        max_idle_age: Duration::from_secs(120),
    })
    .await
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
async fn request_path_records_hot_keys() {
    let (addr, tracker, shutdown) = spawn_test_server().await.unwrap();
    let client = KVClient::connect(addr.to_string()).unwrap();

    client.set(b"alpha", b"value-a").unwrap();
    client.set(b"beta", b"value-b").unwrap();

    for _ in 0..3 {
        assert_eq!(client.get(b"alpha").unwrap(), Some(b"value-a".to_vec()));
    }
    assert_eq!(client.get(b"beta").unwrap(), Some(b"value-b".to_vec()));

    let bad_arity = send_raw(addr, b"*1\r\n$3\r\nGET\r\n").unwrap();
    assert_eq!(bad_arity, b"-ERR wrong number of arguments for GET\r\n");

    let protocol_error = send_raw(addr, b"*1\r\nnot-a-bulk-len\r\n").unwrap();
    assert_eq!(protocol_error, b"-ERR protocol error\r\n");

    let unsupported_set = send_raw(
        addr,
        b"*4\r\n$3\r\nSET\r\n$5\r\nalpha\r\n$1\r\nx\r\n$2\r\nNX\r\n",
    )
    .unwrap();
    assert_eq!(unsupported_set, b"-ERR unsupported SET options\r\n");

    let invalid_expire = send_raw(
        addr,
        b"*3\r\n$6\r\nEXPIRE\r\n$5\r\nalpha\r\n$3\r\nabc\r\n",
    )
    .unwrap();
    assert_eq!(invalid_expire, b"-ERR invalid integer\r\n");

    let snapshot = tracker.lock().unwrap().latest_snapshot();
    assert_eq!(snapshot.observed_total_accesses, 6, "{snapshot:?}");
    assert!(!snapshot.candidates.is_empty(), "{snapshot:?}");
    assert_eq!(snapshot.candidates[0].key, b"alpha".to_vec(), "{snapshot:?}");
    assert_eq!(
        snapshot.candidates[0].estimated_total_accesses,
        4,
        "{snapshot:?}"
    );
    assert_eq!(
        snapshot.candidates
            .iter()
            .find(|candidate| candidate.key == b"beta".to_vec())
            .unwrap()
            .estimated_total_accesses,
        2,
        "{snapshot:?}"
    );

    let _ = shutdown.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_surface() {
    let (addr, _tracker, shutdown) = spawn_test_server().await.unwrap();
    let client = KVClient::connect(addr.to_string()).unwrap();

    client.set(b"alpha", b"value-a").unwrap();
    client.set(b"beta", b"value-b").unwrap();

    for _ in 0..3 {
        assert_eq!(client.get(b"alpha").unwrap(), Some(b"value-a".to_vec()));
    }
    assert_eq!(client.get(b"beta").unwrap(), Some(b"value-b".to_vec()));

    let info = String::from_utf8(client.info().unwrap()).unwrap();

    assert!(info.contains("hot_tracker_enabled:1"), "{info}");
    assert!(info.contains("hot_snapshot_observed_total_accesses:6"), "{info}");
    assert!(info.contains("hot_snapshot_candidate_count:2"), "{info}");
    assert!(info.contains("hot_candidate_0_key_hex:616c706861"), "{info}");
    assert!(info.contains("hot_candidate_0_total_accesses:4"), "{info}");
    assert!(info.contains("hot_candidate_0_read_accesses:3"), "{info}");
    assert!(info.contains("hot_candidate_1_key_hex:62657461"), "{info}");
    assert!(info.contains("hot_candidate_1_total_accesses:2"), "{info}");
    assert!(!info.contains("hot_candidate_2_"), "{info}");

    let _ = shutdown.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_surface_reports_tracker_disabled_without_tracker_context() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
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

    let client = KVClient::connect(addr.to_string()).unwrap();
    let info = String::from_utf8(client.info().unwrap()).unwrap();

    assert!(info.contains("hot_tracker_enabled:0"), "{info}");
    assert!(!info.contains("hot_candidate_0_"), "{info}");

    let _ = shutdown_tx.send(());
}
