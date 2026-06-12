export const APP_NAME = "ARollCut";

export const SUPPORTED_FORMATS = {
  video: ["MP4", "MOV"],
  audio: ["WAV", "MP3", "M4A", "AAC", "FLAC"],
} as const;

export function supportedFormatsLabel(): string {
  return [...SUPPORTED_FORMATS.video, ...SUPPORTED_FORMATS.audio].join("、");
}
