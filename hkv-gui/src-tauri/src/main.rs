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
        .run(tauri::generate_context!())
        .expect("failed to run HybridKV desktop shell");
}
