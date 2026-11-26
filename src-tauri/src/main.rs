#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use bside::web::{start_embedded_server, EmbeddedServerHandle, UiConfig};
use tauri::{
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    webview::NewWindowResponse,
    window::Color,
    AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use url::Url;

struct AppState {
    server: Arc<Mutex<Option<EmbeddedServerHandle>>>,
    quitting: AtomicBool,
    has_tray: AtomicBool,
}

fn is_external_http_url(url: &Url) -> bool {
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    !matches!(url.host_str(), Some("127.0.0.1") | Some("localhost"))
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn shutdown_server(app: &AppHandle) {
    let state = app.state::<AppState>();
    let handle = state.server.lock().ok().and_then(|mut guard| guard.take());
    if let Some(handle) = handle {
        tauri::async_runtime::block_on(handle.shutdown());
    }
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let autostart_enabled = app.autolaunch().is_enabled().unwrap_or(false);
    let open_item = MenuItem::with_id(app, "open", "Open bside", true, None::<&str>)?;
    let autostart_item = CheckMenuItem::with_id(
        app,
        "autostart",
        "Launch at Login",
        true,
        autostart_enabled,
        None::<&str>,
    )?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit bside", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&open_item, &autostart_item, &separator, &quit_item])?;
    let tray_icon = Image::from_bytes(include_bytes!("../icons/tray.png"))?;
    let autostart_item_for_menu = autostart_item.clone();

    TrayIconBuilder::with_id("bside-tray")
        .icon(tray_icon)
        .tooltip("bside")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open" => show_main_window(app),
            "autostart" => {
                let autolaunch = app.autolaunch();
                let currently_enabled = autolaunch.is_enabled().unwrap_or(false);
                let _ = if currently_enabled {
                    autolaunch.disable()
                } else {
                    autolaunch.enable()
                };
                let updated = autolaunch.is_enabled().unwrap_or(!currently_enabled);
                let _ = autostart_item_for_menu.set_checked(updated);
            }
            "quit" => {
                let state = app.state::<AppState>();
                state.quitting.store(true, Ordering::SeqCst);
                shutdown_server(app);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

fn main() {
    #[cfg(target_os = "linux")]
    {
        // Avoid WebKitGTK DMA-BUF / EGL crashes on Arch Linux, Wayland, and NVIDIA/Mesa setups
        // unless the user explicitly configured this variable.
        if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                if !state.quitting.load(Ordering::SeqCst)
                    && state.has_tray.load(Ordering::SeqCst)
                {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            let config = UiConfig::desktop_default();
            let server = tauri::async_runtime::block_on(start_embedded_server(config))
                .map_err(|err| std::io::Error::other(err.to_string()))?;
            let url = Url::parse(&format!("http://127.0.0.1:{}", server.addr.port()))?;

            app.manage(AppState {
                server: Arc::new(Mutex::new(Some(server))),
                quitting: AtomicBool::new(false),
                has_tray: AtomicBool::new(false),
            });

            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
                .title("bside")
                .inner_size(1280.0, 840.0)
                .min_inner_size(920.0, 620.0)
                .background_color(Color(9, 9, 11, 255))
                .on_navigation(|nav_url| {
                    if is_external_http_url(nav_url) {
                        let _ = open::that(nav_url.as_str());
                        false
                    } else {
                        true
                    }
                })
                .on_new_window(|new_url, _features| {
                    if is_external_http_url(&new_url) {
                        let _ = open::that(new_url.as_str());
                        NewWindowResponse::Deny
                    } else {
                        NewWindowResponse::Allow
                    }
                })
                .build()?;

            match setup_tray(app) {
                Ok(()) => {
                    app.state::<AppState>()
                        .has_tray
                        .store(true, Ordering::SeqCst);
                }
                Err(err) => {
                    // On Linux desktops without libayatana-appindicator / StatusNotifierWatcher
                    // (common on Arch, Hyprland, Sway, GNOME), continue without a tray icon
                    // instead of aborting startup.
                    eprintln!(
                        "[warn] System tray unavailable ({err}); continuing without tray icon"
                    );
                }
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building bside desktop application");

    app.run(|app_handle, event| match event {
        RunEvent::Exit => {
            shutdown_server(app_handle);
        }
        #[cfg(target_os = "macos")]
        RunEvent::Reopen {
            has_visible_windows,
            ..
        } if !has_visible_windows => {
            show_main_window(app_handle);
        }
        _ => {}
    });
}
