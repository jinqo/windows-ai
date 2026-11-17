import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, errorMessage } from "../lib/api";
import { startRecording } from "../lib/voice";
import { useApp } from "../store/app";

type Recorder = Awaited<ReturnType<typeof startRecording>>;

/**
 * Push-to-talk speech-to-text. Provider "windows" uses native
 * Windows.Media.SpeechRecognition in the backend; "openai_whisper" records
 * in the webview and uploads for transcription (requires an API key).
 */
export function useVoice(onText: (text: string) => void) {
  const [listening, setListening] = useState(false);
  const [busy, setBusy] = useState(false);
  const [partial, setPartial] = useState("");
  const recorder = useRef<Recorder | null>(null);
  const provider = useApp((s) => s.config?.voice.stt_provider ?? "none");

  useEffect(() => {
    const un = listen<{ text: string }>("voice-partial", (e) => setPartial(e.payload.text));
    return () => {
      un.then((f) => f());
    };
  }, []);

  const start = useCallback(async () => {
    if (listening || busy) return;
    setPartial("");
    try {
      if (provider === "windows") await api.voiceStart();
      else if (provider === "openai_whisper") recorder.current = await startRecording();
      else throw new Error("Speech input is turned off. Choose a speech provider in Settings → Voice.");
      setListening(true);
    } catch (e) {
      useApp.getState().toast("error", errorMessage(e));
    }
  }, [listening, busy, provider]);

  const stop = useCallback(async () => {
    if (!listening) return;
    setListening(false);
    setBusy(true);
    try {
      let text = "";
      if (provider === "windows") text = await api.voiceStop();
      else if (recorder.current) {
        const { base64, mime } = await recorder.current.stop();
        recorder.current = null;
        text = await api.voiceTranscribe(base64, mime);
      }
      if (text.trim()) onText(text.trim());
      else useApp.getState().toast("info", "No speech recognized");
    } catch (e) {
      useApp.getState().toast("error", errorMessage(e));
    } finally {
      setBusy(false);
      setPartial("");
    }
  }, [listening, provider, onText]);

  const cancel = useCallback(() => {
    if (provider === "windows") api.voiceCancel().catch(() => {});
    recorder.current?.cancel();
    recorder.current = null;
    setListening(false);
    setPartial("");
  }, [provider]);

  return { listening, busy, partial, start, stop, cancel, enabled: provider !== "none" };
}
