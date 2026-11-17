// Voice helpers: text-to-speech via the Web Speech API (local Windows
// voices) and audio recording for the Whisper speech-to-text provider.

export interface TtsOptions { voice?: string; rate?: number; pitch?: number; volume?: number }

export function ttsAvailable(): boolean {
  return typeof window !== "undefined" && "speechSynthesis" in window;
}

export function listVoices(): SpeechSynthesisVoice[] {
  return ttsAvailable() ? window.speechSynthesis.getVoices() : [];
}

/** Strips Markdown so code fences and symbols are not read aloud. */
export function speakableText(md: string): string {
  return md
    .replace(/```[\s\S]*?```/g, " (code omitted) ")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/!\[[^\]]*\]\([^)]*\)/g, "")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/^#{1,6}\s+/gm, "")
    .replace(/[*_~>|]/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

export function speak(text: string, o: TtsOptions = {}) {
  if (!ttsAvailable()) return;
  const synth = window.speechSynthesis;
  synth.cancel();
  const u = new SpeechSynthesisUtterance(speakableText(text));
  const v = listVoices().find((x) => x.name === o.voice);
  if (v) u.voice = v;
  u.rate = o.rate ?? 1;
  u.pitch = o.pitch ?? 1;
  u.volume = o.volume ?? 1;
  synth.speak(u);
}

export const ttsControls = {
  pause: () => ttsAvailable() && window.speechSynthesis.pause(),
  resume: () => ttsAvailable() && window.speechSynthesis.resume(),
  stop: () => ttsAvailable() && window.speechSynthesis.cancel(),
  speaking: () => ttsAvailable() && window.speechSynthesis.speaking,
};

/** Records microphone audio until `stop()` is called. */
export async function startRecording(): Promise<{ stop: () => Promise<{ base64: string; mime: string }>; cancel: () => void }> {
  if (!navigator.mediaDevices?.getUserMedia) throw new Error("Microphone access is not available");
  let stream: MediaStream;
  try {
    stream = await navigator.mediaDevices.getUserMedia({ audio: true });
  } catch (e) {
    throw new Error("Microphone permission was denied. Allow microphone access for desktop apps in Windows Settings → Privacy & security → Microphone.");
  }
  const mime = MediaRecorder.isTypeSupported("audio/webm;codecs=opus") ? "audio/webm;codecs=opus" : "audio/webm";
  const rec = new MediaRecorder(stream, { mimeType: mime });
  const chunks: Blob[] = [];
  rec.ondataavailable = (e) => e.data.size && chunks.push(e.data);
  rec.start(250);
  const release = () => stream.getTracks().forEach((t) => t.stop());
  return {
    cancel: () => {
      rec.stop();
      release();
    },
    stop: () =>
      new Promise((resolve) => {
        rec.onstop = async () => {
          release();
          const blob = new Blob(chunks, { type: mime });
          const buf = new Uint8Array(await blob.arrayBuffer());
          let bin = "";
          for (let i = 0; i < buf.length; i += 0x8000) bin += String.fromCharCode(...buf.subarray(i, i + 0x8000));
          resolve({ base64: btoa(bin), mime: mime.split(";")[0] });
        };
        rec.stop();
      }),
  };
}
