// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#, windows_subsystem = "windows")]

use tauri::{CustomMenuItem, Menu, MenuItem, Submenu};

fn main() {
    let open = CustomMenuItem::new("open".to_string(), "打开文件");
    let menu = Menu::new()
        .add_submenu(Submenu::new("文件", Menu::new().add_item(open)));

    tauri::Builder::default()
        .menu(menu)
        .on_menu_event(|event| {
            match event.menu_item_id() {
                "open" => {
                    println!("打开文件菜单被点击");
                }
                _ => {}
            }
        })
        .setup(|app| {
            let args: Vec<String> = std::env::args().collect();
            println!("启动参数: {:?}", args);
            
            if args.len() > 1 {
                for arg in args.iter().skip(1) {
                    if arg.ends_with(".ncm") {
                        println!("通过文件关联打开的文件: {}", arg);
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.emit("file-opened", arg);
                        }
                    }
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
