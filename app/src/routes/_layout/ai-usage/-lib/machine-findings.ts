import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import type { AiUsageMachine } from "@/lib/api/queries/ai-usage-report";
import { formatCount } from "./format";

export type Severity = "critical" | "warning" | "note";

export type Finding = Readonly<{
  key: string;
  severity: Severity;
  text: string;
}>;

/**
 * What may be missing or unknown in a machine's figures, last of all its
 * usage in the selected dates that has no known price. A provider without
 * history is only worth a note when the machine uploaded its usage before;
 * otherwise the tool is simply not used there.
 */
export function machineFindings(
  machine: AiUsageMachine,
  staleAfterDays: number,
): Finding[] {
  const findings: Finding[] = [];
  if (machine.stale) {
    findings.push({
      key: "stale",
      severity: "warning",
      text: `Stale: no sync for more than ${staleAfterDays} days. Usage since then is missing, not zero.`,
    });
  }
  for (const entry of machine.coverage) {
    const tool = AI_PROVIDER_LABELS[entry.provider];
    switch (entry.status) {
      case "failed":
        findings.push({
          key: `${entry.provider}-failed`,
          severity: "critical",
          text: `${tool} history could not be read on the last sync; its stored usage was kept.`,
        });
        break;
      case "partial":
        findings.push({
          key: `${entry.provider}-partial`,
          severity: "warning",
          text: `${tool} history was only partly readable on the last sync.`,
        });
        break;
      case "missing":
        if (entry.hasUsage) {
          findings.push({
            key: `${entry.provider}-missing`,
            severity: "warning",
            text: `No ${tool} history was found on the last sync, though earlier usage is stored.`,
          });
        }
        break;
      case "ok":
        break;
    }
    if (entry.unreadable > 0) {
      findings.push({
        key: `${entry.provider}-unreadable`,
        severity: "warning",
        text: `${formatCount(entry.unreadable)} ${tool} history ${entry.unreadable === 1 ? "file" : "files"} could not be read.`,
      });
    }
    if (entry.malformedLines > 0 || entry.skippedRecords > 0) {
      findings.push({
        key: `${entry.provider}-skipped`,
        severity: "note",
        text: `${tool}: ${formatCount(entry.malformedLines)} malformed lines and ${formatCount(entry.skippedRecords)} records skipped.`,
      });
    }
    if (entry.pricing?.status === "unavailable") {
      findings.push({
        key: `${entry.provider}-pricing`,
        severity: "warning",
        text: `Prices were unavailable for ${tool}, so its costs from this machine are unknown.`,
      });
    }
  }
  if (machine.usage?.cost.kind === "partial") {
    const unpriced = machine.usage.cost.unpricedRecords;
    findings.push({
      key: "unpriced",
      severity: "warning",
      text: `${formatCount(unpriced)} of its requests in these dates ${unpriced === 1 ? "has" : "have"} no known price, so ${unpriced === 1 ? "its" : "their"} cost is unknown.`,
    });
  }
  return findings;
}

/**
 * Whether a machine's figures may be incomplete or wrong: it has a finding
 * worse than a note. The Machines tab shows this from every other tab.
 */
export function needsAttention(
  machine: AiUsageMachine,
  staleAfterDays: number,
): boolean {
  return machineFindings(machine, staleAfterDays).some(
    (finding) => finding.severity !== "note",
  );
}
