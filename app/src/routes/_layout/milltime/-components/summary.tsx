import { TimeEntry } from "@/lib/api/queries/milltime";
import { useMemo } from "react";
import {
  PieChart,
  Pie,
  Cell,
  ResponsiveContainer,
  BarChart,
  Bar,
  XAxis,
  YAxis,
  Tooltip,
  TooltipProps,
} from "recharts";
import { format, parseISO, getISODay } from "date-fns";
import { formatHoursAsHoursMinutes } from "@/lib/utils";
import { match } from "ts-pattern";
import { BarChart3, PieChartIcon } from "lucide-react";

type SummaryProps = {
  timeEntries: Array<TimeEntry>;
};

// Refined color palette - warm amber to teal spectrum
const COLORS = [
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

export function Summary({ timeEntries }: SummaryProps) {
  const totalHours = useMemo(
    () => timeEntries.reduce((sum, entry) => sum + entry.hours, 0),
    [timeEntries]
  );

  const nUniqueProjects = useMemo(() => {
    return new Set(timeEntries.map((entry) => entry.projectName)).size;
  }, [timeEntries]);

  const projectData = useMemo(() => {
    const projectHours = timeEntries.reduce(
      (acc, entry) => {
        acc[entry.projectName] = (acc[entry.projectName] || 0) + entry.hours;
        return acc;
      },
      {} as Record<string, number>
    );

    const totalHours = Object.values(projectHours).reduce(
      (sum, hours) => sum + hours,
      0
    );
    const threshold = totalHours * 0.01;

    return Object.entries(projectHours)
      .filter(([, hours]) => hours >= threshold)
      .map(([name, value]) => ({
        name,
        value,
      }))
      .sort((a, b) => b.value - a.value);
  }, [timeEntries]);

  const dailyData = useMemo(() => {
    const dailyProjectHours = timeEntries.reduce(
      (acc, entry) => {
        const date = parseISO(entry.date);
        const day = format(date, "EEE");
        const dayIndex = getISODay(date);

        if (!acc[day]) {
          acc[day] = {
            date,
            dayIndex,
            projects: {} as Record<string, number>,
          };
        }

        acc[day].projects[entry.projectName] =
          (acc[day].projects[entry.projectName] || 0) + entry.hours;
        return acc;
      },
      {} as Record<
        string,
        {
          date: Date;
          dayIndex: number;
          projects: Record<string, number>;
        }
      >
    );

    const projectNames = Array.from(
      new Set(timeEntries.map((entry) => entry.projectName))
    );

    return Object.entries(dailyProjectHours)
      .map(([day, { dayIndex, projects }]) => ({
        name: day,
        dayIndex,
        ...projectNames.reduce(
          (acc, project) => ({
            ...acc,
            [project]: projects[project] || 0,
          }),
          {}
        ),
      }))
      .sort((a, b) => a.dayIndex - b.dayIndex)
      .map(({ name, ...rest }) => ({
        name,
        ...rest,
      }));
  }, [timeEntries]);

  return (
    <div className="card-elevated overflow-hidden rounded-2xl">
      {/* Header */}
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
                {timeEntries.length} {timeEntries.length === 1 ? "entry" : "entries"}
                {nUniqueProjects > 0 && (
                  <> across {nUniqueProjects} {nUniqueProjects === 1 ? "project" : "projects"}</>
                )}
              </p>
            </div>
          </div>
        </div>
      </div>

      {/* Charts */}
      <div className="p-5">
        {/* Project Breakdown - Donut Chart */}
        <div className="mb-6">
          <div className="mb-3 flex items-center gap-2">
            <PieChartIcon className="h-4 w-4 text-muted-foreground" />
            <h4 className="text-sm font-medium text-muted-foreground uppercase tracking-wider">
              By Project
            </h4>
          </div>
          <div className="h-48">
            <ResponsiveContainer width="100%" height="100%">
              <PieChart>
                <Pie
                  data={projectData}
                  cx="50%"
                  cy="50%"
                  innerRadius={45}
                  outerRadius={70}
                  paddingAngle={2}
                  dataKey="value"
                >
                  {projectData.map((entry, index) => (
                    <Cell
                      key={`cell-${index}-${entry.name}`}
                      fill={COLORS[index % COLORS.length]}
                      stroke="hsl(var(--background))"
                      strokeWidth={2}
                    />
                  ))}
                </Pie>
                <Tooltip
                  content={<ProjectBreakdownTooltip totalHours={totalHours} />}
                />
              </PieChart>
            </ResponsiveContainer>
          </div>
          {/* Legend */}
          <div className="mt-3 flex flex-wrap justify-center gap-x-4 gap-y-1">
            {projectData.slice(0, 4).map((project, index) => (
              <div key={project.name} className="flex items-center gap-1.5">
                <div
                  className="h-2.5 w-2.5 rounded-full"
                  style={{ backgroundColor: COLORS[index % COLORS.length] }}
                />
                <span className="text-xs text-muted-foreground truncate max-w-[100px]">
                  {project.name}
                </span>
              </div>
            ))}
            {projectData.length > 4 && (
              <span className="text-xs text-muted-foreground">
                +{projectData.length - 4} more
              </span>
            )}
          </div>
        </div>

        {/* Daily Hours - Bar Chart */}
        <div>
          <div className="mb-3 flex items-center gap-2">
            <BarChart3 className="h-4 w-4 text-muted-foreground" />
            <h4 className="text-sm font-medium text-muted-foreground uppercase tracking-wider">
              Daily Hours
            </h4>
          </div>
          <div className="h-44">
            <ResponsiveContainer width="100%" height="100%">
              <BarChart data={dailyData} barCategoryGap="20%">
                <XAxis
                  dataKey="name"
                  tick={{ fill: "hsl(var(--muted-foreground))", fontSize: 11 }}
                  axisLine={{ stroke: "hsl(var(--border))" }}
                  tickLine={false}
                />
                <YAxis
                  tick={{ fill: "hsl(var(--muted-foreground))", fontSize: 11 }}
                  axisLine={false}
                  tickLine={false}
                  width={30}
                />
                <Tooltip
                  content={<DailyHoursTooltip />}
                  cursor={{ fill: "hsl(var(--muted) / 0.5)" }}
                />
                {projectData.map((project, index) => (
                  <Bar
                    key={project.name}
                    dataKey={project.name}
                    stackId="a"
                    fill={COLORS[index % COLORS.length]}
                    radius={index === projectData.length - 1 ? [4, 4, 0, 0] : 0}
                  />
                ))}
              </BarChart>
            </ResponsiveContainer>
          </div>
        </div>
      </div>
    </div>
  );
}

function ProjectBreakdownTooltip({
  totalHours,
  ...props
}: TooltipProps<number, string> & { totalHours: number }) {
  if (props.active && props.payload && props.payload.length) {
    const { name, value } = props.payload[0];
    const percent = ((value as number) / totalHours) * 100;

    return (
      <div className="rounded-lg border border-border/50 bg-card/95 p-3 shadow-elevated backdrop-blur-sm">
        <p className="mb-1 font-semibold">{name}</p>
        <div className="space-y-0.5 text-sm">
          <p className="text-muted-foreground">
            <span className="text-foreground font-medium">
              {formatHoursAsHoursMinutes(value as number)}
            </span>
          </p>
          <p className="text-muted-foreground">
            {percent.toFixed(1)}% of total
          </p>
        </div>
      </div>
    );
  }
  return null;
}

function DailyHoursTooltip(props: TooltipProps<number, string>) {
  if (props.active && props.payload && props.payload.length) {
    const nonZeroEntries = props.payload.filter(
      (entry) => (entry.value as number) > 0
    );
    const totalHours = nonZeroEntries.reduce(
      (sum, entry) => sum + (entry.value as number),
      0
    );

    if (totalHours === 0) return null;

    return (
      <div className="rounded-lg border border-border/50 bg-card/95 p-3 shadow-elevated backdrop-blur-sm">
        <p className="mb-2 font-semibold">{dayShortToLong(props.label)}</p>
        <div className="space-y-1">
          {nonZeroEntries.slice(0, 5).map((entry, index) => (
            <div
              key={`${entry.name}-${index}`}
              className="flex items-center justify-between gap-4 text-sm"
            >
              <div className="flex items-center gap-1.5">
                <div
                  className="h-2 w-2 rounded-full"
                  style={{ backgroundColor: entry.color }}
                />
                <span className="text-muted-foreground truncate max-w-[120px]">
                  {entry.name}
                </span>
              </div>
              <span className="font-medium time-display">
                {formatHoursAsHoursMinutes(entry.value as number)}
              </span>
            </div>
          ))}
          {nonZeroEntries.length > 5 && (
            <p className="text-xs text-muted-foreground">
              +{nonZeroEntries.length - 5} more
            </p>
          )}
        </div>
        <div className="mt-2 border-t border-border/50 pt-2">
          <div className="flex items-center justify-between text-sm">
            <span className="text-muted-foreground">Total</span>
            <span className="font-semibold time-display">
              {formatHoursAsHoursMinutes(totalHours)}
            </span>
          </div>
        </div>
      </div>
    );
  }
  return null;
}

function dayShortToLong(dayShort: string) {
  return match(dayShort)
    .with("Mon", () => "Monday")
    .with("Tue", () => "Tuesday")
    .with("Wed", () => "Wednesday")
    .with("Thu", () => "Thursday")
    .with("Fri", () => "Friday")
    .with("Sat", () => "Saturday")
    .with("Sun", () => "Sunday")
    .otherwise(() => dayShort);
}
