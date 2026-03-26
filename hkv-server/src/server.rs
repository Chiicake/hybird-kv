//! # TCP Server
//!
//! Accept RESP2 connections, parse commands, and dispatch them to the
//! storage engine with minimal overhead.

use std::future::Future;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use bytes::BytesMut;
use socket2::{SockRef, TcpKeepalive};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::task::JoinSet;

use hkv_engine::{KVEngine, TtlStatus};

use crate::metrics::Metrics;
use crate::observation::{
    AccessClass, CommandKind, ExperimentObservationSink, ObservationEvent, SharedObservationLog,
};
use crate::protocol::{RespError, RespParser};
use crate::tracker::{AccessOp, HotTracker};

const DEFAULT_SHUTDOWN_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_KEEPALIVE_TIME: Duration = Duration::from_secs(30);
const DEFAULT_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);
const DEFAULT_KEEPALIVE_RETRIES: u32 = 3;

#[derive(Clone, Copy)]
struct ServerConfig {
    shutdown_drain_timeout: Duration,
    keepalive_time: Duration,
    keepalive_interval: Duration,
    keepalive_retries: u32,
}

const DEFAULT_SERVER_CONFIG: ServerConfig = ServerConfig {
    shutdown_drain_timeout: DEFAULT_SHUTDOWN_DRAIN_TIMEOUT,
    keepalive_time: DEFAULT_KEEPALIVE_TIME,
    keepalive_interval: DEFAULT_KEEPALIVE_INTERVAL,
    keepalive_retries: DEFAULT_KEEPALIVE_RETRIES,
};

/// Serves accepted TCP connections until shutdown is triggered.
///
/// The shutdown signal stops new accepts immediately, then drains active
/// connections for a bounded grace period before aborting remaining tasks.
pub async fn serve_with_shutdown<E, F>(
    listener: tokio::net::TcpListener,
    engine: Arc<E>,
    metrics: Arc<Metrics>,
    shutdown: F,
) -> std::io::Result<()>
where
    E: KVEngine + 'static,
    F: Future<Output = ()>,
{
    serve_with_shutdown_with_observation(
        listener,
        engine,
        metrics,
        None,
        shutdown,
        DEFAULT_SERVER_CONFIG,
    )
    .await
}

pub async fn serve_with_shutdown_and_observation<E, F>(
    listener: tokio::net::TcpListener,
    engine: Arc<E>,
    metrics: Arc<Metrics>,
    observation_log: Arc<SharedObservationLog>,
    shutdown: F,
) -> std::io::Result<()>
where
    E: KVEngine + 'static,
    F: Future<Output = ()>,
{
    serve_with_shutdown_with_observation(
        listener,
        engine,
        metrics,
        Some(observation_log),
        shutdown,
        DEFAULT_SERVER_CONFIG,
    )
    .await
}

pub async fn serve_with_shutdown_and_tracker<E, F>(
    listener: tokio::net::TcpListener,
    engine: Arc<E>,
    metrics: Arc<Metrics>,
    tracker: Arc<Mutex<HotTracker>>,
    shutdown: F,
) -> std::io::Result<()>
where
    E: KVEngine + 'static,
    F: Future<Output = ()>,
{
    serve_with_shutdown_with_observation_and_tracker(
        listener,
        engine,
        metrics,
        None,
        Some(tracker),
        shutdown,
        DEFAULT_SERVER_CONFIG,
    )
    .await
}

/// Handles a single TCP client connection.
pub async fn handle_connection<E>(stream: TcpStream, engine: Arc<E>) -> std::io::Result<()>
where
    E: KVEngine,
{
    handle_connection_with_metrics(stream, engine, Arc::new(Metrics::new())).await
}

#[allow(dead_code)]
async fn serve_with_shutdown_config<E, F>(
    listener: tokio::net::TcpListener,
    engine: Arc<E>,
    metrics: Arc<Metrics>,
    shutdown: F,
    config: ServerConfig,
) -> std::io::Result<()>
where
    E: KVEngine + 'static,
    F: Future<Output = ()>,
{
    serve_with_shutdown_with_observation_and_tracker(
        listener, engine, metrics, None, None, shutdown, config,
    )
    .await
}

