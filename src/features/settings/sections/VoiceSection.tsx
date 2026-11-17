import { useEffect, useState } from "react";
import { Pause, Play, Square, Volume2 } from "lucide-react";
import type { SectionProps } from "../SettingsView";
import { Card, Field, Notice, Segmented, Toggle } from "../../../components/ui";
import { listVoices, speak, ttsAvailable, ttsControls } from "../../../lib/voice";
import { useApp } from "../../../store/app";
import { api } from "../../../lib/api";

export function VoiceSection({ draft, update }: SectionProps) {
  const v = draft.voice;
  const [voices, setVoices] = useState<SpeechSynthesisVoice[]>(listVoices());
  const [whisperKey, setWhisperKey] = useState("");
  useEffect(() => {
    if (!ttsAvailable()) return;
    const load = () => setVoices(listVoices());
    window.speechSynthesis.addEventListener("voiceschanged", load);
    load();
    return () => window.speechSynthesis.removeEventListener("voiceschanged", load);
  }, []);
  return (
    <>
      <h2>Voice</h2>
      <p className="desc">Push-to-talk speech input and spoken replies. Hold the mic button or F8 to talk.</p>
      <Card title="Speech to text">
        <Field label="Provider">
          <Segmented value={v.stt_provider} onChange={(x) => update((d) => { d.voice.stt_provider = x; })}
            options={[{ value: "windows", label: "Windows" }, { value: "openai_whisper", label: "Whisper API" }, { value: "none", label: "Off" }]} />
        </Field>
        {v.stt_provider === "windows" && (
          <Notice kind="info">Uses Windows speech recognition. Requires a microphone and <b>Settings → Privacy & security → Speech → Online speech recognition</b> to be on (Windows’ own dictation service).</Notice>
        )}
        {v.stt_provider === "openai_whisper" && (
          <>
            <Notice kind="warn">Audio is uploaded to the transcription endpoint below and billed to that account. An API key is required.</Notice>
            <Field label="Endpoint"><input className="input" value={v.whisper_base_url} onChange={(e) => update((d) => { d.voice.whisper_base_url = e.target.value; })} /></Field>
            <Field label="API key" hint="Stored in Windows Credential Manager.">
              <input className="input" type="password" value={whisperKey} onChange={(e) => setWhisperKey(e.target.value)} placeholder="sk-…" />
              <button className="btn" disabled={!whisperKey} onClick={() => api.setSecret("openai_whisper_api_key", whisperKey).then(() => { setWhisperKey(""); useApp.getState().toast("success", "Key saved"); })}>Save</button>
            </Field>
          </>
        )}
        <Field label="Language" hint="BCP-47 tag, e.g. en-US, de-DE.">
          <input className="input" value={v.stt_language} onChange={(e) => update((d) => { d.voice.stt_language = e.target.value; })} style={{ width: 110 }} />
        </Field>
        <Field label="Send transcript automatically"><Toggle on={v.auto_send_transcript} onChange={(x) => update((d) => { d.voice.auto_send_transcript = x; })} /></Field>
      </Card>
      <Card title="Text to speech">
        <Field label="Read replies aloud" hint="Uses voices installed in Windows (local)."><Toggle on={v.tts_enabled} onChange={(x) => update((d) => { d.voice.tts_enabled = x; })} /></Field>
        <Field label="Voice">
          <select className="select" value={v.tts_voice} onChange={(e) => update((d) => { d.voice.tts_voice = e.target.value; })}>
            <option value="">System default</option>
            {voices.map((x) => <option key={x.name} value={x.name}>{x.name}</option>)}
          </select>
        </Field>
        <Field label="Rate" hint={`${v.tts_rate.toFixed(1)}×`}><input type="range" min={0.5} max={2} step={0.1} value={v.tts_rate} onChange={(e) => update((d) => { d.voice.tts_rate = Number(e.target.value); })} /></Field>
        <Field label="Pitch" hint={v.tts_pitch.toFixed(1)}><input type="range" min={0.5} max={2} step={0.1} value={v.tts_pitch} onChange={(e) => update((d) => { d.voice.tts_pitch = Number(e.target.value); })} /></Field>
        <Field label="Volume" hint={`${Math.round(v.tts_volume * 100)}%`}><input type="range" min={0} max={1} step={0.05} value={v.tts_volume} onChange={(e) => update((d) => { d.voice.tts_volume = Number(e.target.value); })} /></Field>
        <Field label="Playback">
          <button className="btn sm" onClick={() => speak("Hello! This is how Windows AI sounds.", { voice: v.tts_voice, rate: v.tts_rate, pitch: v.tts_pitch, volume: v.tts_volume })}><Volume2 size={13} /> Test</button>
          <button className="icon-btn" title="Pause" onClick={ttsControls.pause}><Pause size={14} /></button>
          <button className="icon-btn" title="Resume" onClick={ttsControls.resume}><Play size={14} /></button>
          <button className="icon-btn" title="Stop" onClick={ttsControls.stop}><Square size={14} /></button>
        </Field>
      </Card>
    </>
  );
}
