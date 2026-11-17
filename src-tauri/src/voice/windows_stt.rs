//! Push-to-talk dictation with Windows.Media.SpeechRecognition.
//! The recognizer lives on a dedicated thread; start/stop are signalled
//! through channels so WinRT objects never cross threads.

use crate::error::{AppError, AppResult};
use crate::permissions::Notifier;
use std::sync::{mpsc, Arc, Mutex};
use tokio::sync::oneshot;
use windows::core::HSTRING;
use windows::Foundation::TypedEventHandler;
use windows::Globalization::Language;
use windows::Media::SpeechRecognition::*;

enum Signal {
    Stop,
    Cancel,
}

pub struct Session {
    signal: mpsc::Sender<Signal>,
    done: oneshot::Receiver<AppResult<String>>,
}

fn map_err(e: windows::core::Error) -> AppError {
    // SPERR_SPEECH_PRIVACY_POLICY_NOT_ACCEPTED
    if e.code().0 as u32 == 0x8004_5509 {
        return AppError::Voice(
            "Windows speech recognition is blocked by privacy settings. Turn on Settings → Privacy & security → Speech → Online speech recognition, or switch the speech provider in Windows AI settings.".into(),
        );
    }
    AppError::Voice(format!("speech recognition: {} (0x{:08X})", e.message(), e.code().0 as u32))
}

impl Session {
    pub async fn start(language: String, notifier: Arc<dyn Notifier>) -> AppResult<Self> {
        let (sig_tx, sig_rx) = mpsc::channel::<Signal>();
        let (ready_tx, ready_rx) = oneshot::channel::<AppResult<()>>();
        let (done_tx, done_rx) = oneshot::channel::<AppResult<String>>();
        std::thread::Builder::new()
            .name("windows-stt".into())
            .spawn(move || {
                let text = Arc::new(Mutex::new(String::new()));
                let setup = (|| -> windows::core::Result<(SpeechRecognizer, SpeechContinuousRecognitionSession)> {
                    let recognizer = match Language::CreateLanguage(&HSTRING::from(language.as_str())) {
                        Ok(lang) => SpeechRecognizer::Create(&lang).or_else(|_| SpeechRecognizer::new())?,
                        Err(_) => SpeechRecognizer::new()?,
                    };
                    let constraint = SpeechRecognitionTopicConstraint::Create(SpeechRecognitionScenario::Dictation, &HSTRING::from("dictation"))?;
                    recognizer.Constraints()?.Append(&constraint)?;
                    let compiled = recognizer.CompileConstraintsAsync()?.get()?;
                    if compiled.Status()? != SpeechRecognitionResultStatus::Success {
                        return Err(windows::core::Error::new(
                            windows::core::HRESULT(0x8004_5509u32 as i32),
                            format!("constraint compilation failed: {:?}", compiled.Status()?),
                        ));
                    }
                    let n2 = notifier.clone();
                    recognizer.HypothesisGenerated(&TypedEventHandler::new(move |_, args: windows::core::Ref<SpeechRecognitionHypothesisGeneratedEventArgs>| {
                        if let Some(a) = args.as_ref() {
                            let h = a.Hypothesis()?.Text()?.to_string_lossy();
                            n2.emit("voice-partial", serde_json::json!({ "text": h }));
                        }
                        Ok(())
                    }))?;
                    let session = recognizer.ContinuousRecognitionSession()?;
                    let t2 = text.clone();
                    let n3 = notifier.clone();
                    session.ResultGenerated(&TypedEventHandler::new(
                        move |_, args: windows::core::Ref<SpeechContinuousRecognitionResultGeneratedEventArgs>| {
                            if let Some(a) = args.as_ref() {
                                let r = a.Result()?;
                                if r.Status()? == SpeechRecognitionResultStatus::Success {
                                    let s = r.Text()?.to_string_lossy();
                                    let mut t = t2.lock().unwrap();
                                    if !t.is_empty() && !s.is_empty() {
                                        t.push(' ');
                                    }
                                    t.push_str(&s);
                                    n3.emit("voice-partial", serde_json::json!({ "text": t.clone(), "final": true }));
                                }
                            }
                            Ok(())
                        },
                    ))?;
                    session.StartAsync()?.get()?;
                    Ok((recognizer, session))
                })();
                let (recognizer, session) = match setup {
                    Ok(v) => {
                        let _ = ready_tx.send(Ok(()));
                        v
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(map_err(e)));
                        return;
                    }
                };
                let signal = sig_rx.recv().unwrap_or(Signal::Cancel);
                let res = match signal {
                    Signal::Stop => session.StopAsync().and_then(|op| op.get()).map_err(map_err).map(|_| text.lock().unwrap().trim().to_string()),
                    Signal::Cancel => {
                        let _ = session.CancelAsync().and_then(|op| op.get());
                        Err(AppError::Cancelled)
                    }
                };
                drop(recognizer);
                let _ = done_tx.send(res);
            })
            .map_err(|e| AppError::Voice(e.to_string()))?;
        ready_rx.await.map_err(|_| AppError::Voice("speech thread failed to start".into()))??;
        Ok(Self { signal: sig_tx, done: done_rx })
    }

    pub async fn stop(self) -> AppResult<String> {
        let _ = self.signal.send(Signal::Stop);
        self.done.await.map_err(|_| AppError::Voice("speech thread ended unexpectedly".into()))?
    }

    pub fn cancel(self) {
        let _ = self.signal.send(Signal::Cancel);
    }
}
