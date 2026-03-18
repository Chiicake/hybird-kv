use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream as StdTcpStream};
use std::sync::{Arc, Barrier};
use std::thread;
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
async fn concurrent_clients_can_read_and_write_independent_keys() {
    const CLIENTS: usize = 8;
    const OPS_PER_CLIENT: usize = 16;

    let (addr, shutdown) = spawn_test_server().await.unwrap();
    let addr = addr.to_string();
    let barrier = Arc::new(Barrier::new(CLIENTS));
    let mut handles = Vec::with_capacity(CLIENTS);

    for client_id in 0..CLIENTS {
        let addr = addr.clone();
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            let client = KVClient::connect(addr).expect("connect");
            barrier.wait();

            for op in 0..OPS_PER_CLIENT {
                let key = format!("concurrent:{client_id}:{op}");
                let value = format!("value:{client_id}:{op}");

                client.set(key.as_bytes(), value.as_bytes()).expect("set");
                let fetched = client.get(key.as_bytes()).expect("get");
                assert_eq!(fetched.as_deref(), Some(value.as_bytes()));

                if op % 2 == 0 {
                    assert!(client.delete(key.as_bytes()).expect("delete"));
                    assert_eq!(client.get(key.as_bytes()).expect("get after delete"), None);
                    client
                        .set(key.as_bytes(), value.as_bytes())
                        .expect("restore");
                }
            }
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    let verifier = KVClient::connect(addr).unwrap();
    for client_id in 0..CLIENTS {
        for op in [0, OPS_PER_CLIENT - 1] {
            let key = format!("concurrent:{client_id}:{op}");
            let value = format!("value:{client_id}:{op}");
            let fetched = verifier.get(key.as_bytes()).unwrap();
            assert_eq!(fetched.as_deref(), Some(value.as_bytes()));
        }
    }

    let _ = shutdown.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn command_errors_return_stable_resp_errors_without_closing_connection() {
    let (addr, shutdown) = spawn_test_server().await.unwrap();
    let response = send_raw(
        addr,
        concat!(
            "*1\r\n$3\r\nGET\r\n",
            "*3\r\n$6\r\nEXPIRE\r\n$3\r\nkey\r\n$4\r\nnope\r\n",
            "*5\r\n$3\r\nSET\r\n$3\r\nkey\r\n$5\r\nvalue\r\n$2\r\nPX\r\n$1\r\n1\r\n",
            "*1\r\n$4\r\nPING\r\n"
        )
        .as_bytes(),
    )
    .unwrap();

    assert_eq!(
        response,
        concat!(
            "-ERR wrong number of arguments for GET\r\n",
            "-ERR invalid integer\r\n",
            "-ERR unsupported SET options\r\n",
            "+PONG\r\n"
        )
        .as_bytes()
    );

    let client = KVClient::connect(addr.to_string()).unwrap();
    assert_eq!(client.ping(None).unwrap(), b"PONG");

    let _ = shutdown.send(());
}
