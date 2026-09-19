mod activity;
mod audio;
mod commands;
mod db;
mod fn_hotkey;
mod models;
mod panel;
mod paste;
mod permissions;
mod settings;
mod state;
mod system_audio;
mod transcribe;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use state::AppState;

const TRAY_ID: &str = "mim-dictate-tray";
#[cfg(target_os = "macos")]
const DEFAULT_SHORTCUT: &str = "Option";
#[cfg(not(target_os = "macos"))]
const DEFAULT_SHORTCUT: &str = "F8";
#[cfg(target_os = "macos")]
const FALLBACK_SHORTCUTS: &[&str] = &[
    "CommandOrControl+Shift+Space",
    "F8",
    "F9",
    "Control+Shift+Space",
];
#[cfg(not(target_os = "macos"))]
const FALLBACK_SHORTCUTS: &[&str] = &[DEFAULT_SHORTCUT, "F9", "Control+Shift+Space"];

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    whisper_rs::install_logging_hooks();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let state = AppState::new()?;
            app.manage(state);

            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(|app, _shortcut, event| match event.state() {
                        ShortcutState::Pressed | ShortcutState::Released => {
                            commands::handle_shortcut(app.clone(), event.state());
                        }
                    })
                    .build(),
            )?;

            register_first_available_shortcut(app.handle());
            fn_hotkey::start(app.handle());
            build_tray(app)?;

            if let Some(window) = app.get_webview_window("main") {
                let w = window.clone();
                window.on_window_event(move |event| match event {
                    tauri::WindowEvent::Focused(false) => {
                        let _ = w.hide();
                    }
                    tauri::WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        let _ = w.hide();
                    }
                    _ => {}
                });
            }

            if should_show_panel_on_start(app) {
                panel::show(app.handle());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::start_recording,
            commands::stop_recording,
            commands::is_recording,
            commands::get_history,
            commands::delete_history_item,
            commands::clear_history,
            commands::get_settings,
            commands::set_model,
            commands::set_languages,
            commands::set_hotkey,
            commands::begin_hotkey_capture,
            commands::cancel_hotkey_capture,
            commands::set_activation_mode,
            commands::set_audio_cues,
            commands::set_auto_paste,
            commands::set_mute_during_recording,
            commands::get_model_status,
            commands::download_model,
            commands::copy_text,
            commands::hide_panel,
            commands::get_dictation_stats,
            commands::check_permissions,
            commands::request_mic_permission,
            commands::request_accessibility_permission,
            commands::request_keyboard_permission,
            commands::request_input_monitoring_permission,
            commands::restart_app,
            commands::quit_app,
            commands::open_permission_settings,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| match event {
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => panel::show(app),
            tauri::RunEvent::Exit => {
                system_audio::restore();
                if let Some(state) = app.try_state::<AppState>() {
                    // AppHandle clones can outlive the event loop. Explicitly free
                    // Whisper before ggml's global Metal device is destroyed.
                    state
                        .transcriber
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .shutdown();
                }
            }
            _ => {}
        });
}

fn build_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon@2x.png"))
        .or_else(|_| {
            app.default_window_icon()
                .cloned()
                .ok_or(tauri::Error::AssetNotFound("tray icon".into()))
        })?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Mim Dictate")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .icon_as_template(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => panel::show(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                panel::toggle(tray.app_handle());
            }
        });

    builder = builder.icon(icon);
    builder.build(app)?;
    Ok(())
}

fn should_show_panel_on_start(app: &tauri::App) -> bool {
    let state = app.state::<AppState>();
    let model = state
        .settings
        .lock()
        .ok()
        .map(|settings| settings.model.clone());

    let model_missing = model
        .as_deref()
        .map(|model| !models::is_installed(&state.app_dir, model).unwrap_or(false))
        .unwrap_or(false);
    let permissions = permissions::check();

    model_missing
        || permissions.microphone != "authorized"
        || !permissions.accessibility
        || !permissions.input_monitoring
}

