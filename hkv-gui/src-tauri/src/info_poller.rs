use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use hkv_client::{ClientConfig, KVClient};

use crate::models::InfoSnapshot;
use crate::server_manager::parse_info_snapshot;

pub struct InfoPoller {
    client_factory: Box<dyn InfoClientFactory>,
    state: Mutex<InfoPollerState>,
}

struct InfoPollerState {
    last_snapshot: Option<InfoSnapshot>,
    last_error: Option<String>,
}

pub trait InfoClient: Send {
    fn info(&self) -> Result<String, String>;
}

pub trait InfoClientFactory: Send + Sync {
    fn connect(&self, address: &str) -> Result<Box<dyn InfoClient>, String>;
}

struct KvInfoClient {
    inner: KVClient,
}

impl InfoClient for KvInfoClient {
    fn info(&self) -> Result<String, String> {
        let bytes = self.inner.info().map_err(|err| err.to_string())?;
        String::from_utf8(bytes).map_err(|err| err.to_string())
    }
}

pub struct KvInfoClientFactory;

impl InfoClientFactory for KvInfoClientFactory {
    fn connect(&self, address: &str) -> Result<Box<dyn InfoClient>, String> {
        let client = KVClient::with_config(ClientConfig {
            addr: address.to_string(),
            connect_timeout: Some(Duration::from_millis(200)),
            read_timeout: Some(Duration::from_millis(200)),
            write_timeout: Some(Duration::from_millis(200)),
            ..ClientConfig::default()
        })
        .map_err(|err| err.to_string())?;
        Ok(Box::new(KvInfoClient { inner: client }))
    }
}

impl Default for InfoPoller {
    fn default() -> Self {
        Self::new()
    }
}

impl InfoPoller {
    pub fn new() -> Self {
        Self::with_client_factory(Box::new(KvInfoClientFactory))
    }

    pub fn with_client_factory(client_factory: Box<dyn InfoClientFactory>) -> Self {
        Self {
            client_factory,
            state: Mutex::new(InfoPollerState {
                last_snapshot: None,
                last_error: None,
            }),
        }
    }

    pub fn poll(&self, address: &str) -> Result<InfoSnapshot, String> {
        let client = match self.client_factory.connect(address) {
            Ok(client) => client,
            Err(err) => {
                self.record_error(err.clone());
                return Err(err);
            }
        };
        let raw = match client.info() {
            Ok(raw) => raw,
            Err(err) => {
                self.record_error(err.clone());
                return Err(err);
            }
        };
        let captured_at = format!(
            "{}Z",
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_else(|_| Duration::from_secs(0))
                .as_secs()
        );
        let snapshot = match parse_info_snapshot(&captured_at, &raw) {
            Ok(snapshot) => snapshot,
            Err(err) => {
                self.record_error(err.clone());
                return Err(err);
            }
        };
        let mut state = self.state.lock().expect("info poller mutex poisoned");
        state.last_error = None;
        state.last_snapshot = Some(snapshot.clone());
        Ok(snapshot)
    }

    pub fn snapshot(&self) -> Option<InfoSnapshot> {
        self.state
            .lock()
            .expect("info poller mutex poisoned")
            .last_snapshot
            .clone()
    }

    pub fn clear(&self) {
        let mut state = self.state.lock().expect("info poller mutex poisoned");
        state.last_snapshot = None;
        state.last_error = None;
    }

    pub fn take_error(&self) -> Option<String> {
        self.state
            .lock()
            .expect("info poller mutex poisoned")
            .last_error
            .clone()
    }

    fn record_error(&self, message: String) {
        let mut state = self.state.lock().expect("info poller mutex poisoned");
        state.last_error = Some(message);
    }
}
