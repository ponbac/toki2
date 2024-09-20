"use client";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { TimeEntry } from "@/lib/api/queries/milltime";
import { useMemo } from "react";
import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
} from "recharts";
import { format, startOfWeek, addDays } from "date-fns";

type DataVisualizationProps = {
  timeEntries: Array<TimeEntry>;
};

export function DataVisualization({ timeEntries }: DataVisualizationProps) {
  const data = useMemo(() => {
    const weekStart = startOfWeek(new Date());
    const weekDays = Array.from({ length: 7 }, (_, i) => addDays(weekStart, i));
    
    const activityData = weekDays.map((day) => {
      const dayEntries = timeEntries.filter((entry) => 
        format(entry.date, 'yyyy-MM-dd') === format(day, 'yyyy-MM-dd')
      );
      
      const activities = dayEntries.reduce((acc, entry) => {
        acc[entry.activityName] = (acc[entry.activityName] || 0) + entry.hours;
        return acc;
      }, {} as Record<string, number>);
      
      return {
        name: format(day, 'EEE'),
        ...activities,
      };
    });

    return activityData;
  }, [timeEntries]);

  const activities = useMemo(() => 
    Array.from(new Set(timeEntries.map(entry => entry.activityName))),
    [timeEntries]
  );

  const colors = ['#8884d8', '#82ca9d', '#ffc658', '#ff7300', '#a4de6c'];

  return (
    <Card>
      <CardHeader>
        <CardTitle>Activity Distribution</CardTitle>
      </CardHeader>
      <CardContent className="h-80">
        <ResponsiveContainer width="100%" height="100%">
          <AreaChart data={data}>
            <CartesianGrid strokeDasharray="3 3" />
            <XAxis dataKey="name" />
            <YAxis />
            <Tooltip />
            {activities.map((activity, index) => (
              <Area
                key={activity}
                type="monotone"
                dataKey={activity}
                stackId="1"
                stroke={colors[index % colors.length]}
                fill={colors[index % colors.length]}
              />
            ))}
          </AreaChart>
        </ResponsiveContainer>
      </CardContent>
    </Card>
  );
}
