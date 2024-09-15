'use client'

import { useState } from 'react'
import { DateRange, DayPicker } from 'react-day-picker'
import { format } from 'date-fns'
import { CalendarIcon } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'

type DateRangeSelectorProps = {
  onRangeChange: (range: DateRange) => void
}

export function DateRangeSelector({ onRangeChange }: DateRangeSelectorProps) {
  const [date, setDate] = useState<DateRange | undefined>({
    from: new Date(),
    to: new Date(),
  })

  const handleRangeSelect = (range: DateRange | undefined) => {
    if (range) {
      setDate(range)
      onRangeChange(range)
    }
  }

  const quickSelect = (days: number) => {
    const to = new Date()
    const from = new Date()
    from.setDate(from.getDate() - days)
    handleRangeSelect({ from, to })
  }

  return (
    <div className="flex items-center space-x-4">
      <Popover>
        <PopoverTrigger asChild>
          <Button variant="outline" className="w-[300px] justify-start text-left font-normal">
            <CalendarIcon className="mr-2 h-4 w-4" />
            {date?.from ? (
              date.to ? (
                <>
                  {format(date.from, "LLL dd, y")} - {format(date.to, "LLL dd, y")}
                </>
              ) : (
                format(date.from, "LLL dd, y")
              )
            ) : (
              <span>Pick a date</span>
            )}
          </Button>
        </PopoverTrigger>
        <PopoverContent className="w-auto p-0" align="start">
          <DayPicker
            mode="range"
            selected={date}
            onSelect={handleRangeSelect}
            numberOfMonths={2}
          />
        </PopoverContent>
      </Popover>
      <Button onClick={() => quickSelect(7)}>This Week</Button>
      <Button onClick={() => quickSelect(14)}>Last Week</Button>
      <Button onClick={() => quickSelect(30)}>This Month</Button>
    </div>
  )
}