export type NotePreview = {
  fullText: string;
  previewText: string;
  lineLabel: string;
  isMultiline: boolean;
};

export function formatNotePreview(
  note: string | null | undefined,
): NotePreview | null {
  const fullText = note?.trim();
  if (!fullText) return null;

  const rawLines = fullText.split(/\r?\n/);
  const contentLines = rawLines
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
  const previewText = (contentLines.length ? contentLines : [fullText])
    .join(" / ")
    .replace(/[ \t]+/g, " ");
  const lineCount = Math.max(contentLines.length, 1);

  return {
    fullText,
    previewText,
    lineLabel: `${lineCount} ${lineCount === 1 ? "line" : "lines"}`,
    isMultiline: rawLines.length > 1,
  };
}
