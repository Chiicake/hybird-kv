use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream as StdTcpStream};
use std::sync::Arc;
use std::time::Duration;

use hkv_client::{ClientTtl, KVClient};
use hkv_engine::MemoryEngine;
use hkv_server::metrics::Metrics;
use hkv_server::phase2a_testing::{AccessClass, CommandKind, SharedObservationLog};
use hkv_server::server;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

async fn spawn_test_server(
) -> std::io::Result<(SocketAddr, Arc<SharedObservationLog>, oneshot::Sender<()>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let engine = Arc::new(MemoryEngine::new());
    let metrics = Arc::new(Metrics::new());
    let observation_log = Arc::new(SharedObservationLog::default());
    let expirer = engine.start_expirer(Duration::from_millis(50));

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let server_metrics = Arc::clone(&metrics);
    let server_observation_log = Arc::clone(&observation_log);

    tokio::spawn(async move {
        let mut expirer = Some(expirer);
        let _ = server::serve_with_shutdown_and_observation(
            listener,
            engine,
            server_metrics,
            server_observation_log,
            async {
                let _ = shutdown_rx.await;
            },
        )
        .await;

        if let Some(handle) = expirer.take() {
            handle.stop();
        }
    });

    Ok((addr, observation_log, shutdown_tx))
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
async fn request_path_records_observation_events() {
    let (addr, observation_log, shutdown) = spawn_test_server().await.unwrap();
    let client = KVClient::connect(addr.to_string()).unwrap();

    client.set(b"obs:set", b"value").unwrap();
    assert_eq!(client.get(b"obs:set").unwrap(), Some(b"value".to_vec()));
    assert!(client.expire(b"obs:set", Duration::from_secs(10)).unwrap());
    assert!(matches!(client.ttl(b"obs:set").unwrap(), ClientTtl::ExpiresIn(_)));
    client.delete(b"obs:set").unwrap();

    let unknown = send_raw(addr, b"*1\r\n$7\r\nUNKNOWN\r\n").unwrap();
    assert_eq!(unknown, b"-ERR unknown command\r\n");

    let malformed = send_raw(addr, b"*x\r\n").unwrap();
    assert_eq!(malformed, b"-ERR protocol error\r\n");

    let events = observation_log.observations();
    assert_eq!(events.len(), 5, "unexpected events: {events:#?}");

    assert_eq!(events[0].command, CommandKind::Set);
    assert_eq!(events[0].key, b"obs:set".to_vec());
    assert_eq!(events[0].access, AccessClass::Write);
    assert_eq!(events[0].value_size, Some(5));

    assert_eq!(events[1].command, CommandKind::Get);
    assert_eq!(events[1].key, b"obs:set".to_vec());
    assert_eq!(events[1].access, AccessClass::Read);
    assert_eq!(events[1].value_size, None);

    assert_eq!(events[2].command, CommandKind::Expire);
    assert_eq!(events[2].key, b"obs:set".to_vec());
    assert_eq!(events[2].access, AccessClass::Write);
    assert_eq!(events[2].value_size, None);

    assert_eq!(events[3].command, CommandKind::Ttl);
    assert_eq!(events[3].key, b"obs:set".to_vec());
    assert_eq!(events[3].access, AccessClass::Read);
    assert_eq!(events[3].value_size, None);

    assert_eq!(events[4].command, CommandKind::Delete);
    assert_eq!(events[4].key, b"obs:set".to_vec());
    assert_eq!(events[4].access, AccessClass::Write);
    assert_eq!(events[4].value_size, None);

    for event in &events {
        assert!(event.timestamp.duration_since(std::time::UNIX_EPOCH).is_ok());
    }

    let _ = shutdown.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invalid_and_failed_commands_do_not_record_misleading_observation_events() {
    let (addr, observation_log, shutdown) = spawn_test_server().await.unwrap();

    let wrong_arity_get = send_raw(addr, b"*1\r\n$3\r\nGET\r\n").unwrap();
    assert_eq!(wrong_arity_get, b"-ERR wrong number of arguments for GET\r\n");

    let wrong_arity_ttl = send_raw(addr, b"*1\r\n$3\r\nTTL\r\n").unwrap();
    assert_eq!(wrong_arity_ttl, b"-ERR wrong number of arguments for TTL\r\n");

    let invalid_expire = send_raw(
        addr,
        b"*3\r\n$6\r\nEXPIRE\r\n$3\r\nkey\r\n$1\r\nx\r\n",
    )
    .unwrap();
    assert_eq!(invalid_expire, b"-ERR invalid integer\r\n");

    let invalid_set = send_raw(
        addr,
        b"*5\r\n$3\r\nSET\r\n$3\r\nkey\r\n$5\r\nvalue\r\n$2\r\nPX\r\n$1\r\n1\r\n",
    )
    .unwrap();
    assert_eq!(invalid_set, b"-ERR unsupported SET options\r\n");

    assert!(observation_log.observations().is_empty());

    let _ = shutdown.send(());
}
