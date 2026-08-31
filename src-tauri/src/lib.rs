mod importer;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            importer::import_file,
            importer::list_imported,
            importer::read_parquet,
            importer::list_cache,
            importer::delete_cache,
            importer::export_files,
            importer::scan_quality,
            importer::get_columns,
            importer::merge_files
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
