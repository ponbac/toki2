import { useTheme } from "@/hooks/useTheme";
import { Moon, Sun, SunMoon } from "lucide-react";
import { match } from "ts-pattern";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";

const THEME_LABELS = {
  light: "Light",
  dark: "Dark",
  system: "System",
} as const;

export function ThemeSwitcher() {
  const { theme, resolvedTheme, setTheme } = useTheme();

  const resolvedThemeLabel = match(resolvedTheme)
    .with("dark", () => THEME_LABELS.dark)
    .with("light", () => THEME_LABELS.light)
    .exhaustive();

  const themeMeta = match(theme)
    .with("light", () => ({
      Icon: Sun,
      currentLabel: THEME_LABELS.light,
      nextTheme: "dark" as const,
    }))
    .with("dark", () => ({
      Icon: Moon,
      currentLabel: THEME_LABELS.dark,
      nextTheme: "system" as const,
    }))
    .with("system", () => ({
      Icon: SunMoon,
      currentLabel: `${THEME_LABELS.system} (${resolvedThemeLabel})`,
      nextTheme: "light" as const,
    }))
    .exhaustive();

  const { Icon, currentLabel, nextTheme } = themeMeta;
  const nextLabel = THEME_LABELS[nextTheme];
  const label = `Theme: ${currentLabel} (next: ${nextLabel})`;

  const toggle = () => setTheme(nextTheme);

  return (
    <Tooltip delayDuration={0}>
      <TooltipTrigger asChild>
        <button
          type="button"
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
        <span>
          Theme: {currentLabel}{" "}
          <span className="italic text-muted-foreground">(next: {nextLabel})</span>
        </span>
      </TooltipContent>
    </Tooltip>
  );
}