async fn serve_with_shutdown_with_observation<E, F>(
    listener: tokio::net::TcpListener,
    engine: Arc<E>,
    metrics: Arc<Metrics>,
    observation_log: Option<Arc<SharedObservationLog>>,
    shutdown: F,
    config: ServerConfig,
) -> std::io::Result<()>
where
    E: KVEngine + 'static,
    F: Future<Output = ()>,
{
    serve_with_shutdown_with_observation_and_tracker(
        listener,
        engine,
        metrics,
        observation_log,
        None,
        shutdown,
        config,
    )
    .await
}

async fn serve_with_shutdown_with_observation_and_tracker<E, F>(
    listener: tokio::net::TcpListener,
    engine: Arc<E>,
    metrics: Arc<Metrics>,
    observation_log: Option<Arc<SharedObservationLog>>,
    tracker: Option<Arc<Mutex<HotTracker>>>,
    shutdown: F,
    config: ServerConfig,
) -> std::io::Result<()>
where
    E: KVEngine + 'static,
    F: Future<Output = ()>,
{
    let listener = listener;
    let mut connections = JoinSet::new();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            Some(join_result) = connections.join_next(), if !connections.is_empty() => {
                reap_connection_task(join_result);
            }
            accept = listener.accept() => {
                let (stream, _) = accept?;
                configure_accepted_stream(&stream, config)?;
                let engine = Arc::clone(&engine);
                let metrics = Arc::clone(&metrics);
                let observation_log = observation_log.as_ref().map(Arc::clone);
                let tracker = tracker.as_ref().map(Arc::clone);
                connections.spawn(async move {
                    handle_connection_with_observation_and_tracker(
                        stream,
                        engine,
                        metrics,
                        observation_log,
                        tracker,
                    )
                    .await
                });
            }
        }
    }

    drop(listener);

    let drain = async {
        while let Some(join_result) = connections.join_next().await {
            reap_connection_task(join_result);
        }
    };

    if tokio::time::timeout(config.shutdown_drain_timeout, drain)
        .await
        .is_err()
    {
        connections.abort_all();
        while let Some(join_result) = connections.join_next().await {
            reap_connection_task(join_result);
        }
    }

    Ok(())
}

/// Handles a single TCP client connection with shared server metrics.
pub async fn handle_connection_with_metrics<E>(
    stream: TcpStream,
    engine: Arc<E>,
    metrics: Arc<Metrics>,
) -> std::io::Result<()>
where
    E: KVEngine,
{
    handle_connection_with_observation_and_tracker(stream, engine, metrics, None, None).await
}

pub async fn handle_connection_with_observation<E>(
    stream: TcpStream,
    engine: Arc<E>,
    metrics: Arc<Metrics>,
    observation_log: Option<Arc<SharedObservationLog>>,
) -> std::io::Result<()>
where
    E: KVEngine,
{
    handle_connection_with_observation_and_tracker(stream, engine, metrics, observation_log, None)
        .await
}

async fn handle_connection_with_observation_and_tracker<E>(
    stream: TcpStream,
    engine: Arc<E>,
    metrics: Arc<Metrics>,
    observation_log: Option<Arc<SharedObservationLog>>,
    tracker: Option<Arc<Mutex<HotTracker>>>,
) -> std::io::Result<()>
where
    E: KVEngine,
{
    let mut stream = stream;
    let mut buffer = BytesMut::with_capacity(8 * 1024);
    let mut parser = RespParser::new();

    loop {
        let bytes = stream.read_buf(&mut buffer).await?;
        if bytes == 0 {
            break;
        }

        loop {
            match parser.parse(&mut buffer) {
                Ok(Some(args)) => {
                    metrics.record_request_start();
                    let started_at = Instant::now();
                    let observation_sink = observation_sink(
                        observation_log.as_deref(),
                        tracker.as_deref(),
                    );
                    let response = dispatch_command(
                        &args,
                        engine.as_ref(),
                        metrics.as_ref(),
                        tracker.as_deref(),
                        observation_sink
                            .as_ref()
                            .map(|sink| sink as &dyn ExperimentObservationSink),
                    );
                    let write_result = stream.write_all(&response).await;
                    finish_tracked_request(metrics.as_ref(), started_at, &response, write_result)?;
                }
                Ok(None) => break,
                Err(RespError::Protocol) => {
                    metrics.record_request_start();
                    let started_at = Instant::now();
                    let response = resp_error("protocol error");
                    let write_result = stream.write_all(&response).await;
                    finish_tracked_request(metrics.as_ref(), started_at, &response, write_result)?;
                    return Ok(());
                }
            }
        }
    }

    Ok(())
}

