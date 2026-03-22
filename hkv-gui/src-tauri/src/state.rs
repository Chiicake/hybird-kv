use crate::models::{InfoSnapshot, NormalizedRunSummary, ServerStatus};

#[derive(Debug, Default)]
pub struct AppState;

impl AppState {
    pub fn list_runs(&self) -> Vec<NormalizedRunSummary> {
        Vec::new()
    }

    pub fn server_status(&self) -> ServerStatus {
        ServerStatus {
            state: "stopped".into(),
            address: "127.0.0.1:6380".into(),
            pid: None,
            started_at: None,
            last_error: None,
        }
    }

    pub fn info_snapshot(&self) -> Option<InfoSnapshot> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::AppState;

    #[test]
    fn app_state_provides_default_contract_handles() {
        let state = AppState::default();

        assert!(state.list_runs().is_empty());
        assert_eq!(state.server_status().state, "stopped");
        assert_eq!(state.server_status().address, "127.0.0.1:6380");
        assert!(state.info_snapshot().is_none());
    }
}
