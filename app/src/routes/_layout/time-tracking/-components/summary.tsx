import type { TimeEntry } from "@/lib/api/queries/time-tracking";
import { useMemo } from "react";
import { barY, crosshair, defineChart, stack } from "@tanstack/charts";
import { pie, polar, radialArc } from "@tanstack/charts/polar";
import { Chart } from "@tanstack/charts/react/tooltip";
import { scaleBand } from "@tanstack/charts/scales/band";
import { scaleLinear } from "@tanstack/charts/scales/linear";
import { format, parseISO } from "date-fns";
import { formatHoursAsHoursMinutes } from "@/lib/utils";
import {
  USAGE_CHART_AXIS,
  USAGE_CHART_THEME,
  USAGE_CHART_TOOLTIP,
} from "@/lib/chart-theme";
import { BarChart3, PieChartIcon } from "lucide-react";
import { COLORS } from "./colors";

const OTHER_COLOR = "hsl(var(--muted-foreground))";

type ProjectRow = Readonly<{
  key: string;
  name: string;
  hours: number;
  color: string;
  members: readonly string[];
  isOther: boolean;
}>;

type DailyProject = Readonly<{
  key: string;
  name: string;
  hours: number;
  color: string;
  projectCount: number;
  isOther: boolean;
}>;

type DailyRow = Readonly<{
  date: string;
  hours: number;
  projects: readonly DailyProject[];
}>;

type DailyPoint = Readonly<{
  key: string;
  row: DailyRow;
  project: DailyProject;
}>;

/** Summarizes recorded hours by project and local calendar date. */
export function Summary({
  timeEntries,
}: {
  timeEntries: readonly TimeEntry[];
}) {
  const { totalHours, projectData, dailyData, nUniqueProjects } =
    useMemo(() => {
      const projectHours = new Map<string, number>();
      const days = new Map<string, Map<string, number>>();
      let totalHours = 0;

      for (const entry of timeEntries) {
        totalHours += entry.hours;
        projectHours.set(
          entry.projectName,
          (projectHours.get(entry.projectName) ?? 0) + entry.hours,
        );
        // Dates are local calendar dates; parsing them avoids a UTC date shift.
        const date = format(parseISO(entry.date), "yyyy-MM-dd");
        const dailyHours = days.get(date) ?? new Map<string, number>();
        dailyHours.set(
          entry.projectName,
          (dailyHours.get(entry.projectName) ?? 0) + entry.hours,
        );
        days.set(date, dailyHours);
      }

      const threshold = totalHours * 0.01;
      const projects = [...projectHours]
        .filter(([, hours]) => hours > 0)
        .sort((a, b) => b[1] - a[1]);
      const projectData: ProjectRow[] = [];
      const minorProjects: string[] = [];
      let otherHours = 0;

      for (const [name, hours] of projects) {
        if (hours < threshold) {
          minorProjects.push(name);
          otherHours += hours;
        } else {
          projectData.push({
            key: JSON.stringify(["project", name]),
            name,
            hours,
            color: COLORS[projectData.length % COLORS.length],
            members: [name],
            isOther: false,
          });
        }
      }

      if (minorProjects.length > 0) {
        projectData.push({
          key: JSON.stringify(["other"]),
          name: "Other projects",
          hours: otherHours,
          color: OTHER_COLOR,
          members: minorProjects,
          isOther: true,
        });
      }

      const dailyData: DailyRow[] = [...days]
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([date, hoursByProject]) => ({
          date,
          hours: [...hoursByProject.values()].reduce(
            (sum, hours) => sum + hours,
            0,
          ),
          projects: projectData.map((project) => ({
            key: project.key,
            name: project.name,
            color: project.color,
            isOther: project.isOther,
            hours: project.members.reduce(
              (sum, name) => sum + (hoursByProject.get(name) ?? 0),
              0,
            ),
            projectCount: project.members.filter(
              (name) => (hoursByProject.get(name) ?? 0) > 0,
            ).length,
          })),
        }));

      return {
        totalHours,
        projectData,
        dailyData,
        nUniqueProjects: projectHours.size,
      };
    }, [timeEntries]);

  return (
    <div className="card-elevated overflow-hidden rounded-2xl">
      <div className="relative overflow-hidden border-b border-border/50 bg-gradient-to-br from-primary/10 via-primary/5 to-transparent px-5 py-4">
        <div className="absolute -right-8 -top-8 h-24 w-24 rounded-full bg-primary/10 blur-2xl" />
        <div className="relative">
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-primary/20">
              <PieChartIcon className="h-5 w-5 text-primary" />
            </div>
            <div>
              <h3 className="font-display text-lg font-semibold">
                Summary
                <span className="ml-2 text-muted-foreground">
                  ({formatHoursAsHoursMinutes(totalHours)})
                </span>
              </h3>
              <p className="text-sm text-muted-foreground">
                {timeEntries.length}{" "}
                {timeEntries.length === 1 ? "entry" : "entries"}
                {nUniqueProjects > 0 ? (
                  <>
                    {" "}
                    across {nUniqueProjects}{" "}
                    {nUniqueProjects === 1 ? "project" : "projects"}
                  </>
                ) : null}
              </p>
            </div>
          </div>
        </div>
      </div>

      <div className="p-5">
        <div className="mb-6">
          <div className="mb-3 flex items-center gap-2">
            <PieChartIcon className="h-4 w-4 text-muted-foreground" />
            <h4 className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
              By Project
            </h4>
          </div>
          <ProjectBreakdownChart
            projects={projectData}
            totalHours={totalHours}
          />
        </div>

        <div>
          <div className="mb-3 flex items-center gap-2">
            <BarChart3 className="h-4 w-4 text-muted-foreground" />
            <h4 className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
              Daily Hours
            </h4>
          </div>
          <DailyHoursChart rows={dailyData} projects={projectData} />
        </div>
      </div>
    </div>
  );
}

