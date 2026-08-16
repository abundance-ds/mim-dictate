use std::{path::Path, time::Instant};

use serde::Serialize;
use whisper_rs::{
    get_lang_str, FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters,
};

use crate::models;

#[derive(Default)]
pub struct Transcriber {
    loaded_model: Option<String>,
    context: Option<WhisperContext>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TranscriptResult {
    pub text: String,
    pub language: String,
    pub duration_ms: u128,
    pub model: String,
}

impl Transcriber {
    pub fn transcribe(
        &mut self,
        app_dir: &Path,
        model: &str,
        languages: &[String],
        samples: &[f32],
    ) -> anyhow::Result<TranscriptResult> {
        self.ensure_model(app_dir, model)?;

        let started = Instant::now();
        let allowed_languages = normalize_languages(languages);
        let forced = if allowed_languages.len() == 1 {
            Some(allowed_languages[0].as_str())
        } else {
            None
        };

        let context = self
            .context
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Model context is not loaded"))?;
        let mut result = run_once(context, samples, forced)?;

        if allowed_languages.len() > 1
            && !result.language.is_empty()
            && !allowed_languages
                .iter()
                .any(|lang| lang == &result.language)
        {
            result = run_once(context, samples, Some(&allowed_languages[0]))?;
            result.language = allowed_languages[0].clone();
        }

        Ok(TranscriptResult {
            text: result.text,
            language: if result.language.is_empty() {
                forced.unwrap_or("auto").to_string()
            } else {
                result.language
            },
            duration_ms: started.elapsed().as_millis(),
            model: model.to_string(),
        })
    }

    fn ensure_model(&mut self, app_dir: &Path, model: &str) -> anyhow::Result<()> {
        if self.loaded_model.as_deref() == Some(model) && self.context.is_some() {
            return Ok(());
        }

        let path = models::model_path(app_dir, model)?;
        if !path.exists() {
            return Err(anyhow::anyhow!(
                "Model '{model}' is not downloaded. Download it from the main panel first."
            ));
        }

        let path = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Model path is not valid UTF-8"))?;
        let context = WhisperContext::new_with_params(path, WhisperContextParameters::default())?;
        self.context = Some(context);
        self.loaded_model = Some(model.to_string());
        Ok(())
    }
}

struct RunResult {
    text: String,
    language: String,
}

fn run_once(
    context: &WhisperContext,
    samples: &[f32],
    language: Option<&str>,
) -> anyhow::Result<RunResult> {
    let mut state = context.create_state()?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

    let thread_count = std::thread::available_parallelism()
        .map(|count| count.get().min(4) as i32)
        .unwrap_or(4);
    params.set_n_threads(thread_count);
    params.set_language(language);
    params.set_no_timestamps(true);
    params.set_single_segment(true);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);

    state.full(params, samples)?;
    let text = state
        .as_iter()
        .map(|segment| segment.to_string())
        .collect::<Vec<_>>()
        .join("")
        .trim()
        .to_string();
    let language = get_lang_str(state.full_lang_id_from_state())
        .unwrap_or("")
        .to_string();

    Ok(RunResult { text, language })
}

fn normalize_languages(languages: &[String]) -> Vec<String> {
    let mut out = languages
        .iter()
        .filter(|lang| lang.as_str() != "auto")
        .cloned()
        .collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}