fn dispatch_command(
    args: &[Vec<u8>],
    engine: &impl KVEngine,
    metrics: &Metrics,
    tracker: Option<&Mutex<HotTracker>>,
    observation_sink: Option<&dyn ExperimentObservationSink>,
) -> Vec<u8> {
    if args.is_empty() {
        return resp_error("empty command");
    }

    let cmd = &args[0];
    if eq_ignore_ascii_case(cmd, b"PING") {
        return handle_ping(args);
    }
    if eq_ignore_ascii_case(cmd, b"GET") {
        return observe_command_result(observation_sink, planned_observations(observation_sink, args), || {
            handle_get(args, engine)
        });
    }
    if eq_ignore_ascii_case(cmd, b"SET") {
        return observe_command_result(observation_sink, planned_observations(observation_sink, args), || {
            handle_set(args, engine)
        });
    }
    if eq_ignore_ascii_case(cmd, b"DEL") {
        return observe_command_result(observation_sink, planned_observations(observation_sink, args), || {
            handle_del(args, engine)
        });
    }
    if eq_ignore_ascii_case(cmd, b"EXPIRE") {
        return observe_command_result(observation_sink, planned_observations(observation_sink, args), || {
            handle_expire(args, engine)
        });
    }
    if eq_ignore_ascii_case(cmd, b"TTL") {
        return observe_command_result(observation_sink, planned_observations(observation_sink, args), || {
            handle_ttl(args, engine)
        });
    }
    if eq_ignore_ascii_case(cmd, b"INFO") {
        return handle_info(metrics, tracker);
    }

    resp_error("unknown command")
}

fn handle_ping(args: &[Vec<u8>]) -> Vec<u8> {
    match args.len() {
        1 => resp_simple("PONG"),
        2 => resp_bulk(&args[1]),
        _ => resp_error("wrong number of arguments for PING"),
    }
}

fn handle_get(args: &[Vec<u8>], engine: &impl KVEngine) -> Vec<u8> {
    if args.len() != 2 {
        return resp_error("wrong number of arguments for GET");
    }
    match engine.get(&args[1]) {
        Ok(Some(value)) => resp_bulk(&value),
        Ok(None) => resp_null(),
        Err(_) => resp_error("engine error"),
    }
}

fn handle_set(args: &[Vec<u8>], engine: &impl KVEngine) -> Vec<u8> {
    if args.len() < 3 {
        return resp_error("wrong number of arguments for SET");
    }

    let key = args[1].clone();
    let value = args[2].clone();

    if args.len() == 3 {
        if engine.set(key, value).is_ok() {
            return resp_simple("OK");
        }
        return resp_error("engine error");
    }

    if args.len() == 5 && eq_ignore_ascii_case(&args[3], b"EX") {
        let seconds = match parse_u64(&args[4]) {
            Ok(value) => value,
            Err(resp) => return resp,
        };

        if engine
            .set_with_ttl(key, value, Duration::from_secs(seconds))
            .is_err()
        {
            return resp_error("engine error");
        }

        return resp_simple("OK");
    }

    resp_error("unsupported SET options")
}

fn handle_del(args: &[Vec<u8>], engine: &impl KVEngine) -> Vec<u8> {
    if args.len() < 2 {
        return resp_error("wrong number of arguments for DEL");
    }

    let mut removed = 0i64;
    for key in &args[1..] {
        match engine.delete(key) {
            Ok(true) => removed += 1,
            Ok(false) => {}
            Err(_) => return resp_error("engine error"),
        }
    }

    resp_integer(removed)
}

fn handle_expire(args: &[Vec<u8>], engine: &impl KVEngine) -> Vec<u8> {
    if args.len() != 3 {
        return resp_error("wrong number of arguments for EXPIRE");
    }

    let seconds = match parse_u64(&args[2]) {
        Ok(value) => value,
        Err(resp) => return resp,
    };

    match engine.expire(&args[1], Duration::from_secs(seconds)) {
        Ok(()) => resp_integer(1),
        Err(hkv_common::HkvError::NotFound) => resp_integer(0),
        Err(_) => resp_error("engine error"),
    }
}

