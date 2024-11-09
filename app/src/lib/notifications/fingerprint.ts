/* eslint-disable @typescript-eslint/no-unused-vars */

declare global {
  interface Window {
    webkitAudioContext: typeof AudioContext;
  }
}

interface FingerprintComponents {
  userAgent: string;
  language: string;
  colorDepth: number;
  screenResolution: string;
  timezone: string;
  touchSupport: boolean;
  cookiesEnabled: boolean;
  localStorage: boolean;
  sessionStorage: boolean;
  canvasFingerprint: string;
  webglVendor?: string;
  webglRenderer?: string;
  webglError?: string;
  audioFingerprint?: number;
  audioError?: string;
}

async function hashString(str: string): Promise<string> {
  const msgBuffer = new TextEncoder().encode(str);
  const hashBuffer = await crypto.subtle.digest("SHA-256", msgBuffer);
  const hashArray = Array.from(new Uint8Array(hashBuffer));
  const hashHex = hashArray
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
  return hashHex;
}

export async function generateFingerprint(): Promise<string> {
  const components: FingerprintComponents = {} as FingerprintComponents;

  // Basic browser information
  components.userAgent = navigator.userAgent;
  components.language = navigator.language;
  components.colorDepth = window.screen.colorDepth;
  components.screenResolution = `${window.screen.width}x${window.screen.height}`;
  components.timezone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  components.touchSupport = "ontouchstart" in window;

  // Browser capabilities
  components.cookiesEnabled = navigator.cookieEnabled;
  components.localStorage = !!window.localStorage;
  components.sessionStorage = !!window.sessionStorage;

  // Canvas fingerprinting
  const canvas = document.createElement("canvas");
  const ctx = canvas.getContext("2d");
  if (!ctx) {
    throw new Error("2D context not supported");
  }
  canvas.width = 200;
  canvas.height = 200;

  // Draw some shapes and text
  ctx.textBaseline = "top";
  ctx.font = "14px Arial";
  ctx.fillStyle = "#f60";
  ctx.fillRect(125, 1, 62, 20);
  ctx.fillStyle = "#069";
  ctx.fillText("Hello, world!", 2, 15);
  ctx.fillStyle = "rgba(102, 204, 0, 0.7)";
  ctx.fillRect(2, 100, 100, 50);

  components.canvasFingerprint = canvas.toDataURL();

  // WebGL information
  try {
    const gl =
      canvas.getContext("webgl") || canvas.getContext("experimental-webgl");
    if (gl instanceof WebGLRenderingContext) {
      components.webglVendor = gl.getParameter(gl.VENDOR);
      components.webglRenderer = gl.getParameter(gl.RENDERER);
    }
  } catch (_) {
    components.webglError = "WebGL not supported";
  }

  // Audio fingerprinting
  try {
    type AudioContextType = typeof AudioContext;
    const AudioContextClass = (window.AudioContext ||
      window.webkitAudioContext) as AudioContextType;

    const audioContext = new AudioContextClass();
    const oscillator = audioContext.createOscillator();
    const analyser = audioContext.createAnalyser();
    const gainNode = audioContext.createGain();
    gainNode.gain.value = 0; // Mute the sound
    oscillator.connect(analyser);
    analyser.connect(gainNode);
    gainNode.connect(audioContext.destination);
    oscillator.start(0);
    audioContext.close();
    components.audioFingerprint = analyser.fftSize;
  } catch (_) {
    components.audioError = "Audio fingerprinting not supported";
  }

  // Generate final hash
  const fingerprintString = JSON.stringify(components);
  return await hashString(fingerprintString);
}
