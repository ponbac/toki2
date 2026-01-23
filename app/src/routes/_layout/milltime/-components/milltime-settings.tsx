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

export const MilltimeSettings = (props: {
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
                className="h-8 w-8 text-muted-foreground hover:text-foreground"
              >
                <Settings2Icon className="h-4 w-4" />
                <span className="sr-only">Settings</span>
              </Button>
            </PopoverTrigger>
          </TooltipTrigger>
          <TooltipContent>
            <p>Milltime settings</p>
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
      <PopoverContent
        align="end"
        className="w-72 border-zinc-600 bg-gradient-to-br from-zinc-800 to-zinc-900 shadow-lg"
      >
        <div className="space-y-4">
          <h4 className="text-sm font-medium leading-none">Preferences</h4>
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label htmlFor="remember-project" className="text-sm font-normal">
                Remember project
              </Label>
              <p className="text-xs text-muted-foreground">
                Auto-fill last used project and activity for new timers.
              </p>
            </div>
            <Switch
              id="remember-project"
              checked={props.rememberLastProject}
              onCheckedChange={props.setRememberLastProject}
            />
          </div>
        </div>
      </PopoverContent>
    </Popover>
  );
};
