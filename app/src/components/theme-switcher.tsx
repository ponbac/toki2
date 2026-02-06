import { useTheme } from "@/hooks/useTheme";
import { Moon, Sun } from "lucide-react";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";

export function ThemeSwitcher() {
  const { resolvedTheme, setTheme } = useTheme();

  const toggle = () => setTheme(resolvedTheme === "dark" ? "light" : "dark");
  const Icon = resolvedTheme === "dark" ? Moon : Sun;
  const label = resolvedTheme === "dark" ? "Dark mode" : "Light mode";

  return (
    <Tooltip delayDuration={0}>
      <TooltipTrigger asChild>
        <button
          onClick={toggle}
          className="flex h-10 w-10 items-center justify-center rounded-xl text-muted-foreground transition-all duration-300 hover:bg-primary/10 hover:text-foreground"
          aria-label={label}
        >
          <Icon className="h-5 w-5 transition-transform duration-300 hover:scale-110" />
        </button>
      </TooltipTrigger>
      <TooltipContent
        side="right"
        className="rounded-lg border-border/50 bg-card/95 px-3 py-2 font-medium shadow-elevated backdrop-blur-sm"
      >
        {label}
      </TooltipContent>
    </Tooltip>
  );
}
