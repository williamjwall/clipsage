use std::sync::Arc;
use std::time::Duration;
use tauri::{Manager, State};
use tokio::sync::Mutex;
use arboard::Clipboard;
use uuid::Uuid;
use chrono::Utc;

mod database;
mod ollama;
use database::{Database, ClipItem};

type DbState = Arc<Mutex<Database>>;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! Welcome to ClipSage!", name)
}

#[tauri::command]
async fn hide_window(window: tauri::Window) -> Result<(), String> {
    window.hide().map_err(|e| e.to_string())
}

#[tauri::command]
async fn show_window(window: tauri::Window) -> Result<(), String> {
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())
}

#[tauri::command]
async fn search_clips(query: String, db: State<'_, DbState>) -> Result<Vec<ClipItem>, String> {
    let db = db.lock().await;
    if query.trim().is_empty() {
        db.get_recent_clips(50).await.map_err(|e| e.to_string())
    } else {
        db.search_clips(&query, 50).await.map_err(|e| e.to_string())
    }
}

#[tauri::command]
async fn get_recent_clips(db: State<'_, DbState>) -> Result<Vec<ClipItem>, String> {
    let db = db.lock().await;
    db.get_recent_clips(50).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn semantic_search_clips(query: String, db: State<'_, DbState>) -> Result<Vec<ClipItem>, String> {
    let db = db.lock().await;
    // For now, we'll use a simple embedding of the query text
    // In a production system, you'd want to use a proper embedding model
    let query_embedding: Vec<f32> = query
        .chars()
        .map(|c| c as u32 as f32 / 255.0)
        .collect();
    
    db.semantic_search(&query_embedding, 50).await.map_err(|e| e.to_string())
}

async fn start_clipboard_monitor(db: DbState) {
    let mut clipboard = match Clipboard::new() {
        Ok(cb) => cb,
        Err(e) => {
            eprintln!("Failed to initialize clipboard: {}", e);
            return;
        }
    };

    let mut last_content = String::new();

    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;

        if let Ok(content) = clipboard.get_text() {
            if content != last_content && !content.trim().is_empty() && content.len() > 3 {
                last_content = content.clone();
                
                // Generate a simple summary (first 50 chars or first line)
                let summary = if content.len() > 50 {
                    format!("{}...", &content[..47])
                } else {
                    content.lines().next().unwrap_or(&content).to_string()
                };

                // Simple tag generation based on content
                let mut tags = Vec::new();
                if content.contains("http") || content.contains("www") {
                    tags.push("url".to_string());
                }
                if content.contains("function") || content.contains("const") || content.contains("let") {
                    tags.push("code".to_string());
                }
                if content.contains("@") && content.contains(".") {
                    tags.push("email".to_string());
                }
                if content.len() > 200 {
                    tags.push("long-text".to_string());
                }

                let clip_item = ClipItem {
                    id: Uuid::new_v4().to_string(),
                    content: content.clone(),
                    summary,
                    tags,
                    timestamp: Utc::now(),
                    source: Some("clipboard".to_string()),
                    embedding: Some(content
                        .chars()
                        .map(|c| c as u32 as f32 / 255.0)
                        .collect()),
                };

                let db = db.lock().await;
                if let Err(e) = db.insert_clip(&clip_item).await {
                    eprintln!("Failed to insert clip: {}", e);
                }
            }
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_handle = app.handle().clone();
            
            tauri::async_runtime::spawn(async move {
                // Initialize database with proper SQLite file URL
                let data_dir = app_handle.path().app_data_dir().unwrap();
                std::fs::create_dir_all(&data_dir).unwrap();
                let db_path = data_dir.join("clipsage.db");
                println!("Attempting to create database at: {}", db_path.display());
                
                let database = match Database::new(&format!("sqlite://{}?mode=rwc", db_path.display())).await {
                    Ok(db) => {
                        println!("Database initialized successfully!");
                        Arc::new(Mutex::new(db))
                    },
                    Err(e) => {
                        eprintln!("Failed to initialize database: {}", e);
                        eprintln!("Current directory: {:?}", std::env::current_dir());
                        return;
                    }
                };

                // Store database in app state
                app_handle.manage(database.clone());

                println!("Starting clipboard monitoring...");
                // Start clipboard monitoring
                start_clipboard_monitor(database).await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet, 
            hide_window, 
            show_window, 
            search_clips, 
            get_recent_clips,
            semantic_search_clips
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
