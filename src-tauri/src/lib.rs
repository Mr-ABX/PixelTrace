use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Position, Size, WebviewWindow,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

static IS_GHOST_MODE: AtomicBool = AtomicBool::new(false);

#[derive(serde::Serialize)]
struct MonitorInfo {
    name: Option<String>,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    scale_factor: f64,
}

#[cfg(target_os = "macos")]
fn setup_macos_window(window: &WebviewWindow) {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};

    if let Ok(ns_win_ptr) = window.ns_window() {
        let ns_win = ns_win_ptr as *mut AnyObject;
        unsafe {
            // NSWindowCollectionBehavior:
            // 1   = NSWindowCollectionBehaviorCanJoinAllSpaces
            // 16  = NSWindowCollectionBehaviorStationary
            // 64  = NSWindowCollectionBehaviorIgnoresCycle
            // 256 = NSWindowCollectionBehaviorFullScreenAuxiliary
            let behavior: usize = 1 | 16 | 64 | 256;
            let _: () = msg_send![ns_win, setCollectionBehavior: behavior];

            // Window Level: NSStatusWindowLevel = 25 (above normal windows and standard menu bar)
            let level: isize = 25;
            let _: () = msg_send![ns_win, setLevel: level];

            let _: () = msg_send![ns_win, setHasShadow: false];
            let _: () = msg_send![ns_win, setOpaque: false];
            let _: () = msg_send![ns_win, setAcceptsMouseMovedEvents: true];
            let _: () = msg_send![ns_win, setTitlebarAppearsTransparent: true];

            // Hide standard traffic-light buttons so window controls never drop down
            for button_id in 0..3isize {
                let button: *mut AnyObject = msg_send![ns_win, standardWindowButton: button_id];
                if !button.is_null() {
                    let _: () = msg_send![button, setHidden: true];
                }
            }

            if let Some(ns_color_cls) = AnyClass::get(c"NSColor") {
                let clear_color: *mut AnyObject = msg_send![ns_color_cls, clearColor];
                let _: () = msg_send![ns_win, setBackgroundColor: clear_color];
            }
        }
    }
}

#[tauri::command]
fn set_click_through(window: WebviewWindow, ignore: bool) -> Result<(), String> {
    IS_GHOST_MODE.store(ignore, Ordering::SeqCst);
    window.set_ignore_cursor_events(ignore).map_err(|e| e.to_string())?;

    #[cfg(target_os = "macos")]
    {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;
        if let Ok(ns_win_ptr) = window.ns_window() {
            let ns_win = ns_win_ptr as *mut AnyObject;
            unsafe {
                let _: () = msg_send![ns_win, setIgnoresMouseEvents: ignore];
            }
        }
    }

    if !ignore {
        let _ = window.set_focus();
    }

    let _ = window.emit("ghost-mode-changed", ignore);
    Ok(())
}

#[tauri::command]
fn toggle_ghost_mode(app: AppHandle) -> Result<bool, String> {
    if let Some(window) = app.get_webview_window("main") {
        let current = IS_GHOST_MODE.load(Ordering::SeqCst);
        let new_state = !current;
        set_click_through(window, new_state)?;
        Ok(new_state)
    } else {
        Err("Main window not found".to_string())
    }
}

#[tauri::command]
fn toggle_overlay(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            window.hide().map_err(|e| e.to_string())?;
        } else {
            window.show().map_err(|e| e.to_string())?;
            window.set_focus().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
fn get_monitors(app: AppHandle) -> Result<Vec<MonitorInfo>, String> {
    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    let list = monitors
        .into_iter()
        .map(|m| MonitorInfo {
            name: m.name().cloned(),
            width: m.size().width,
            height: m.size().height,
            x: m.position().x,
            y: m.position().y,
            scale_factor: m.scale_factor(),
        })
        .collect();
    Ok(list)
}

#[tauri::command]
fn focus_monitor(window: WebviewWindow, x: i32, y: i32, width: u32, height: u32) -> Result<(), String> {
    window.set_position(Position::Physical(PhysicalPosition { x, y })).map_err(|e| e.to_string())?;
    window.set_size(Size::Physical(PhysicalSize { width, height })).map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    setup_macos_window(&window);
    let _ = window.set_focus();
    Ok(())
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "macos")]
    let shortcut_overlay = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyD);
    #[cfg(not(target_os = "macos"))]
    let shortcut_overlay = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyD);

    #[cfg(target_os = "macos")]
    let shortcut_ghost_x = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyX);
    #[cfg(not(target_os = "macos"))]
    let shortcut_ghost_x = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyX);

    #[cfg(target_os = "macos")]
    let shortcut_ghost_g = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyG);
    #[cfg(not(target_os = "macos"))]
    let shortcut_ghost_g = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyG);

    let shortcut_overlay_clone = shortcut_overlay.clone();
    let shortcut_ghost_x_clone = shortcut_ghost_x.clone();
    let shortcut_ghost_g_clone = shortcut_ghost_g.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        if shortcut == &shortcut_overlay_clone {
                            let _ = toggle_overlay(app.clone());
                        } else if shortcut == &shortcut_ghost_x_clone || shortcut == &shortcut_ghost_g_clone {
                            let _ = toggle_ghost_mode(app.clone());
                        }
                    }
                })
                .build(),
        )
        .setup(move |app| {
            // Register global shortcuts
            let _ = app.global_shortcut().register(shortcut_overlay);
            let _ = app.global_shortcut().register(shortcut_ghost_x);
            let _ = app.global_shortcut().register(shortcut_ghost_g);

            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            if let Some(window) = app.get_webview_window("main") {
                // Determine primary monitor bounds to cover entire screen without native fullscreen mode
                let monitor = window
                    .current_monitor()
                    .ok()
                    .flatten()
                    .or_else(|| app.primary_monitor().ok().flatten());

                if let Some(m) = monitor {
                    let pos = m.position();
                    let size = m.size();
                    let _ = window.set_position(Position::Physical(*pos));
                    let _ = window.set_size(Size::Physical(*size));
                }

                #[cfg(target_os = "macos")]
                setup_macos_window(&window);

                let _ = window.show();
                let _ = window.set_focus();
            }

            let toggle_i = MenuItem::with_id(app, "toggle", "Toggle Overlay (⌘⇧D)", true, None::<&str>)?;
            let ghost_i = MenuItem::with_id(app, "ghost", "Toggle Ghost Mode (⌘⇧X / ⌘⇧G)", true, None::<&str>)?;
            let sep1 = PredefinedMenuItem::separator(app)?;
            let clear_i = MenuItem::with_id(app, "clear", "Clear Screen Markups", true, None::<&str>)?;
            let sep2 = PredefinedMenuItem::separator(app)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit PixelTrace", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&toggle_i, &ghost_i, &sep1, &clear_i, &sep2, &quit_i])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "toggle" => {
                        let _ = toggle_overlay(app.clone());
                    }
                    "ghost" => {
                        let _ = toggle_ghost_mode(app.clone());
                    }
                    "clear" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.emit("clear-canvas", ());
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            set_click_through,
            toggle_ghost_mode,
            toggle_overlay,
            get_monitors,
            focus_monitor,
            quit_app
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