function ProjectBreakdownChart({
  projects,
  totalHours,
}: {
  projects: readonly ProjectRow[];
  totalHours: number;
}) {
  const definition = useMemo(() => {
    const slices = pie(projects, {
      value: "hours",
      startAngle: Math.PI / 2,
      endAngle: (-3 * Math.PI) / 2,
      gapAngle: projects.length > 1 ? (2 * Math.PI) / 180 : 0,
    });

    return defineChart({
      marks: [
        polar({
          marks: [
            radialArc(slices, {
              key: "key",
              innerRadius: 45,
              outerRadius: 70,
              fill: (slice) => slice.color,
              stroke: "hsl(var(--background))",
              strokeWidth: 2,
              states: [
                { when: { focus: "unmatched" }, style: { opacity: 0.45 } },
              ],
            }),
          ],
          scales: { angle: null, radius: null },
        }),
      ],
      scales: { x: null, y: null },
      margin: 0,
      theme: USAGE_CHART_THEME,
      focusRing: false,
      tooltip: {
        ...USAGE_CHART_TOOLTIP,
        format: ({ datum }) =>
          [
            datum.name,
            formatHoursAsHoursMinutes(datum.hours),
            `${formatProjectShare(datum.hours, totalHours)} of total`,
            ...(datum.isOther
              ? [
                  `${formatProjectCount(datum.members.length)} below 1% of total`,
                  ...datum.members.slice(0, 5),
                  ...(datum.members.length > 5
                    ? [`+${datum.members.length - 5} more`]
                    : []),
                ]
              : []),
          ].join("\n"),
      },
    });
  }, [projects, totalHours]);

  const other = projects.find((project) => project.isOther);
  const firstProjects = projects.slice(0, 6);
  const legendProjects =
    other && !firstProjects.includes(other)
      ? [...firstProjects, other]
      : firstProjects;
  const hiddenProjects = projects.length - legendProjects.length;

  return (
    <>
      <div className="h-48">
        {projects.length > 0 ? (
          <Chart
            definition={definition}
            height={192}
            className="w-full tabular-nums"
            ariaLabel="Recorded hours by project"
            ariaDescription="Each slice shows a project's share of total recorded time. Projects under 1% are grouped as Other projects. Use arrow keys to inspect projects and Enter to pin details."
            renderTooltipBody={({ points }) => {
              const project = points[0]?.datum;
              return project ? (
                <ProjectBreakdownTooltip
                  project={project}
                  totalHours={totalHours}
                />
              ) : null;
            }}
          />
        ) : (
          <EmptyChart />
        )}
      </div>
      {legendProjects.length > 0 ? (
        <div className="mt-3 flex flex-wrap justify-center gap-x-4 gap-y-1">
          {legendProjects.map((project) => (
            <div
              key={project.key}
              className="flex items-center gap-1.5"
              title={project.name}
            >
              <div
                className="h-2.5 w-2.5 shrink-0 rounded-full"
                style={{ backgroundColor: project.color }}
              />
              <span className="max-w-[200px] truncate text-xs text-muted-foreground">
                {project.name}
              </span>
            </div>
          ))}
          {hiddenProjects > 0 ? (
            <span className="text-xs text-muted-foreground">
              +{hiddenProjects} more
            </span>
          ) : null}
        </div>
      ) : null}
      {other ? (
        <p className="mt-2 text-center text-xs text-muted-foreground">
          {formatProjectCount(other.members.length)} below 1% of total, shown as
          Other projects.
        </p>
      ) : null}
    </>
  );
}

