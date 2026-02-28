import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { Button } from "@/components/ui/button";
import { Settings2Icon } from "lucide-react";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";

export const TimeTrackingSettings = ({
  rememberLastProject,
  setRememberLastProject,
}: {
  rememberLastProject: boolean;
  setRememberLastProject: (value: boolean) => void;
}) => {
  return (
    <Popover>
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>
            <PopoverTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="h-8 w-8 text-muted-foreground hover:bg-muted/60 hover:text-foreground"
              >
                <Settings2Icon className="h-4 w-4" />
                <span className="sr-only">Settings</span>
              </Button>
            </PopoverTrigger>
          </TooltipTrigger>
          <TooltipContent>
            <p>Settings</p>
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
      <PopoverContent
        align="end"
        className="w-80 rounded-xl border-border/70 bg-card/95 p-4 text-card-foreground shadow-elevated backdrop-blur supports-[backdrop-filter]:bg-card/90"
      >
        <div className="space-y-3">
          <h4 className="text-sm font-semibold leading-none text-foreground">
            Preferences
          </h4>
          <div className="flex items-center justify-between rounded-lg border border-border/60 bg-background/60 px-3 py-2">
            <div className="space-y-0.5">
              <Label
                htmlFor="remember-project"
                className="text-sm font-medium text-foreground"
              >
                Remember project
              </Label>
              <p className="pr-3 text-xs leading-relaxed text-muted-foreground">
                Auto-fill last used project and activity for new timers.
              </p>
            </div>
            <Switch
              id="remember-project"
              checked={rememberLastProject}
              onCheckedChange={setRememberLastProject}
            />
          </div>
        </div>
      </PopoverContent>
    </Popover>
  );
};
