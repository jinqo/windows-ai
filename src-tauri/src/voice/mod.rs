//! Voice: speech-to-text providers. Text-to-speech runs in the webview
//! via the Web Speech API (`speechSynthesis`), which uses the installed
//! Windows voices locally.
//!
//! STT providers:
//! - `windows`: native Windows.Media.SpeechRecognition (push-to-talk with a
//!   continuous dictation session). Requires a microphone and, for the
//!   dictation grammar, Windows' "Online speech recognition" privacy setting.
//! - `openai_whisper`: audio recorded in the webview is uploaded to an
//!   OpenAI-compatible /audio/transcriptions endpoint. Requires an API key.

#[cfg(windows)]
mod windows_stt;

use crate::error::{AppError, AppResult};
use crate::permissions::Notifier;
use std::sync::{Arc, Mutex};

pub struct VoiceManager {
    #[cfg(windows)]
    active: Mutex<Option<windows_stt::Session>>,
    #[cfg(not(windows))]
    _p: Mutex<()>,
}

impl Default for VoiceManager {
    fn default() -> Self {
        Self {
            #[cfg(windows)]
            active: Mutex::new(None),
            #[cfg(not(windows))]
            _p: Mutex::new(()),
        }
    }
}

impl VoiceManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    #[cfg(windows)]
    pub async fn start(&self, language: &str, notifier: Arc<dyn Notifier>) -> AppResult<()> {
        if self.active.lock().unwrap().is_some() {
            return Err(AppError::Voice("already listening".into()));
        }
        let s = windows_stt::Session::start(language.to_string(), notifier).await?;
        *self.active.lock().unwrap() = Some(s);
        Ok(())
    }

    #[cfg(windows)]
    pub async fn stop(&self) -> AppResult<String> {
        let s = self.active.lock().unwrap().take().ok_or_else(|| AppError::Voice("not listening".into()))?;
        s.stop().await
    }

    #[cfg(windows)]
    pub fn cancel(&self) {
        if let Some(s) = self.active.lock().unwrap().take() {
            s.cancel();
        }
    }

    #[cfg(not(windows))]
    pub async fn start(&self, _language: &str, _n: Arc<dyn Notifier>) -> AppResult<()> {
        Err(AppError::Voice("native speech recognition is only available on Windows".into()))
    }
    #[cfg(not(windows))]
    pub async fn stop(&self) -> AppResult<String> {
        Err(AppError::Voice("not listening".into()))
    }
    #[cfg(not(windows))]
    pub fn cancel(&self) {}

    pub fn is_listening(&self) -> bool {
        #[cfg(windows)]
        {
            self.active.lock().unwrap().is_some()
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
}

/// Uploads recorded audio to an OpenAI-compatible transcription endpoint.
pub async fn transcribe_whisper(base_url: &str, audio: Vec<u8>, mime: &str, language: &str) -> AppResult<String> {
    if audio.is_empty() {
        return Err(AppError::Voice("no audio recorded".into()));
    }
    if audio.len() > 25 * 1024 * 1024 {
        return Err(AppError::Voice("recording exceeds 25 MB".into()));
    }
    let key = crate::secrets::get_secret("openai_whisper_api_key")
        .ok_or_else(|| AppError::MissingCredential("Whisper speech-to-text needs an API key (Settings → Voice).".into()))?;
    let ext = if mime.contains("ogg") { "ogg" } else if mime.contains("wav") { "wav" } else { "webm" };
    let part = reqwest::multipart::Part::bytes(audio).file_name(format!("speech.{ext}")).mime_str(mime).map_err(|e| AppError::Voice(e.to_string()))?;
    let lang = language.split('-').next().unwrap_or("en").to_string();
    let form = reqwest::multipart::Form::new().text("model", "whisper-1").text("language", lang).part("file", part);
    let resp = crate::ai::provider::http_client()
        .post(format!("{}/audio/transcriptions", base_url.trim_end_matches('/')))
        .bearer_auth(key)
        .multipart(form)
        .send()
        .await?;
    let status = resp.status();
    let v: serde_json::Value = resp.json().await.unwrap_or_default();
    if !status.is_success() {
        return Err(AppError::Voice(format!("transcription failed ({status}): {}", v["error"]["message"].as_str().unwrap_or(""))));
    }
    Ok(v["text"].as_str().unwrap_or_default().trim().to_string())
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn whisper_rejects_empty_audio() {
        let e = super::transcribe_whisper("http://127.0.0.1:9", vec![], "audio/webm", "en-US").await.unwrap_err();
        assert_eq!(e.kind(), "voice");
    }
}
