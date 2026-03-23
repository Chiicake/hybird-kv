mod commands;
mod info_poller;
mod models;
mod server_manager;
mod state;

fn main() {
    commands::register_commands(tauri::Builder::default())
        .run(tauri::generate_context!())
        .expect("failed to run HybridKV desktop shell");
}
