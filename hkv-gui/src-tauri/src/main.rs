mod commands;
mod models;
mod state;

fn main() {
    commands::register_commands(tauri::Builder::default())
        .run(tauri::generate_context!())
        .expect("failed to run HybridKV desktop shell");
}