function DailyHoursChart({
  rows,
  projects,
}: {
  rows: readonly DailyRow[];
  projects: readonly ProjectRow[];
}) {
  const definition = useMemo(() => {
    const singleWeek =
      new Set(rows.map((row) => format(parseISO(row.date), "RRRR-II"))).size <=
      1;
    const points: DailyPoint[] = rows.flatMap((row) =>
      row.projects.map((project) => ({
        key: JSON.stringify([row.date, project.key]),
        row,
        project,
      })),
    );

    return defineChart({
      marks: [
        crosshair({
          x: {
            band: {
              fill: "hsl(var(--muted) / 0.5)",
              fillOpacity: 1,
              inset: -4,
            },
          },
          y: false,
        }),
        barY(points, {
          x: (point) => point.row.date,
          y: (point) => point.project.hours,
          color: (point) => point.project.key,
          key: "key",
          layout: stack({ order: projects.map((project) => project.key) }),
          radius: { end: 4 },
        }),
      ],
      scales: {
        x: {
          scale: scaleBand<string>()
            .domain(rows.map((row) => row.date))
            .paddingInner(0.4)
            .paddingOuter(0.2),
          axis: {
            ...USAGE_CHART_AXIS,
            line: { stroke: "hsl(var(--border))", strokeOpacity: 1 },
            ticks: {
              size: 0,
              line: false,
              format: (date) =>
                format(parseISO(date), singleWeek ? "EEE" : "MMM d"),
            },
            tickLabels: {
              fontSize: 11,
              opacity: 1,
              dy: 3,
              thin: { minGap: 12, priority: "ends" },
            },
          },
        },
        y: {
          scale: scaleLinear,
          nice: true,
          axis: {
            ...USAGE_CHART_AXIS,
            ticks: { size: 0, line: false, count: 5 },
          },
        },
      },
      color: {
        domain: projects.map((project) => project.key),
        range: projects.map((project) => project.color),
      },
      theme: USAGE_CHART_THEME,
      margin: { left: 35 },
      focus: "group-x",
      focusRing: false,
      tooltip: {
        ...USAGE_CHART_TOOLTIP,
        formatGroup: (points) => {
          const row = points[0]?.datum.row;
          return row
            ? [
                format(parseISO(row.date), "EEEE, MMM d, yyyy"),
                ...row.projects
                  .filter((project) => project.hours > 0)
                  .map(
                    (project) =>
                      `${project.name}${project.isOther ? ` (${formatProjectCount(project.projectCount)})` : ""}: ${formatHoursAsHoursMinutes(project.hours)}`,
                  ),
                `Total: ${formatHoursAsHoursMinutes(row.hours)}`,
              ].join("\n")
            : "";
        },
      },
    });
  }, [rows, projects]);

  return (
    <div className="h-44">
      {projects.length > 0 ? (
        <Chart
          definition={definition}
          height={176}
          className="w-full tabular-nums"
          ariaLabel="Recorded hours per local day, stacked by project"
          ariaDescription="Bars include all recorded time, with projects under 1% of total grouped as Other projects. Use arrow keys to inspect dates and Enter to pin details."
          renderTooltipBody={({ points }) => {
            const row = points[0]?.datum.row;
            return row ? <DailyHoursTooltip row={row} /> : null;
          }}
        />
      ) : (
        <EmptyChart />
      )}
    </div>
  );
}

