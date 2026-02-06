import type { LucideIcon } from "lucide-react";
import {
  Briefcase,
  Code2,
  Layers,
  Palette,
  Lightbulb,
  Wrench,
  BookOpen,
  Rocket,
  Compass,
  Gem,
} from "lucide-react";

// Refined color palette - warm amber to teal spectrum
export const COLORS = [
  "hsl(38 95% 55%)",   // Primary amber
  "hsl(172 66% 50%)",  // Teal
  "hsl(262 83% 58%)",  // Purple
  "hsl(350 89% 60%)",  // Rose
  "hsl(142 71% 45%)",  // Emerald
  "hsl(217 91% 60%)",  // Blue
  "hsl(45 93% 58%)",   // Yellow
  "hsl(280 68% 60%)",  // Violet
  "hsl(195 74% 50%)",  // Cyan
  "hsl(24 95% 55%)",   // Orange
];

export const PROJECT_ICONS: LucideIcon[] = [
  Briefcase,
  Code2,
  Layers,
  Palette,
  Lightbulb,
  Wrench,
  BookOpen,
  Rocket,
  Compass,
  Gem,
];

export type ProjectStyle = {
  color: string;
  Icon: LucideIcon;
};

/** Convert `hsl(38 95% 55%)` to `hsl(38 95% 55% / alpha)` */
export function withAlpha(hslColor: string, alpha: number): string {
  return hslColor.replace(")", ` / ${alpha})`);
}

/** Sort projects by total hours descending, return the sorted list. */
function sortProjectsByHours(
  entries: Array<{ projectName: string; hours: number }>,
): string[] {
  const projectHours = new Map<string, number>();
  entries.forEach((e) => {
    projectHours.set(
      e.projectName,
      (projectHours.get(e.projectName) || 0) + e.hours,
    );
  });
  const projects = [...new Set(entries.map((e) => e.projectName))];
  projects.sort(
    (a, b) => (projectHours.get(b) || 0) - (projectHours.get(a) || 0),
  );
  return projects;
}

/** Assign a color from COLORS to each project, sorted by total hours (most hours = first color). */
export function buildProjectColorMap(
  entries: Array<{ projectName: string; hours: number }>,
): Map<string, string> {
  const projects = sortProjectsByHours(entries);
  const colorMap = new Map<string, string>();
  projects.forEach((project, i) => {
    colorMap.set(project, COLORS[i % COLORS.length]);
  });
  return colorMap;
}

/** Assign a color and icon to each project, sorted by total hours. */
export function buildProjectStyleMap(
  entries: Array<{ projectName: string; hours: number }>,
): Map<string, ProjectStyle> {
  const projects = sortProjectsByHours(entries);
  const styleMap = new Map<string, ProjectStyle>();
  projects.forEach((project, i) => {
    styleMap.set(project, {
      color: COLORS[i % COLORS.length],
      Icon: PROJECT_ICONS[i % PROJECT_ICONS.length],
    });
  });
  return styleMap;
}
