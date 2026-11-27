/**
 * Voice input for the Move prompt.
 *
 * On-device recognition through expo-speech-recognition. The words become the
 * same prompt the home screen types, and take the same path from there — the
 * audio itself is never uploaded by this app.
 *
 * The native module is absent from builds made before it was added, and from
 * Expo Go, so every call here is guarded and the mic is simply not offered.
 */

export type VoiceEvent = 'start' | 'end' | 'result' | 'error' | 'nomatch';

interface Subscription {
  remove: () => void;
}

interface SpeechModule {
  isRecognitionAvailable: () => boolean;
  requestPermissionsAsync: () => Promise<{ granted: boolean }>;
  start: (options: Record<string, unknown>) => void;
  stop: () => void;
  addListener: (event: string, listener: (payload: any) => void) => Subscription;
}

let cached: SpeechModule | null | undefined;

function speechModule(): SpeechModule | null {
  if (cached !== undefined) return cached;
  try {
    // Required lazily: importing it at module scope throws where the native
    // module is not linked, which would take the whole home screen with it.
    cached = require('expo-speech-recognition').ExpoSpeechRecognitionModule as SpeechModule;
  } catch {
    cached = null;
  }
  return cached;
}

/** True when this build and this phone can actually listen. */
export function speechAvailable(): boolean {
  const mod = speechModule();
  if (!mod) return false;
  try {
    return mod.isRecognitionAvailable();
  } catch {
    return false;
  }
}

export async function requestVoicePermission(): Promise<boolean> {
  const mod = speechModule();
  if (!mod) return false;
  try {
    const result = await mod.requestPermissionsAsync();
    return !!result?.granted;
  } catch {
    return false;
  }
}

export function startListening(lang = 'en-US'): void {
  try {
    speechModule()?.start({ lang, interimResults: true, continuous: false });
  } catch {
    // The screen already shows the error event; a throw here would be a crash.
  }
}

export function stopListening(): void {
  try {
    speechModule()?.stop();
  } catch {
    // Stopping something that never started is not an error worth showing.
  }
}

export function addVoiceListener(event: VoiceEvent, listener: (payload: any) => void): Subscription | null {
  try {
    return speechModule()?.addListener(event, listener) ?? null;
  } catch {
    return null;
  }
}

/** Interim results arrive repeatedly; an empty one must not wipe what was heard. */
export function mergeTranscript(previous: string, next: string | undefined | null): string {
  const text = (next ?? '').trim();
  return text.length > 0 ? text : previous;
}

/**
 * What to tell the customer. Every one of these ends with the way out: typing
 * it. An aborted session is their own tap, so it says nothing.
 */
const MESSAGES: Record<string, string> = {
  aborted: '',
  'not-allowed': 'Microphone access is off. Turn it on in Settings, or type it instead.',
  'service-not-allowed': 'This phone has no speech service available. Type it instead.',
  'language-not-supported': 'Speech is not available in this language on this phone. Type it instead.',
  'no-speech': "I didn't catch anything. Tap the circle and say it again.",
  'audio-capture': "I couldn't reach the microphone. Type it instead.",
  network: 'Speech needs a connection right now. Type it instead.',
};

export function voiceErrorMessage(code?: string | null, fallback?: string | null): string {
  if (code && code in MESSAGES) return MESSAGES[code];
  const detail = (fallback ?? '').trim();
  return detail.length > 0 ? detail : "Speech didn't work just then. Tap the circle to try again, or type it.";
}
