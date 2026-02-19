export type TimeReportMode = "review" | "develop";

export function buildWorkItemTimeReportText({
  workItemId,
  title,
  parentWorkItemId,
  mode,
}: {
  workItemId: string | number;
  title: string;
  parentWorkItemId?: string | number | null;
  mode: TimeReportMode;
}) {
  const parentPrefix =
    parentWorkItemId !== null && parentWorkItemId !== undefined
      ? `#${parentWorkItemId} `
      : "";
  const reviewPrefix = mode === "review" ? "[CR] " : "";

  return `${parentPrefix}#${workItemId} - ${reviewPrefix}${title}`;
}
