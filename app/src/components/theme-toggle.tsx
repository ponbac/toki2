'use client'

import { MoonIcon, SunIcon } from 'lucide-react'
import { Button } from '@/components/ui/button'

type ThemeToggleProps = {
  darkMode: boolean
  setDarkMode: (value: boolean) => void
}

export function ThemeToggle({ darkMode, setDarkMode }: ThemeToggleProps) {
  return (
    <Button variant="ghost" onClick={() => setDarkMode(!darkMode)}>
      {darkMode ? <SunIcon className="w-5 h-5" /> : <MoonIcon className="w-5 h-5" />}
    </Button>
  )
}