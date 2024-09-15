'use client'

import { useState } from 'react'
import { PlusIcon } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import { Textarea } from '@/components/ui/textarea'

export function AddEntryButton() {
  const [isOpen, setIsOpen] = useState(false)

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    // Implement form submission logic here
    setIsOpen(false)
  }

  return (
    <Dialog open={isOpen} onOpenChange={setIsOpen}>
      <DialogTrigger asChild>
        <Button className="mt-4">
          <PlusIcon className="w-4 h-4 mr-2" />
          Add Entry
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Add New Time Entry</DialogTitle>
        </DialogHeader>
        <form onSubmit={handleSubmit} className="space-y-4">
          <Input type="number" placeholder="Duration (minutes)" required />
          <Select required>
            <option value="">Select Project</option>
            <option value="Project A">Project A</option>
            <option value="Project B">Project B</option>
          </Select>
          <Select required>
            <option value="">Select Activity</option>
            <option value="Development">Development</option>
            <option value="Meeting">Meeting</option>
          </Select>
          <Textarea placeholder="Note" />
          <Button type="submit">Add Entry</Button>
        </form>
      </DialogContent>
    </Dialog>
  )
}