'use client'

import { useState, useMemo } from 'react'
import { DateRange } from 'react-day-picker'
import { format, isEqual } from 'date-fns'
import { Edit2Icon, SaveIcon, XIcon } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { Select } from '@/components/ui/select'

type TimeEntry = {
  id: number
  date: Date
  duration: number
  project: string
  activity: string
  note: string
}

type TimeEntriesListProps = {
  dateRange: DateRange
}

export function TimeEntriesList({ dateRange }: TimeEntriesListProps) {
  const [entries, setEntries] = useState<TimeEntry[]>([
    // Sample data
    { id: 1, date: new Date(2023, 4, 15), duration: 120, project: 'Project A', activity: 'Development', note: 'Implemented new feature' },
    { id: 2, date: new Date(2023, 4, 15), duration: 60, project: 'Project B', activity: 'Meeting', note: 'Client call' },
    { id: 3, date: new Date(2023, 4, 16), duration: 90, project: 'Project A', activity: 'Planning', note: 'Sprint planning' },
    { id: 4, date: new Date(2023, 4, 17), duration: 180, project: 'Project C', activity: 'Development', note: 'Bug fixes' },
  ])
  const [editingId, setEditingId] = useState<number | null>(null)

  const groupedEntries = useMemo(() => {
    const groups: { [key: string]: TimeEntry[] } = {}
    entries.forEach(entry => {
      const dateKey = format(entry.date, 'yyyy-MM-dd')
      if (!groups[dateKey]) {
        groups[dateKey] = []
      }
      groups[dateKey].push(entry)
    })
    return Object.entries(groups).sort(([a], [b]) => new Date(b).getTime() - new Date(a).getTime())
  }, [entries])

  const formatDuration = (minutes: number) => {
    const hours = Math.floor(minutes / 60)
    const mins = minutes % 60
    return `${hours}h ${mins}m`
  }

  const handleEdit = (id: number) => {
    setEditingId(id)
  }

  const handleSave = (id: number) => {
    // Implement save logic here
    setEditingId(null)
  }

  const handleCancel = () => {
    setEditingId(null)
  }

  return (
    <div className="mt-8 space-y-8">
      {groupedEntries.map(([dateKey, dayEntries]) => (
        <div key={dateKey}>
          <h2 className="text-lg font-semibold mb-4">
            {format(new Date(dateKey), 'EEEE')}
            <span className="text-sm text-gray-500 dark:text-gray-400 ml-2">
              {format(new Date(dateKey), 'MMMM d, yyyy')}
            </span>
          </h2>
          <div className="space-y-4">
            {dayEntries.map((entry) => (
              <div key={entry.id} className="bg-white dark:bg-gray-800 shadow-md rounded-lg p-4">
                {editingId === entry.id ? (
                  <div className="space-y-4">
                    <div className="flex space-x-4">
                      <Input
                        type="number"
                        defaultValue={entry.duration}
                        className="w-24"
                        placeholder="Duration (minutes)"
                      />
                      <Select defaultValue={entry.project}>
                        <option value="Project A">Project A</option>
                        <option value="Project B">Project B</option>
                        <option value="Project C">Project C</option>
                      </Select>
                      <Select defaultValue={entry.activity}>
                        <option value="Development">Development</option>
                        <option value="Meeting">Meeting</option>
                        <option value="Planning">Planning</option>
                      </Select>
                    </div>
                    <Textarea defaultValue={entry.note} placeholder="Note" />
                    <div className="flex space-x-2">
                      <Button onClick={() => handleSave(entry.id)}>
                        <SaveIcon className="w-4 h-4 mr-2" />
                        Save
                      </Button>
                      <Button variant="outline" onClick={handleCancel}>
                        <XIcon className="w-4 h-4 mr-2" />
                        Cancel
                      </Button>
                    </div>
                  </div>
                ) : (
                  <div className="flex justify-between items-start">
                    <div>
                      <h3 className="text-lg font-semibold">{formatDuration(entry.duration)}</h3>
                      <p className="text-sm text-gray-600 dark:text-gray-400">{entry.project} - {entry.activity}</p>
                      <p className="text-sm mt-2">{entry.note}</p>
                    </div>
                    <Button variant="ghost" onClick={() => handleEdit(entry.id)}>
                      <Edit2Icon className="w-4 h-4" />
                    </Button>
                  </div>
                )}
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  )
}