fn handle_ttl(args: &[Vec<u8>], engine: &impl KVEngine) -> Vec<u8> {
    if args.len() != 2 {
        return resp_error("wrong number of arguments for TTL");
    }

    match engine.ttl(&args[1]) {
        Ok(TtlStatus::Missing) => resp_integer(-2),
        Ok(TtlStatus::NoExpiry) => resp_integer(-1),
        Ok(TtlStatus::ExpiresIn(remaining)) => resp_integer(remaining.as_secs() as i64),
        Err(_) => resp_error("engine error"),
    }
}

fn handle_info(metrics: &Metrics, tracker: Option<&Mutex<HotTracker>>) -> Vec<u8> {
    let snapshot = metrics.snapshot();
    let average_us = snapshot.latency.average_us().unwrap_or(0.0);
    let p50_us = snapshot.latency.percentile_us(50.0).unwrap_or(0);
    let p90_us = snapshot.latency.percentile_us(90.0).unwrap_or(0);
    let p99_us = snapshot.latency.percentile_us(99.0).unwrap_or(0);
    let p999_us = snapshot.latency.percentile_us(99.9).unwrap_or(0);
    let mut info = format!(
        concat!(
            "role:master\r\n",
            "engine:hybridkv\r\n",
            "requests_total:{}\r\n",
            "errors_total:{}\r\n",
            "inflight:{}\r\n",
            "uptime_sec:{:.3}\r\n",
            "qps_avg:{:.3}\r\n",
            "error_rate:{:.3}\r\n",
            "latency_samples:{}\r\n",
            "latency_avg_us:{:.3}\r\n",
            "latency_max_us:{}\r\n",
            "latency_p50_us:{}\r\n",
            "latency_p90_us:{}\r\n",
            "latency_p99_us:{}\r\n",
            "latency_p999_us:{}\r\n"
        ),
        snapshot.requests_total,
        snapshot.errors_total,
        snapshot.inflight,
        snapshot.uptime.as_secs_f64(),
        snapshot.qps(),
        snapshot.error_rate(),
        snapshot.latency.samples,
        average_us,
        snapshot.latency.max_us,
        p50_us,
        p90_us,
        p99_us,
        p999_us,
    );

    append_hot_tracker_info(&mut info, tracker);
    resp_bulk(info.as_bytes())
}

fn append_hot_tracker_info(info: &mut String, tracker: Option<&Mutex<HotTracker>>) {
    let Some(tracker) = tracker else {
        info.push_str("hot_tracker_enabled:0\r\n");
        return;
    };

    let snapshot = tracker.lock().unwrap().latest_snapshot();
    info.push_str("hot_tracker_enabled:1\r\n");
    info.push_str(&format!(
        "hot_snapshot_observed_total_accesses:{}\r\n",
        snapshot.observed_total_accesses
    ));
    info.push_str(&format!(
        "hot_snapshot_candidate_count:{}\r\n",
        snapshot.candidates.len()
    ));

    for (index, candidate) in snapshot.candidates.iter().enumerate() {
        info.push_str(&format!(
            "hot_candidate_{index}_key_hex:{}\r\n",
            encode_hex(&candidate.key)
        ));
        info.push_str(&format!(
            "hot_candidate_{index}_total_accesses:{}\r\n",
            candidate.estimated_total_accesses
        ));
        info.push_str(&format!(
            "hot_candidate_{index}_read_accesses:{}\r\n",
            candidate.estimated_read_accesses
        ));
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn resp_simple(message: &str) -> Vec<u8> {
    let mut buf = Vec::with_capacity(message.len() + 3);
    buf.extend_from_slice(b"+");
    buf.extend_from_slice(message.as_bytes());
    buf.extend_from_slice(b"\r\n");
    buf
}

fn resp_error(message: &str) -> Vec<u8> {
    let mut buf = Vec::with_capacity(message.len() + 6);
    buf.extend_from_slice(b"-ERR ");
    buf.extend_from_slice(message.as_bytes());
    buf.extend_from_slice(b"\r\n");
    buf
}

fn resp_integer(value: i64) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b":");
    buf.extend_from_slice(value.to_string().as_bytes());
    buf.extend_from_slice(b"\r\n");
    buf
}

