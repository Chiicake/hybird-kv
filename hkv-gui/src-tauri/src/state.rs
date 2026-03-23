use crate::info_poller::InfoPoller;
use crate::models::{InfoSnapshot, NormalizedRunSummary, ServerStatus, StartServerRequest};
use crate::server_manager::ServerManager;

pub struct AppState {
    server_manager: ServerManager,
    info_poller: InfoPoller,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            server_manager: ServerManager::new(),
            info_poller: InfoPoller::new(),
        }
    }
}

impl AppState {
    pub(crate) fn with_parts(server_manager: ServerManager, info_poller: InfoPoller) -> Self {
        Self {
            server_manager,
            info_poller,
        }
    }

    pub fn list_runs(&self) -> Vec<NormalizedRunSummary> {
        Vec::new()
    }

    pub fn start_server(
        &self,
        request: Option<StartServerRequest>,
    ) -> Result<ServerStatus, String> {
        self.server_manager.start(request)
    }

    pub fn stop_server(&self) -> Result<ServerStatus, String> {
        let status = self.server_manager.stop()?;
        self.info_poller.clear();
        Ok(status)
    }

    pub fn server_status(&self) -> ServerStatus {
        let mut status = self.server_manager.status();
        if let Some(error) = self.info_poller.take_error() {
            status.last_error = Some(error);
        }
        status
    }

    pub fn info_snapshot(&self) -> Option<InfoSnapshot> {
        let status = self.server_manager.status();
        if status.state == "running" {
            match self.info_poller.poll(&status.address) {
                Ok(snapshot) => Some(snapshot),
                Err(_) => None,
            }
        } else {
            self.info_poller.snapshot()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AppState;
    use crate::info_poller::{InfoClient, InfoClientFactory, InfoPoller};
    use crate::server_manager::{LaunchSpec, ManagedChild, ProcessLauncher, ServerManager};
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct FakeLauncher {
        starts: Arc<Mutex<VecDeque<FakeChild>>>,
    }

    impl FakeLauncher {
        fn running(pid: u32) -> Self {
            Self {
                starts: Arc::new(Mutex::new(vec![FakeChild::running(pid)].into())),
            }
        }
    }

    impl ProcessLauncher for FakeLauncher {
        fn spawn(&self, _spec: &LaunchSpec) -> Result<Box<dyn ManagedChild>, String> {
            let child = self
                .starts
                .lock()
                .expect("starts mutex poisoned")
                .pop_front()
                .expect("fake child should exist");
            Ok(Box::new(child))
        }
    }

    struct FakeChild {
        pid: u32,
        waits: VecDeque<Result<Option<i32>, std::io::Error>>,
    }

    impl FakeChild {
        fn running(pid: u32) -> Self {
            Self {
                pid,
                waits: vec![Ok(None), Ok(None), Ok(None)].into(),
            }
        }
    }

    impl ManagedChild for FakeChild {
        fn id(&self) -> u32 {
            self.pid
        }

        fn try_wait(&mut self) -> std::io::Result<Option<i32>> {
            self.waits.pop_front().unwrap_or(Ok(None))
        }

        fn kill(&mut self) -> std::io::Result<()> {
            Ok(())
        }

        fn wait(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    struct FailingClientFactory;

    impl InfoClientFactory for FailingClientFactory {
        fn connect(&self, _address: &str) -> Result<Box<dyn InfoClient>, String> {
            Ok(Box::new(FailingClient))
        }
    }

    struct FailingClient;

    impl InfoClient for FailingClient {
        fn info(&self) -> Result<String, String> {
            Err("poll failed".into())
        }
    }

    #[test]
    fn app_state_provides_default_contract_handles() {
        let state = AppState::default();

        assert!(state.list_runs().is_empty());
        assert_eq!(state.server_status().state, "stopped");
        assert_eq!(state.server_status().address, "127.0.0.1:6380");
        assert!(state.info_snapshot().is_none());
    }

    #[test]
    fn app_state_surfaces_info_poll_failures_through_server_status() {
        let manager = ServerManager::with_launcher(Box::new(FakeLauncher::running(4242)));
        manager
            .start(None)
            .expect("server manager should report running state");
        let poller = InfoPoller::with_client_factory(Box::new(FailingClientFactory));
        let state = AppState::with_parts(manager, poller);

        assert!(state.info_snapshot().is_none());
        assert_eq!(state.server_status().last_error, Some("poll failed".into()));
    }
}
