'use client'

import { DateRange } from 'react-day-picker'
import { AreaChart, Area, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from 'recharts'

type DataVisualizationProps = {
  dateRange: DateRange
}

export function DataVisualization({ dateRange }: DataVisualizationProps) {
  // Sample data - replace with actual data
  const data = [
    { name: 'Mon', Development: 4, Meeting: 2, Planning: 2 },
    { name: 'Tue', Development: 3, Meeting: 1, Planning: 3 },
    { name: 'Wed', Development: 5, Meeting: 3, Planning: 1 },
    { name: 'Thu', Development: 4, Meeting: 2, Planning: 2 },
    { name: 'Fri', Development: 3, Meeting: 2, Planning: 3 },
  ]

  return (
    <div className="bg-white dark:bg-gray-800 shadow-md rounded-lg p-4 mt-8">
      <h2 className="text-xl font-semibold mb-4">Activity Distribution</h2>
      <div className="h-80">
        <ResponsiveContainer width="100%" height="100%">
          <AreaChart data={data}>
            <CartesianGrid strokeDasharray="3 3" />
            <XAxis dataKey="name" />
            <YAxis />
            <Tooltip />
            <Area type="monotone" dataKey="Development" stackId="1" stroke="#8884d8" fill="#8884d8" />
            <Area type="monotone" dataKey="Meeting" stackId="1" stroke="#82ca9d" fill="#82ca9d" />
            <Area type="monotone" dataKey="Planning" stackId="1" stroke="#ffc658" fill="#ffc658" />
          </AreaChart>
        </ResponsiveContainer>
      </div>
    </div>
  )
}