fn resp_bulk(data: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"$");
    buf.extend_from_slice(data.len().to_string().as_bytes());
    buf.extend_from_slice(b"\r\n");
    buf.extend_from_slice(data);
    buf.extend_from_slice(b"\r\n");
    buf
}

fn resp_null() -> Vec<u8> {
    b"$-1\r\n".to_vec()
}

fn observe_command_result<F>(
    sink: Option<&dyn ExperimentObservationSink>,
    events: Vec<ObservationEvent>,
    dispatch: F,
) -> Vec<u8>
where
    F: FnOnce() -> Vec<u8>,
{
    let response = dispatch();
    if !is_error_response(&response)
        && let Some(sink) = sink
    {
        for event in events {
            sink.record_observation(event);
        }
    }
    response
}

fn planned_observations(
    sink: Option<&dyn ExperimentObservationSink>,
    args: &[Vec<u8>],
) -> Vec<ObservationEvent> {
    if sink.is_none() {
        return Vec::new();
    }
    if args.is_empty() {
        return Vec::new();
    }

    let command = &args[0];
    if eq_ignore_ascii_case(command, b"GET") {
        return match args {
            [_, key] => vec![ObservationEvent::read(
                CommandKind::Get,
                key.clone(),
                SystemTime::now(),
            )],
            _ => Vec::new(),
        };
    }

    if eq_ignore_ascii_case(command, b"SET") {
        return match args {
            [_, key, value] => vec![ObservationEvent::write(
                CommandKind::Set,
                key.clone(),
                Some(value.len()),
                SystemTime::now(),
            )],
            [_, key, value, ex, _] if eq_ignore_ascii_case(ex, b"EX") => {
                vec![ObservationEvent::write(
                    CommandKind::Set,
                    key.clone(),
                    Some(value.len()),
                    SystemTime::now(),
                )]
            }
            _ => Vec::new(),
        };
    }

    if eq_ignore_ascii_case(command, b"DEL") {
        if args.len() < 2 {
            return Vec::new();
        }
        return args[1..]
            .iter()
            .map(|key| {
                ObservationEvent::write(CommandKind::Delete, key.clone(), None, SystemTime::now())
            })
            .collect();
    }

    if eq_ignore_ascii_case(command, b"EXPIRE") {
        return match args {
            [_, key, _] => vec![ObservationEvent::write(
                CommandKind::Expire,
                key.clone(),
                None,
                SystemTime::now(),
            )],
            _ => Vec::new(),
        };
    }

    if eq_ignore_ascii_case(command, b"TTL") {
        return match args {
            [_, key] => vec![ObservationEvent::read(
                CommandKind::Ttl,
                key.clone(),
                SystemTime::now(),
            )],
            _ => Vec::new(),
        };
    }

    Vec::new()
}

fn observation_sink<'a>(
    log: Option<&'a SharedObservationLog>,
    tracker: Option<&'a Mutex<HotTracker>>,
) -> Option<CompositeObservationSink<'a>> {
    if log.is_none() && tracker.is_none() {
        return None;
    }

    Some(CompositeObservationSink { log, tracker })
}

struct CompositeObservationSink<'a> {
    log: Option<&'a SharedObservationLog>,
    tracker: Option<&'a Mutex<HotTracker>>,
}

impl ExperimentObservationSink for CompositeObservationSink<'_> {
    fn record_observation(&self, event: ObservationEvent) {
        if let Some(log) = self.log {
            log.record_observation(event.clone());
        }

        if let Some(tracker) = self.tracker {
            let mut tracker = tracker.lock().unwrap();
            let op = match event.access {
                AccessClass::Read => AccessOp::Read,
                AccessClass::Write => AccessOp::Write,
            };
            tracker.record_access(&event.key, op, event.timestamp, event.value_size);
            tracker.publish_snapshot(event.timestamp);
        }
    }
}

fn is_error_response(response: &[u8]) -> bool {
    response.first() == Some(&b'-')
}

