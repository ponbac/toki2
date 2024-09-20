import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
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
} from "recharts";
import { format, parseISO } from "date-fns";
import { formatHoursAsHoursMinutes } from "@/lib/utils";

type SummaryProps = {
  timeEntries: Array<TimeEntry>;
};

export function Summary({ timeEntries }: SummaryProps) {
  const totalHours = useMemo(
    () => timeEntries.reduce((sum, entry) => sum + entry.hours, 0),
    [timeEntries],
  );

  const projectData = useMemo(() => {
    const projectHours = timeEntries.reduce(
      (acc, entry) => {
        acc[entry.projectName] = (acc[entry.projectName] || 0) + entry.hours;
        return acc;
      },
      {} as Record<string, number>,
    );

    return Object.entries(projectHours).map(([name, value]) => ({
      name,
      value,
    }));
  }, [timeEntries]);

  const dailyData = useMemo(() => {
    const dailyHours = timeEntries.reduce(
      (acc, entry) => {
        const date = parseISO(entry.date);
        const day = format(date, "EEE");
        if (!acc[day]) {
          acc[day] = { date, hours: 0 };
        }
        acc[day].hours += entry.hours;
        return acc;
      },
      {} as Record<string, { date: Date; hours: number }>,
    );

    return Object.values(dailyHours)
      .sort((a, b) => a.date.getTime() - b.date.getTime())
      .map(({ date, hours }) => ({ name: format(date, "EEE"), hours }));
  }, [timeEntries]);

  const COLORS = ["#0088FE", "#00C49F", "#FFBB28", "#FF8042"];

  return (
    <Card className="">
      <CardHeader>
        <CardTitle>Summary</CardTitle>
      </CardHeader>
      <CardContent>
        <p className="mb-4 text-2xl font-bold">
          Total Hours: {totalHours.toFixed(2)}
        </p>

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
            </PieChart>
          </ResponsiveContainer>
        </div>

        <h3 className="mb-2 mt-4 text-lg font-semibold">Daily Hours</h3>
        <div className="h-64">
          <ResponsiveContainer width="100%" height="100%">
            <BarChart data={dailyData}>
              <XAxis dataKey="name" />
              <YAxis />
              <Tooltip
                formatter={(value) =>
                  formatHoursAsHoursMinutes(value as number)
                }
              />
              <Bar dataKey="hours" fill="#8884d8" />
            </BarChart>
          </ResponsiveContainer>
        </div>
      </CardContent>
    </Card>
  );
}