pub(crate) fn set_tray_busy(app: &tauri::AppHandle, busy: bool) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let title = if busy { Some("REC") } else { None };
        let _ = tray.set_title(title);
        let _ = tray.set_tooltip(Some(if busy {
            "Mim Dictate is recording"
        } else {
            "Mim Dictate"
        }));
    }
}

fn register_first_available_shortcut(app: &tauri::AppHandle) {
    let preferred = app
        .try_state::<AppState>()
        .and_then(|state| {
            state
                .settings
                .lock()
                .ok()
                .map(|settings| settings.hotkey.clone())
        })
        .unwrap_or_else(|| DEFAULT_SHORTCUT.to_string());
    let preferred = normalize_shortcut(&preferred);

    if fn_hotkey::is_modifier_hotkey(&preferred) {
        if let Some(state) = app.try_state::<AppState>() {
            let _ = state.update_hotkey(preferred.clone());
        }
        return;
    }

    for candidate in shortcut_candidates(&preferred) {
        if app.global_shortcut().register(candidate.as_str()).is_ok() {
            if let Some(state) = app.try_state::<AppState>() {
                let _ = state.update_hotkey(candidate);
            }
            break;
        }
    }
}

pub(crate) fn replace_registered_shortcut(
    app: &tauri::AppHandle,
    current: &str,
    next: &str,
) -> anyhow::Result<String> {
    let next = normalize_shortcut(next);
    let current = normalize_shortcut(current);

    if fn_hotkey::is_modifier_hotkey(&next) {
        if !fn_hotkey::is_modifier_hotkey(&current) {
            let _ = app.global_shortcut().unregister(current.as_str());
        }
        return Ok(next);
    }

    if fn_hotkey::is_modifier_hotkey(&current) {
        if !app.global_shortcut().is_registered(next.as_str()) {
            app.global_shortcut().register(next.as_str())?;
        }
        return Ok(next);
    }

    if current == next && app.global_shortcut().is_registered(current.as_str()) {
        return Ok(next);
    }

    app.global_shortcut().register(next.as_str())?;
    if current != next {
        let _ = app.global_shortcut().unregister(current.as_str());
    }

    Ok(next)
}

fn shortcut_candidates(preferred: &str) -> Vec<String> {
    let mut candidates = vec![normalize_shortcut(preferred)];
    for fallback in FALLBACK_SHORTCUTS {
        if !candidates.iter().any(|candidate| candidate == fallback) {
            candidates.push((*fallback).to_string());
        }
    }
    candidates
}

fn normalize_shortcut(shortcut: &str) -> String {
    let shortcut = shortcut.trim();
    if let Some(shortcut) = fn_hotkey::normalize_modifier_hotkey(shortcut) {
        return shortcut;
    }
    if shortcut.is_empty() {
        DEFAULT_SHORTCUT.to_string()
    } else {
        shortcut.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_candidates_try_preferred_first() {
        let candidates = shortcut_candidates("F10");
        assert_eq!(candidates.first().map(String::as_str), Some("F10"));
        assert!(candidates
            .iter()
            .any(|candidate| FALLBACK_SHORTCUTS.contains(&candidate.as_str())));
    }

    #[test]
    fn shortcut_candidates_deduplicate_default() {
        let candidates = shortcut_candidates(DEFAULT_SHORTCUT);
        let default_count = candidates
            .iter()
            .filter(|candidate| candidate.as_str() == DEFAULT_SHORTCUT)
            .count();
        assert_eq!(default_count, 1);
    }

    #[test]
    fn normalize_shortcut_uses_default_for_blank_values() {
        assert_eq!(normalize_shortcut("  "), DEFAULT_SHORTCUT);
        assert_eq!(normalize_shortcut(" F8 "), "F8");
    }

    #[test]
    fn normalize_shortcut_preserves_fn_hotkey_aliases() {
        assert_eq!(normalize_shortcut("fn"), "Fn");
        assert_eq!(normalize_shortcut("Globe"), "Fn");
        assert_eq!(normalize_shortcut("Function"), "Fn");
        assert_eq!(normalize_shortcut("option"), "Option");
        assert_eq!(normalize_shortcut("Alt"), "Option");
    }
}
