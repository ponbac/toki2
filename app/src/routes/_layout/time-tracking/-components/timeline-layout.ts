import { parseISO } from "date-fns";
import type { TimeEntry } from "@/lib/api/queries/time-tracking";

export const SHORT_ENTRY_THRESHOLD_PX = 20;
export const MICRO_CARD_HEIGHT_PX = 16;
export const MICRO_MAX_LANES = 3;
export const MICRO_LANE_X_OFFSET_PX = 10;

export type TimelineVisualVariant = "full" | "micro";

export type TimelineLaidOutEntry = TimeEntry & {
  startMs: number;
  endMs: number;
  topPx: number;
  bottomPx: number;
  durationPx: number;
  visualVariant: TimelineVisualVariant;
  visualHeightPx: number;
  microLane: number;
  column: number;
  totalColumns: number;
};

function clamp(value: number, min: number, max: number) {
  return Math.max(min, Math.min(value, max));
}

function getHourInDay(date: Date) {
  return (
    date.getHours() +
    date.getMinutes() / 60 +
    date.getSeconds() / 3600 +
    date.getMilliseconds() / 3_600_000
  );
}

function overlaps(a: TimelineLaidOutEntry, b: TimelineLaidOutEntry) {
  return a.startMs < b.endMs && b.startMs < a.endMs;
}

function assignColumns(entries: TimelineLaidOutEntry[]) {
  if (entries.length <= 1) {
    entries.forEach((entry) => {
      entry.column = 0;
      entry.totalColumns = 1;
    });
    return;
  }

  const sorted = [...entries].sort((a, b) => a.startMs - b.startMs || b.endMs - a.endMs);

  // Build transitive overlap clusters.
  const clusterByIndex = new Map<number, number>();
  let nextCluster = 0;

  for (let i = 0; i < sorted.length; i++) {
    let cluster = -1;
    for (let j = 0; j < i; j++) {
      if (!overlaps(sorted[j], sorted[i])) continue;
      const jCluster = clusterByIndex.get(j)!;
      if (cluster === -1) {
        cluster = jCluster;
        continue;
      }

      if (cluster !== jCluster) {
        const oldCluster = jCluster;
        for (const [idx, candidate] of clusterByIndex) {
          if (candidate === oldCluster) clusterByIndex.set(idx, cluster);
        }
      }
    }

    clusterByIndex.set(i, cluster === -1 ? nextCluster++ : cluster);
  }

  const membersByCluster = new Map<number, number[]>();
  for (const [idx, cluster] of clusterByIndex) {
    const members = membersByCluster.get(cluster) ?? [];
    members.push(idx);
    membersByCluster.set(cluster, members);
  }

  for (const members of membersByCluster.values()) {
    if (members.length === 1) {
      const only = sorted[members[0]];
      only.column = 0;
      only.totalColumns = 1;
      continue;
    }

    members.sort((a, b) => {
      const startDelta = sorted[a].startMs - sorted[b].startMs;
      if (startDelta !== 0) return startDelta;
      return sorted[b].endMs - sorted[a].endMs;
    });

    const assigned: number[] = [];

    for (const idx of members) {
      const used = new Set<number>();
      for (const otherIdx of assigned) {
        if (overlaps(sorted[idx], sorted[otherIdx])) {
          used.add(sorted[otherIdx].column);
        }
      }

      let column = 0;
      while (used.has(column)) column++;
      sorted[idx].column = column;
      assigned.push(idx);
    }

    const totalColumns =
      Math.max(...members.map((member) => sorted[member].column)) + 1;

    for (const idx of members) {
      sorted[idx].totalColumns = totalColumns;
    }
  }
}

function assignMicroLanes(
  entries: TimelineLaidOutEntry[],
  microMaxLanes: number,
  microCardHeightPx: number,
) {
  const laneBottomByColumn = new Map<string, number[]>();

  const microEntries = entries
    .filter((entry) => entry.visualVariant === "micro")
    .sort((a, b) => a.topPx - b.topPx || a.startMs - b.startMs);

  for (const entry of microEntries) {
    const key = `${entry.column}/${entry.totalColumns}`;
    const laneBottoms =
      laneBottomByColumn.get(key) ??
      Array.from({ length: microMaxLanes }, () => Number.NEGATIVE_INFINITY);

    let lane = laneBottoms.findIndex((bottom) => bottom <= entry.topPx);
    if (lane === -1) lane = microMaxLanes - 1;

    laneBottoms[lane] = Math.max(
      laneBottoms[lane],
      entry.topPx + microCardHeightPx,
    );
    entry.microLane = lane;
    laneBottomByColumn.set(key, laneBottoms);
  }
}

export function layoutDayEntries({
  dayEntries,
  gridStartHour,
  gridEndHour,
  hourHeightPx,
  shortEntryThresholdPx = SHORT_ENTRY_THRESHOLD_PX,
  microCardHeightPx = MICRO_CARD_HEIGHT_PX,
  microMaxLanes = MICRO_MAX_LANES,
}: {
  dayEntries: TimeEntry[];
  gridStartHour: number;
  gridEndHour: number;
  hourHeightPx: number;
  shortEntryThresholdPx?: number;
  microCardHeightPx?: number;
  microMaxLanes?: number;
}): TimelineLaidOutEntry[] {
  if (dayEntries.length === 0) return [];

  const gridHeight = Math.max(0, (gridEndHour - gridStartHour) * hourHeightPx);
  if (gridHeight === 0) return [];

  const entries: TimelineLaidOutEntry[] = [];

  for (const entry of dayEntries) {
    if (!entry.startTime || !entry.endTime) continue;

    const start = parseISO(entry.startTime);
    const end = parseISO(entry.endTime);
    const startMs = start.getTime();
    const endMs = end.getTime();

    if (!Number.isFinite(startMs) || !Number.isFinite(endMs) || endMs <= startMs) {
      continue;
    }

    const topPx = clamp(
      (getHourInDay(start) - gridStartHour) * hourHeightPx,
      0,
      gridHeight,
    );
    const bottomPx = clamp(
      (getHourInDay(end) - gridStartHour) * hourHeightPx,
      0,
      gridHeight,
    );

    if (bottomPx <= topPx) continue;

    const durationPx = Math.max(1, bottomPx - topPx);
    const visualVariant =
      durationPx < shortEntryThresholdPx ? "micro" : "full";

    entries.push({
      ...entry,
      startMs,
      endMs,
      topPx,
      bottomPx,
      durationPx,
      visualVariant,
      visualHeightPx:
        visualVariant === "micro" ? microCardHeightPx : durationPx,
      microLane: 0,
      column: 0,
      totalColumns: 1,
    });
  }

  if (entries.length === 0) return [];

  assignColumns(entries);
  assignMicroLanes(entries, microMaxLanes, microCardHeightPx);

  return entries.sort((a, b) => a.topPx - b.topPx || a.startMs - b.startMs);
}
