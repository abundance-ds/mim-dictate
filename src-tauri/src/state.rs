use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use crate::{audio::AudioRecorder, db::Database, settings::Settings, transcribe::Transcriber};

pub struct AppState {
    pub app_dir: PathBuf,
    pub settings_path: PathBuf,
    pub settings: Arc<Mutex<Settings>>,
    pub recorder: Arc<Mutex<AudioRecorder>>,
    pub transcriber: Arc<Mutex<Transcriber>>,
    pub db: Database,
}

impl AppState {
    pub fn new() -> anyhow::Result<Self> {
        let data_root = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        // The app was previously named Mim TTS; adopt an existing mim-tts dir
        // so users keep their models, settings, and history.
        let mut app_dir = data_root.join("mim-dictate");
        let legacy_dir = data_root.join("mim-tts");
        if !app_dir.exists() && legacy_dir.exists() && fs::rename(&legacy_dir, &app_dir).is_err() {
            app_dir = legacy_dir;
        }
        fs::create_dir_all(app_dir.join("models"))?;

        let settings_path = app_dir.join("settings.json");
        let settings = Settings::load(&settings_path)?;
        let db = Database::new(app_dir.join("history.db"))?;

        Ok(Self {
            app_dir,
            settings_path,
            settings: Arc::new(Mutex::new(settings)),
            recorder: Arc::new(Mutex::new(AudioRecorder::default())),
            transcriber: Arc::new(Mutex::new(Transcriber::default())),
            db,
        })
    }

    pub fn save_settings(&self) -> anyhow::Result<Settings> {
        let settings = self
            .settings
            .lock()
            .map_err(|_| anyhow::anyhow!("settings lock poisoned"))?
            .clone();
        settings.save(&self.settings_path)?;
        Ok(settings)
    }

    pub fn update_hotkey(&self, hotkey: String) -> anyhow::Result<()> {
        {
            let mut settings = self
                .settings
                .lock()
                .map_err(|_| anyhow::anyhow!("settings lock poisoned"))?;
            settings.hotkey = hotkey;
            settings.normalize();
        }
        self.save_settings()?;
        Ok(())
    }
}