function EmptyChart() {
  return (
    <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
      No recorded time in this range.
    </div>
  );
}

function ProjectBreakdownTooltip({
  project,
  totalHours,
}: {
  project: ProjectRow;
  totalHours: number;
}) {
  return (
    <div className="max-w-60 rounded-lg border border-border/50 bg-card/95 p-3 shadow-elevated backdrop-blur-sm">
      <p className="mb-1 break-words font-semibold">{project.name}</p>
      <div className="space-y-0.5 text-sm">
        <p className="text-muted-foreground">
          <span className="font-medium text-foreground">
            {formatHoursAsHoursMinutes(project.hours)}
          </span>
        </p>
        <p className="text-muted-foreground">
          {formatProjectShare(project.hours, totalHours)} of total
        </p>
        {project.isOther ? (
          <>
            <p className="text-xs text-muted-foreground">
              {formatProjectCount(project.members.length)} below 1% of total
            </p>
            <ul className="break-words text-xs text-muted-foreground">
              {project.members.slice(0, 5).map((name) => (
                <li key={name}>{name}</li>
              ))}
            </ul>
            {project.members.length > 5 ? (
              <p className="text-xs text-muted-foreground">
                +{project.members.length - 5} more
              </p>
            ) : null}
          </>
        ) : null}
      </div>
    </div>
  );
}

function DailyHoursTooltip({ row }: { row: DailyRow }) {
  const nonZeroProjects = row.projects.filter((project) => project.hours > 0);

  return (
    <div className="rounded-lg border border-border/50 bg-card/95 p-3 shadow-elevated backdrop-blur-sm">
      <p className="mb-2 font-semibold">
        {format(parseISO(row.date), "EEEE, MMM d, yyyy")}
      </p>
      <div className="space-y-1">
        {nonZeroProjects.slice(0, 5).map((project) => (
          <div
            key={project.key}
            className="flex items-center justify-between gap-4 text-sm"
          >
            <div className="flex min-w-0 items-center gap-1.5">
              <div
                className="h-2 w-2 shrink-0 rounded-full"
                style={{ backgroundColor: project.color }}
              />
              <div className="min-w-0">
                <span
                  className="block max-w-[120px] truncate text-muted-foreground"
                  title={project.name}
                >
                  {project.name}
                </span>
                {project.isOther ? (
                  <span className="block text-xs text-muted-foreground">
                    {formatProjectCount(project.projectCount)}
                  </span>
                ) : null}
              </div>
            </div>
            <span className="time-display shrink-0 font-medium">
              {formatHoursAsHoursMinutes(project.hours)}
            </span>
          </div>
        ))}
        {nonZeroProjects.length > 5 ? (
          <p className="text-xs text-muted-foreground">
            +{nonZeroProjects.length - 5} more
          </p>
        ) : null}
      </div>
      <div className="mt-2 border-t border-border/50 pt-2">
        <div className="flex items-center justify-between text-sm">
          <span className="text-muted-foreground">Total</span>
          <span className="time-display font-semibold">
            {formatHoursAsHoursMinutes(row.hours)}
          </span>
        </div>
      </div>
    </div>
  );
}

function formatProjectCount(count: number): string {
  return `${count} ${count === 1 ? "project" : "projects"}`;
}

/** Keeps a positive fraction visible even when it rounds below one decimal. */
function formatProjectShare(hours: number, totalHours: number): string {
  const percentage = totalHours > 0 ? (hours / totalHours) * 100 : 0;
  return percentage > 0 && percentage < 0.1
    ? "<0.1%"
    : `${percentage.toFixed(1)}%`;
}
