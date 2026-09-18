use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::ShortcutState;

use crate::{
    audio::RecordingStats,
    db::{DictationStats, Transcription},
    models::{self, ModelStatus},
    paste::{self, PasteOutcome},
    settings::{is_supported_language, Settings},
    state::AppState,
    transcribe::TranscriptResult,
};

#[derive(Debug, Clone, Serialize)]
pub struct StopResult {
    pub transcript: Option<TranscriptResult>,
    pub paste: Option<PasteOutcome>,
    pub stats: RecordingStats,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
struct StatusEvent {
    state: String,
    message: String,
}

type CommandResult<T> = Result<T, String>;

#[tauri::command]
pub async fn start_recording(app: AppHandle) -> CommandResult<()> {
    start_recording_impl(app).await
}

#[tauri::command]
pub async fn stop_recording(app: AppHandle) -> CommandResult<StopResult> {
    stop_recording_impl(app).await
}

#[tauri::command]
pub fn is_recording(state: State<'_, AppState>) -> CommandResult<bool> {
    let recorder = state
        .recorder
        .lock()
        .map_err(|_| "audio lock poisoned".to_string())?;
    Ok(recorder.is_recording())
}

#[tauri::command]
pub fn get_history(state: State<'_, AppState>) -> CommandResult<Vec<Transcription>> {
    state.db.list(100).map_err(to_command_error)
}

#[tauri::command]
pub fn delete_history_item(state: State<'_, AppState>, id: i64) -> CommandResult<()> {
    state.db.delete(id).map_err(to_command_error)
}

#[tauri::command]
pub fn clear_history(state: State<'_, AppState>) -> CommandResult<()> {
    state.db.clear().map_err(to_command_error)
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> CommandResult<Settings> {
    Ok(state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned".to_string())?
        .clone())
}

#[tauri::command]
pub fn set_model(
    app: AppHandle,
    state: State<'_, AppState>,
    model: String,
) -> CommandResult<Settings> {
    if !models::is_supported(&model) {
        return Err("Unknown model".to_string());
    }
    {
        let mut settings = state
            .settings
            .lock()
            .map_err(|_| "settings lock poisoned".to_string())?;
        settings.model = model;
    }
    let settings = state.save_settings().map_err(to_command_error)?;
    emit_settings(&app, &settings);
    Ok(settings)
}

#[tauri::command]
pub fn set_languages(
    app: AppHandle,
    state: State<'_, AppState>,
    langs: Vec<String>,
) -> CommandResult<Settings> {
    {
        let mut settings = state
            .settings
            .lock()
            .map_err(|_| "settings lock poisoned".to_string())?;
        settings.languages = langs
            .into_iter()
            .filter(|lang| is_supported_language(lang))
            .collect();
        settings.normalize();
    }
    let settings = state.save_settings().map_err(to_command_error)?;
    emit_settings(&app, &settings);
    Ok(settings)
}

#[tauri::command]
pub fn set_hotkey(
    app: AppHandle,
    state: State<'_, AppState>,
    hotkey: String,
) -> CommandResult<Settings> {
    let current_hotkey = state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned".to_string())?
        .hotkey
        .clone();
    let registered_hotkey = crate::replace_registered_shortcut(&app, &current_hotkey, &hotkey)
        .map_err(to_command_error)?;

    {
        let mut settings = state
            .settings
            .lock()
            .map_err(|_| "settings lock poisoned".to_string())?;
        settings.hotkey = registered_hotkey;
        settings.normalize();
    }
    let settings = state.save_settings().map_err(to_command_error)?;
    emit_settings(&app, &settings);
    Ok(settings)
}

#[tauri::command]
pub fn begin_hotkey_capture() -> CommandResult<()> {
    crate::fn_hotkey::begin_capture();
    Ok(())
}

#[tauri::command]
pub fn cancel_hotkey_capture() -> CommandResult<()> {
    crate::fn_hotkey::cancel_capture();
    Ok(())
}

#[tauri::command]
pub fn set_activation_mode(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: String,
) -> CommandResult<Settings> {
    if !matches!(mode.as_str(), "hold" | "toggle") {
        return Err("Activation mode must be hold or toggle".to_string());
    }
    {
        let mut settings = state
            .settings
            .lock()
            .map_err(|_| "settings lock poisoned".to_string())?;
        settings.activation_mode = mode;
    }
    let settings = state.save_settings().map_err(to_command_error)?;
    emit_settings(&app, &settings);
    Ok(settings)
}

#[tauri::command]
pub fn set_audio_cues(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> CommandResult<Settings> {
    {
        let mut settings = state
            .settings
            .lock()
            .map_err(|_| "settings lock poisoned".to_string())?;
        settings.audio_cues = enabled;
    }
    let settings = state.save_settings().map_err(to_command_error)?;
    emit_settings(&app, &settings);
    Ok(settings)
}

#[tauri::command]
pub fn set_auto_paste(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> CommandResult<Settings> {
    {
        let mut settings = state
            .settings
            .lock()
            .map_err(|_| "settings lock poisoned".to_string())?;
        settings.auto_paste = enabled;
    }
    let settings = state.save_settings().map_err(to_command_error)?;
    emit_settings(&app, &settings);
    Ok(settings)
}

#[tauri::command]
pub fn get_model_status(state: State<'_, AppState>) -> CommandResult<Vec<ModelStatus>> {
    models::statuses(&state.app_dir).map_err(to_command_error)
}

#[tauri::command]
pub async fn download_model(
    app: AppHandle,
    state: State<'_, AppState>,
    model: String,
) -> CommandResult<ModelStatus> {
    models::download(app, state.app_dir.clone(), model)
        .await
        .map_err(to_command_error)
}

#[tauri::command]
pub fn copy_text(app: AppHandle, text: String) -> CommandResult<()> {
    app.clipboard().write_text(text).map_err(to_command_error)
}

#[tauri::command]
pub fn hide_panel(app: AppHandle) -> CommandResult<()> {
    if let Some(window) = app.get_webview_window("main") {
        window.hide().map_err(to_command_error)?;
    }
    Ok(())
}

pub fn handle_shortcut(app: AppHandle, shortcut_state: ShortcutState) {
    if crate::fn_hotkey::is_capturing() {
        return;
    }

    tauri::async_runtime::spawn(async move {
        let activation_mode = app
            .state::<AppState>()
            .settings
            .lock()
            .map(|settings| settings.activation_mode.clone())
            .unwrap_or_else(|_| "hold".to_string());

        match (activation_mode.as_str(), shortcut_state) {
            ("hold", ShortcutState::Pressed) => {
                let _ = start_recording_impl(app).await;
            }
            ("hold", ShortcutState::Released) => {
                let _ = stop_recording_impl(app).await;
            }
            ("toggle", ShortcutState::Pressed) => {
                let is_recording = app
                    .state::<AppState>()
                    .recorder
                    .lock()
                    .map(|recorder| recorder.is_recording())
                    .unwrap_or(false);
                if is_recording {
                    let _ = stop_recording_impl(app).await;
                } else {
                    let _ = start_recording_impl(app).await;
                }
            }
            _ => {}
        }
    });
}

async fn start_recording_impl(app: AppHandle) -> CommandResult<()> {
    let state = app.state::<AppState>();
    let model = state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned".to_string())?
        .model
        .clone();
    if !models::is_installed(&state.app_dir, &model).map_err(to_command_error)? {
        return Err(format!("Download the {model} model before recording."));
    }

    let mute = state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned".to_string())?
        .mute_during_recording;
    crate::system_audio::mute_if_needed(mute);

    {
        let mut recorder = state
            .recorder
            .lock()
            .map_err(|_| "audio lock poisoned".to_string())?;
        recorder.start_recording().map_err(to_command_error)?;
    }
    crate::set_tray_busy(&app, true);
    emit_status(&app, "recording", "Recording");
    Ok(())
}

async fn stop_recording_impl(app: AppHandle) -> CommandResult<StopResult> {
    let state = app.state::<AppState>();
    let mut tray_recording = RecordingTrayGuard::new(app.clone());
    crate::system_audio::restore();
    let paste_target_pid = paste::frontmost_app_pid();

    let capture = {
        let mut recorder = state
            .recorder
            .lock()
            .map_err(|_| "audio lock poisoned".to_string())?;
        if !recorder.is_recording() {
            return Err("Recording is not active".to_string());
        }
        recorder.stop_recording().map_err(to_command_error)?
    };
    tray_recording.clear();

    if !capture.stats.has_speech {
        emit_status(&app, "idle", "No speech detected");
        return Ok(StopResult {
            transcript: None,
            paste: None,
            stats: capture.stats,
            message: "No speech detected".to_string(),
        });
    }

    emit_status(&app, "transcribing", "Transcribing");
    let settings = state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned".to_string())?
        .clone();
    let transcriber = state.transcriber.clone();
    let app_dir = state.app_dir.clone();
    let samples = capture.samples;
    let model = settings.model.clone();
    let languages = settings.languages.clone();

    let transcript = tauri::async_runtime::spawn_blocking(move || {
        let _activity = crate::activity::ActivityGuard::begin("Transcribing dictation");
        let mut transcriber = transcriber
            .lock()
            .map_err(|_| anyhow::anyhow!("transcriber lock poisoned"))?;
        transcriber.transcribe(&app_dir, &model, &languages, &samples)
    })
    .await
    .map_err(to_command_error)?
    .map_err(to_command_error)?;

    if transcript.text.trim().is_empty() {
        emit_status(&app, "idle", "No text detected");
        return Ok(StopResult {
            transcript: Some(transcript),
            paste: None,
            stats: capture.stats,
            message: "No text detected".to_string(),
        });
    }

    let paste = if settings.auto_paste {
        Some(
            paste::paste_text(&app, &transcript.text, paste_target_pid)
                .map_err(to_command_error)?,
        )
    } else {
        None
    };
    let message = paste
        .as_ref()
        .map(|outcome| outcome.message.clone())
        .unwrap_or_else(|| "Done".to_string());

    state
        .db
        .insert(
            &transcript.text,
            &transcript.language,
            capture.stats.duration_ms as i64,
            capture.stats.speech_duration_ms as i64,
            &transcript.model,
        )
        .map_err(to_command_error)?;

    emit_status(&app, "done", &message);
    let _ = app.emit("history-changed", ());

    Ok(StopResult {
        transcript: Some(transcript),
        paste,
        stats: capture.stats,
        message,
    })
}

struct RecordingTrayGuard {
    app: AppHandle,
    active: bool,
}

impl RecordingTrayGuard {
    fn new(app: AppHandle) -> Self {
        Self { app, active: true }
    }

    fn clear(&mut self) {
        if self.active {
            crate::set_tray_busy(&self.app, false);
            self.active = false;
        }
    }
}

impl Drop for RecordingTrayGuard {
    fn drop(&mut self) {
        self.clear();
    }
}

fn emit_status(app: &AppHandle, state: &str, message: &str) {
    let _ = app.emit(
        "recording-state",
        StatusEvent {
            state: state.to_string(),
            message: message.to_string(),
        },
    );
}

fn emit_settings(app: &AppHandle, settings: &Settings) {
    let _ = app.emit("settings-changed", settings);
}

#[tauri::command]
pub fn set_mute_during_recording(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> CommandResult<Settings> {
    {
        let mut settings = state
            .settings
            .lock()
            .map_err(|_| "settings lock poisoned".to_string())?;
        settings.mute_during_recording = enabled;
    }
    let settings = state.save_settings().map_err(to_command_error)?;
    emit_settings(&app, &settings);
    Ok(settings)
}

#[tauri::command]
pub fn get_dictation_stats(state: State<'_, AppState>) -> CommandResult<DictationStats> {
    state.db.stats_30d().map_err(to_command_error)
}

#[tauri::command]
pub fn check_permissions() -> CommandResult<crate::permissions::PermissionStatus> {
    Ok(crate::permissions::check())
}

#[tauri::command]
pub fn request_mic_permission() -> CommandResult<()> {
    crate::permissions::request_mic();
    Ok(())
}

#[tauri::command]
pub fn request_accessibility_permission() -> CommandResult<crate::permissions::PermissionStatus> {
    Ok(crate::permissions::request_accessibility())
}

#[tauri::command]
pub fn request_keyboard_permission() -> CommandResult<crate::permissions::PermissionStatus> {
    Ok(crate::permissions::request_keyboard())
}

#[tauri::command]
pub fn request_input_monitoring_permission() -> CommandResult<crate::permissions::PermissionStatus>
{
    Ok(crate::permissions::request_input_monitoring())
}

#[tauri::command]
pub fn restart_app(app: AppHandle) {
    app.request_restart();
}

#[tauri::command]
pub fn open_permission_settings(pane: String) -> CommandResult<()> {
    crate::permissions::open_settings(&pane);
    Ok(())
}

fn to_command_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
