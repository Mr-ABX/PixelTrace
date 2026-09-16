use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Position, Size, WebviewWindow,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug)]
pub struct InteractiveRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

static IS_GHOST_MODE: AtomicBool = AtomicBool::new(false);
static INTERACTIVE_RECTS: Mutex<Vec<InteractiveRect>> = Mutex::new(Vec::new());

static CURRENTLY_IGNORING: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[cfg(target_os = "macos")]
type CGEventRef = *mut std::ffi::c_void;
#[cfg(target_os = "macos")]
type CGEventSourceRef = *mut std::ffi::c_void;

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventCreate(source: CGEventSourceRef) -> CGEventRef;
    fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
    fn CFRelease(cf: *mut std::ffi::c_void);
}

#[cfg(target_os = "macos")]
fn get_global_cursor_pos() -> Option<(f64, f64)> {
    unsafe {
        let event = CGEventCreate(std::ptr::null_mut());
        if event.is_null() {
            return None;
        }
        let pt = CGEventGetLocation(event);
        CFRelease(event as *mut std::ffi::c_void);
        Some((pt.x, pt.y))
    }
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct POINT {
    x: i32,
    y: i32,
}

#[cfg(target_os = "windows")]
extern "system" {
    fn GetCursorPos(lpPoint: *mut POINT) -> i32;
}

#[cfg(target_os = "windows")]
fn get_global_cursor_pos() -> Option<(f64, f64)> {
    let mut pt = POINT { x: 0, y: 0 };
    let success = unsafe { GetCursorPos(&mut pt) };
    if success != 0 {
        Some((pt.x as f64, pt.y as f64))
    } else {
        None
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn get_global_cursor_pos() -> Option<(f64, f64)> {
    None
}

#[cfg(target_os = "macos")]
fn setup_macos_dock_and_process() {
    use objc2::msg_send;
    use objc2::runtime::AnyClass;

    const ICON_BYTES: &[u8] = include_bytes!("../icons/icon.png");

    unsafe {
        // Explicitly set process name to "PixelTrace"
        if let (Some(ns_process_info_cls), Some(ns_string_cls)) = (
            AnyClass::get(c"NSProcessInfo"),
            AnyClass::get(c"NSString"),
        ) {
            let process_info: *mut objc2::runtime::AnyObject = msg_send![ns_process_info_cls, processInfo];
            if !process_info.is_null() {
                let name_str: *mut objc2::runtime::AnyObject = msg_send![
                    ns_string_cls,
                    stringWithUTF8String: b"PixelTrace\0".as_ptr() as *const std::ffi::c_char
                ];
                if !name_str.is_null() {
                    let _: () = msg_send![process_info, setProcessName: name_str];
                }
            }
        }

        // Set Dock Icon image to Apple HIG centered squircle icon
        if let (Some(ns_data_cls), Some(ns_image_cls), Some(ns_app_cls)) = (
            AnyClass::get(c"NSData"),
            AnyClass::get(c"NSImage"),
            AnyClass::get(c"NSApplication"),
        ) {
            let ns_data: *mut objc2::runtime::AnyObject = msg_send![
                ns_data_cls,
                dataWithBytes: ICON_BYTES.as_ptr() as *const std::ffi::c_void,
                length: ICON_BYTES.len()
            ];
            if !ns_data.is_null() {
                let ns_image: *mut objc2::runtime::AnyObject = msg_send![ns_image_cls, alloc];
                let ns_image: *mut objc2::runtime::AnyObject = msg_send![ns_image, initWithData: ns_data];
                if !ns_image.is_null() {
                    let app: *mut objc2::runtime::AnyObject = msg_send![ns_app_cls, sharedApplication];
                    if !app.is_null() {
                        let _: () = msg_send![app, setApplicationIconImage: ns_image];
                    }
                }
            }
        }
    }
}

fn start_mouse_monitor(app_handle: AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(25));

            let is_ghost = IS_GHOST_MODE.load(Ordering::Relaxed);
            let win = match app_handle.get_webview_window("main") {
                Some(w) => w,
                None => continue,
            };

            if !is_ghost {
                if CURRENTLY_IGNORING.load(Ordering::Relaxed) {
                    CURRENTLY_IGNORING.store(false, Ordering::SeqCst);
                    let _ = win.set_ignore_cursor_events(false);
                }
                continue;
            }

            // In Ghost Mode: query global cursor location
            let (cursor_x, cursor_y) = match get_global_cursor_pos() {
                Some(pt) => pt,
                None => continue,
            };

            let win_pos = match win.outer_position() {
                Ok(p) => p,
                Err(_) => continue,
            };
            let scale_factor = win.scale_factor().unwrap_or(1.0);

            // Convert physical screen coordinates to window-relative logical CSS pixels
            let rel_x = cursor_x - (win_pos.x as f64 / scale_factor);
            let rel_y = cursor_y - (win_pos.y as f64 / scale_factor);

            let mut inside_interactive = false;
            if let Ok(rects) = INTERACTIVE_RECTS.lock() {
                for r in rects.iter() {
                    if rel_x >= (r.x - 12.0)
                        && rel_x <= (r.x + r.width + 12.0)
                        && rel_y >= (r.y - 12.0)
                        && rel_y <= (r.y + r.height + 12.0)
                    {
                        inside_interactive = true;
                        break;
                    }
                }
            }

            let should_ignore = !inside_interactive;
            let was_ignoring = CURRENTLY_IGNORING.load(Ordering::Relaxed);

            if should_ignore != was_ignoring {
                CURRENTLY_IGNORING.store(should_ignore, Ordering::SeqCst);
                let _ = win.set_ignore_cursor_events(should_ignore);
            }
        }
    });
}

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
fn set_interactive_rects(rects: Vec<InteractiveRect>) -> Result<(), String> {
    if let Ok(mut lock) = INTERACTIVE_RECTS.lock() {
        *lock = rects;
    }
    Ok(())
}

#[tauri::command]
fn set_window_interactive(window: WebviewWindow, interactive: bool) -> Result<(), String> {
    let ignore = if interactive {
        false
    } else {
        IS_GHOST_MODE.load(Ordering::Relaxed)
    };

    #[cfg(target_os = "macos")]
    CURRENTLY_IGNORING.store(ignore, Ordering::SeqCst);

    let _ = window.set_ignore_cursor_events(ignore);
    Ok(())
}

#[tauri::command]
fn set_click_through(window: WebviewWindow, ignore: bool) -> Result<(), String> {
    IS_GHOST_MODE.store(ignore, Ordering::SeqCst);

    #[cfg(target_os = "macos")]
    CURRENTLY_IGNORING.store(ignore, Ordering::SeqCst);

    let _ = window.set_ignore_cursor_events(ignore);

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
            {
                app.set_activation_policy(tauri::ActivationPolicy::Regular);
                setup_macos_dock_and_process();
            }

            start_mouse_monitor(app.handle().clone());

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
            let prefs_i = MenuItem::with_id(app, "prefs", "Preferences... (⌘,)", true, None::<&str>)?;
            let sep2 = PredefinedMenuItem::separator(app)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit PixelTrace", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&toggle_i, &ghost_i, &sep1, &clear_i, &prefs_i, &sep2, &quit_i])?;

            let _tray = TrayIconBuilder::with_id("pixeltrace-tray")
                .icon(app.default_window_icon().unwrap().clone())
                .icon_as_template(false)
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
                    "prefs" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.emit("open-preferences", ());
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
            set_interactive_rects,
            set_window_interactive,
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

