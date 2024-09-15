'use client'

import { SearchIcon } from 'lucide-react'
import { Input } from '@/components/ui/input'

export function SearchBar() {
  return (
    <div className="relative">
      <SearchIcon className="absolute left-2 top-1/2 transform -translate-y-1/2 text-gray-400" />
      <Input
        type="text"
        placeholder="Search entries..."
        className="pl-8 w-64"
      />
    </div>
  )
}