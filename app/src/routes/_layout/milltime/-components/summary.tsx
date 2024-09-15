'use client'

import { DateRange } from 'react-day-picker'
import { PieChart, Pie, Cell, ResponsiveContainer, BarChart, Bar, XAxis, YAxis, Tooltip } from 'recharts'

type SummaryProps = {
  dateRange: DateRange
}

export function Summary({ dateRange }: SummaryProps) {
  // Sample data - replace with actual data
  const totalHours = 40
  const projectData = [
    { name: 'Project A', value: 25 },
    { name: 'Project B', value: 15 },
  ]
  const dailyData = [
    { name: 'Mon', hours: 8 },
    { name: 'Tue', hours: 7 },
    { name: 'Wed', hours: 9 },
    { name: 'Thu', hours: 8 },
    { name: 'Fri', hours: 8 },
  ]

  const COLORS = ['#0088FE', '#00C49F', '#FFBB28', '#FF8042']

  return (
    <div className="bg-white dark:bg-gray-800 shadow-md rounded-lg p-4">
      <h2 className="text-xl font-semibold mb-4">Summary</h2>
      <p className="text-2xl font-bold mb-4">Total Hours: {totalHours}</p>
      
      <h3 className="text-lg font-semibold mb-2">Project Breakdown</h3>
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
              label={({ name, percent }) => `${name} ${(percent * 100).toFixed(0)}%`}
            >
              {projectData.map((entry, index) => (
                <Cell key={`cell-${index}`} fill={COLORS[index % COLORS.length]} />
              ))}
            </Pie>
          </PieChart>
        </ResponsiveContainer>
      </div>

      <h3 className="text-lg font-semibold mt-4 mb-2">Daily Hours</h3>
      <div className="h-64">
        <ResponsiveContainer width="100%" height="100%">
          <BarChart data={dailyData}>
            <XAxis dataKey="name" />
            <YAxis />
            <Tooltip />
            <Bar dataKey="hours" fill="#8884d8" />
          </BarChart>
        </ResponsiveContainer>
      </div>
    </div>
  )
}