fn configure_accepted_stream(stream: &TcpStream, config: ServerConfig) -> std::io::Result<()> {
    stream.set_nodelay(true)?;

    let socket = SockRef::from(stream);
    socket.set_keepalive(true)?;

    #[cfg(not(any(target_os = "openbsd", target_os = "haiku")))]
    {
        let keepalive = build_tcp_keepalive(config);
        socket.set_tcp_keepalive(&keepalive)?;
    }

    Ok(())
}

fn finish_tracked_request(
    metrics: &Metrics,
    started_at: Instant,
    response: &[u8],
    write_result: std::io::Result<()>,
) -> std::io::Result<()> {
    if is_error_response(response) {
        metrics.record_error();
    }
    metrics.record_request_end(started_at.elapsed());
    write_result
}

fn reap_connection_task(join_result: Result<std::io::Result<()>, tokio::task::JoinError>) {
    if let Err(join_error) = join_result
        && join_error.is_panic()
    {
        std::panic::resume_unwind(join_error.into_panic());
    }
}

fn build_tcp_keepalive(config: ServerConfig) -> TcpKeepalive {
    let keepalive = TcpKeepalive::new().with_time(config.keepalive_time);
    let keepalive = with_keepalive_interval(keepalive, config.keepalive_interval);
    with_keepalive_retries(keepalive, config.keepalive_retries)
}

#[cfg(any(
    target_os = "android",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "fuchsia",
    target_os = "illumos",
    target_os = "ios",
    target_os = "visionos",
    target_os = "linux",
    target_os = "macos",
    target_os = "netbsd",
    target_os = "tvos",
    target_os = "watchos",
    target_os = "windows",
    target_os = "cygwin",
))]
fn with_keepalive_interval(keepalive: TcpKeepalive, interval: Duration) -> TcpKeepalive {
    keepalive.with_interval(interval)
}

#[cfg(not(any(
    target_os = "android",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "fuchsia",
    target_os = "illumos",
    target_os = "ios",
    target_os = "visionos",
    target_os = "linux",
    target_os = "macos",
    target_os = "netbsd",
    target_os = "tvos",
    target_os = "watchos",
    target_os = "windows",
    target_os = "cygwin",
)))]
fn with_keepalive_interval(keepalive: TcpKeepalive, _: Duration) -> TcpKeepalive {
    keepalive
}

#[cfg(any(
    target_os = "android",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "fuchsia",
    target_os = "illumos",
    target_os = "ios",
    target_os = "visionos",
    target_os = "linux",
    target_os = "macos",
    target_os = "netbsd",
    target_os = "tvos",
    target_os = "watchos",
    target_os = "cygwin",
    target_os = "windows",
))]
fn with_keepalive_retries(keepalive: TcpKeepalive, retries: u32) -> TcpKeepalive {
    keepalive.with_retries(retries)
}

#[cfg(not(any(
    target_os = "android",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "fuchsia",
    target_os = "illumos",
    target_os = "ios",
    target_os = "visionos",
    target_os = "linux",
    target_os = "macos",
    target_os = "netbsd",
    target_os = "tvos",
    target_os = "watchos",
    target_os = "cygwin",
    target_os = "windows",
)))]
fn with_keepalive_retries(keepalive: TcpKeepalive, _: u32) -> TcpKeepalive {
    keepalive
}

fn eq_ignore_ascii_case(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.eq_ignore_ascii_case(y))
}

