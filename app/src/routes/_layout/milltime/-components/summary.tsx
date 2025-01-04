import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
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
import { format, parseISO, getISODay } from "date-fns"; // Added getISODay
import { formatHoursAsHoursMinutes } from "@/lib/utils";
import { match } from "ts-pattern";

type SummaryProps = {
  timeEntries: Array<TimeEntry>;
};

export function Summary({ timeEntries }: SummaryProps) {
  const totalHours = useMemo(
    () => timeEntries.reduce((sum, entry) => sum + entry.hours, 0),
    [timeEntries],
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
      {} as Record<string, number>,
    );

    const totalHours = Object.values(projectHours).reduce(
      (sum, hours) => sum + hours,
      0,
    );
    const threshold = totalHours * 0.01; // at least 1% of total hours

    return (
      Object.entries(projectHours)
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        .filter(([_, hours]) => hours >= threshold)
        .map(([name, value]) => ({
          name,
          value,
        }))
    );
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
      >,
    );

    // Get unique project names
    const projectNames = Array.from(
      new Set(timeEntries.map((entry) => entry.projectName)),
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
          {},
        ),
      }))
      .sort((a, b) => a.dayIndex - b.dayIndex)
      .map(({ name, ...rest }) => ({
        name,
        ...rest,
      }));
  }, [timeEntries]);

  const COLORS = [
    "#FF6B6B", // Coral Red
    "#4ECDC4", // Medium Turquoise
    "#FFA500", // Orange
    "#8A2BE2", // Blue Violet
    "#F7B731", // Saffron
    "#FF1493", // Deep Pink
    "#1E90FF", // Dodger Blue
    "#32CD32", // Lime Green
    "#FF4500", // Orange Red
    "#9370DB", // Medium Purple
    "#FFD700", // Gold
    "#00CED1", // Dark Turquoise
  ];

  return (
    <Card className="">
      <CardHeader>
        <CardTitle className="text-2xl">
          Summary{" "}
          <span className="text-muted-foreground">
            ({formatHoursAsHoursMinutes(totalHours)})
          </span>
        </CardTitle>
        <CardDescription>
          {timeEntries.length} {timeEntries.length === 1 ? "entry" : "entries"}{" "}
          {nUniqueProjects === 0
            ? ""
            : nUniqueProjects > 1
              ? `over ${nUniqueProjects} different projects`
              : "in one project"}
          .
        </CardDescription>
      </CardHeader>
      <CardContent>
        <h3 className="mb-2 text-lg font-semibold">Project Breakdown</h3>
        <div className="h-64">
          <ResponsiveContainer width="100%" height="100%">
            <PieChart>
              <Pie
                data={projectData}
                cx="50%"
                cy="50%"
                outerRadius={80}
                fill="#8884d8"
                dataKey="value"
                label={({ name, percent }) =>
                  `${name} ${(percent * 100).toFixed(0)}%`
                }
              >
                {projectData.map((entry, index) => (
                  <Cell
                    key={`cell-${index}-${entry.name}`}
                    fill={COLORS[index % COLORS.length]}
                  />
                ))}
              </Pie>
              <Tooltip
                content={<ProjectBreakdownTooltip totalHours={totalHours} />}
              />
            </PieChart>
          </ResponsiveContainer>
        </div>

        <h3 className="mb-2 mt-4 text-lg font-semibold">Daily Hours</h3>
        <div className="h-64">
          <ResponsiveContainer width="100%" height="100%">
            <BarChart data={dailyData}>
              <XAxis dataKey="name" />
              <YAxis />
              <Tooltip content={<DailyHoursTooltip />} />
              {projectData.map((project, index) => (
                <Bar
                  key={project.name}
                  dataKey={project.name}
                  stackId="a"
                  fill={COLORS[index % COLORS.length]}
                />
              ))}
            </BarChart>
          </ResponsiveContainer>
        </div>
      </CardContent>
    </Card>
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
      <div className="flex flex-col items-center justify-center rounded-md border border-border bg-background p-2">
        <p className="label">
          <span className="font-semibold">{name}</span>
        </p>
        <p className="label">
          <span className="text-muted-foreground">Time: </span>
          {formatHoursAsHoursMinutes(value as number)}
        </p>
        <p className="label">
          <span className="text-muted-foreground">Percentage: </span>
          {percent.toFixed(1)}%
        </p>
      </div>
    );
  }

  return null;
}

function DailyHoursTooltip(props: TooltipProps<number, string>) {
  if (props.active && props.payload && props.payload.length) {
    const nonZeroEntries = props.payload.filter(
      (entry) => (entry.value as number) > 0,
    );
    const totalHours = nonZeroEntries.reduce(
      (sum, entry) => sum + (entry.value as number),
      0,
    );

    if (totalHours === 0) return null;

    return (
      <div className="flex flex-col items-center justify-center rounded-md border border-border bg-background p-2">
        <p className="label font-semibold">{dayShortToLong(props.label)}</p>
        {nonZeroEntries.map((entry, index) => (
          <p key={`${entry.name}-${index}`} className="label">
            <span style={{ color: entry.color }}>{entry.name}: </span>
            {formatHoursAsHoursMinutes(entry.value as number)}
          </p>
        ))}
        <p className="label mt-1 border-t border-border pt-1">
          <span className="text-muted-foreground">Total: </span>
          {formatHoursAsHoursMinutes(totalHours)}
        </p>
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
