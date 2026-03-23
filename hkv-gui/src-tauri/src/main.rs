use tauri::Emitter;
use tauri::Manager;

mod commands;
mod benchmark_manager;
mod info_poller;
mod models;
mod run_repository;
mod runners;
mod server_manager;
mod state;

fn main() {
    commands::register_commands(tauri::Builder::default())
        .setup(|app| {
            let handle = app.handle().clone();
            let state = app.state::<state::AppState>();
            let mut lifecycle = state.subscribe_benchmark_lifecycle();

            tauri::async_runtime::spawn(async move {
                loop {
                    match lifecycle.recv().await {
                        Ok(event) => {
                            let _ = handle.emit("benchmark:lifecycle", event);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run HybridKV desktop shell");
}