fn parse_u64(arg: &[u8]) -> Result<u64, Vec<u8>> {
    if arg.is_empty() {
        return Err(resp_error("invalid integer"));
    }
    let mut value: u64 = 0;
    for &b in arg {
        if !b.is_ascii_digit() {
            return Err(resp_error("invalid integer"));
        }
        value = value.saturating_mul(10).saturating_add((b - b'0') as u64);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream as StdTcpStream;
    use std::sync::Mutex;
    use std::time::Duration;

    use hkv_common::HkvResult;
    use hkv_engine::MemoryEngine;
    use socket2::SockRef;
    use tokio::net::{TcpListener, TcpStream};
    use tokio::sync::oneshot;
    use tokio::time::timeout;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum FakeOp {
        Set(Vec<u8>, Vec<u8>),
        SetWithTtl(Vec<u8>, Vec<u8>, Duration),
        Delete(Vec<u8>),
        Expire(Vec<u8>, Duration),
        Ttl(Vec<u8>),
        Get(Vec<u8>),
    }

    #[derive(Default)]
    struct FakeEngine {
        ops: Mutex<Vec<FakeOp>>,
        fail_writes: bool,
    }

    impl FakeEngine {
        fn recorded_ops(&self) -> Vec<FakeOp> {
            self.ops.lock().unwrap().clone()
        }

        fn failing_writes() -> Self {
            Self {
                ops: Mutex::new(Vec::new()),
                fail_writes: true,
            }
        }
    }

    impl KVEngine for FakeEngine {
        fn get(&self, key: &[u8]) -> HkvResult<Option<Arc<[u8]>>> {
            self.ops.lock().unwrap().push(FakeOp::Get(key.to_vec()));
            Ok(None)
        }

        fn set(&self, key: Vec<u8>, value: Vec<u8>) -> HkvResult<()> {
            self.ops.lock().unwrap().push(FakeOp::Set(key, value));
            if self.fail_writes {
                return Err(hkv_common::HkvError::InternalError);
            }
            Ok(())
        }

        fn set_with_ttl(&self, key: Vec<u8>, value: Vec<u8>, ttl: Duration) -> HkvResult<()> {
            self.ops
                .lock()
                .unwrap()
                .push(FakeOp::SetWithTtl(key, value, ttl));
            if self.fail_writes {
                return Err(hkv_common::HkvError::InternalError);
            }
            Ok(())
        }

        fn delete(&self, key: &[u8]) -> HkvResult<bool> {
            self.ops.lock().unwrap().push(FakeOp::Delete(key.to_vec()));
            if self.fail_writes {
                return Err(hkv_common::HkvError::InternalError);
            }
            Ok(false)
        }

        fn expire(&self, key: &[u8], ttl: Duration) -> HkvResult<()> {
            self.ops
                .lock()
                .unwrap()
                .push(FakeOp::Expire(key.to_vec(), ttl));
            if self.fail_writes {
                return Err(hkv_common::HkvError::InternalError);
            }
            Ok(())
        }

        fn ttl(&self, key: &[u8]) -> HkvResult<TtlStatus> {
            self.ops.lock().unwrap().push(FakeOp::Ttl(key.to_vec()));
            Ok(TtlStatus::NoExpiry)
        }
    }

    fn test_server_config(shutdown_drain_timeout: Duration) -> ServerConfig {
        ServerConfig {
            shutdown_drain_timeout,
            ..DEFAULT_SERVER_CONFIG
        }
    }

    async fn spawn_server_for_test(
        shutdown_drain_timeout: Duration,
    ) -> (
        std::net::SocketAddr,
        oneshot::Sender<()>,
        tokio::task::JoinHandle<std::io::Result<()>>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let engine = Arc::new(MemoryEngine::new());
        let metrics = Arc::new(Metrics::new());
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let handle = tokio::spawn(async move {
            serve_with_shutdown_config(
                listener,
                engine,
                metrics,
                async {
                    let _ = shutdown_rx.await;
                },
                test_server_config(shutdown_drain_timeout),
            )
            .await
        });

        (addr, shutdown_tx, handle)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn configure_accepted_stream_enables_nodelay_and_keepalive() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let connect = tokio::spawn(async move { TcpStream::connect(addr).await.unwrap() });

        let (server_stream, _) = listener.accept().await.unwrap();
        let _client_stream = connect.await.unwrap();

        configure_accepted_stream(&server_stream, DEFAULT_SERVER_CONFIG).unwrap();

        assert!(server_stream.nodelay().unwrap());
        assert!(SockRef::from(&server_stream).keepalive().unwrap());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn serve_with_shutdown_waits_for_active_connections_to_close() {
        let (addr, shutdown, mut server_task) =
            spawn_server_for_test(Duration::from_millis(200)).await;
        let mut client = StdTcpStream::connect(addr).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        client.write_all(b"*1\r\n$4\r\nPING\r\n").unwrap();

        let mut pong = [0u8; 7];
        client.read_exact(&mut pong).unwrap();
        assert_eq!(&pong, b"+PONG\r\n");

        shutdown.send(()).unwrap();

        assert!(
            timeout(Duration::from_millis(50), &mut server_task)
                .await
                .is_err()
        );

        drop(client);

        let result = timeout(Duration::from_secs(1), &mut server_task)
            .await
            .unwrap();
        assert!(result.unwrap().is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn serve_with_shutdown_aborts_stuck_connections_after_timeout() {
        let (addr, shutdown, mut server_task) =
            spawn_server_for_test(Duration::from_millis(50)).await;
        let _client = StdTcpStream::connect(addr).unwrap();

        shutdown.send(()).unwrap();

        let result = timeout(Duration::from_secs(1), &mut server_task)
            .await
            .unwrap();
        assert!(result.unwrap().is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn serve_with_shutdown_closes_listener_after_shutdown() {
        let (addr, shutdown, mut server_task) =
            spawn_server_for_test(Duration::from_millis(50)).await;

        shutdown.send(()).unwrap();
        let result = timeout(Duration::from_secs(1), &mut server_task)
            .await
            .unwrap();
        assert!(result.unwrap().is_ok());

        assert!(StdTcpStream::connect(addr).is_err());
    }

    #[test]
    fn finish_tracked_request_releases_inflight_on_write_error() {
        let metrics = Metrics::new();
        metrics.record_request_start();

        let result = finish_tracked_request(
            &metrics,
            Instant::now(),
            b"+OK\r\n",
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "write failed",
            )),
        );

        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::BrokenPipe);
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.requests_total, 1);
        assert_eq!(snapshot.errors_total, 0);
        assert_eq!(snapshot.inflight, 0);
        assert_eq!(snapshot.latency.samples, 1);
    }

    #[test]
    fn finish_tracked_request_counts_error_responses_even_on_write_failure() {
        let metrics = Metrics::new();
        metrics.record_request_start();

        let result = finish_tracked_request(
            &metrics,
            Instant::now(),
            b"-ERR protocol error\r\n",
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "write failed",
            )),
        );

        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::BrokenPipe);
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.requests_total, 1);
        assert_eq!(snapshot.errors_total, 1);
        assert_eq!(snapshot.inflight, 0);
        assert_eq!(snapshot.latency.samples, 1);
    }

    #[test]
    fn set_ex_dispatches_only_set_with_ttl() {
        let engine = FakeEngine::default();
        let metrics = Metrics::new();

        let response = dispatch_command(
            &[
                b"SET".to_vec(),
                b"key".to_vec(),
                b"value".to_vec(),
                b"EX".to_vec(),
                b"10".to_vec(),
            ],
            &engine,
            &metrics,
            None,
        );

        assert_eq!(response, b"+OK\r\n");
        assert_eq!(
            engine.recorded_ops(),
            vec![FakeOp::SetWithTtl(
                b"key".to_vec(),
                b"value".to_vec(),
                Duration::from_secs(10)
            )]
        );
    }

    #[test]
    fn plain_set_still_dispatches_set() {
        let engine = FakeEngine::default();
        let metrics = Metrics::new();

        let response = dispatch_command(
            &[b"SET".to_vec(), b"key".to_vec(), b"value".to_vec()],
            &engine,
            &metrics,
            None,
        );

        assert_eq!(response, b"+OK\r\n");
        assert_eq!(
            engine.recorded_ops(),
            vec![FakeOp::Set(b"key".to_vec(), b"value".to_vec())]
        );
    }

    #[test]
    fn planned_observations_skip_wrong_arity_commands() {
        let no_sink: Option<&dyn ExperimentObservationSink> = None;
        assert!(planned_observations(no_sink, &[b"GET".to_vec()]).is_empty());
        assert!(planned_observations(no_sink, &[b"TTL".to_vec()]).is_empty());
        assert!(planned_observations(no_sink, &[b"DEL".to_vec()]).is_empty());
        assert!(planned_observations(no_sink, &[b"EXPIRE".to_vec(), b"key".to_vec()]).is_empty());
    }

    #[test]
    fn dispatch_command_skips_observation_on_engine_error() {
        let engine = FakeEngine::failing_writes();
        let metrics = Metrics::new();
        let observation_log = SharedObservationLog::default();

        let response = dispatch_command(
            &[b"SET".to_vec(), b"key".to_vec(), b"value".to_vec()],
            &engine,
            &metrics,
            Some(&observation_log),
        );

        assert_eq!(response, b"-ERR engine error\r\n");
        assert!(observation_log.observations().is_empty());
    }
}
