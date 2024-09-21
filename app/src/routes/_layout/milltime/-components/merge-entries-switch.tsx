import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";

export const MergeEntriesSwitch = (props: {
  mergeSameDay: boolean;
  setMergeSameDay: (mergeSameDay: boolean) => void;
}) => {
  return (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger>
          <div className="flex items-center space-x-2">
            <Switch
              id="merge-same-day"
              checked={props.mergeSameDay}
              onCheckedChange={props.setMergeSameDay}
            />
            <Label htmlFor="merge-same-day">Merge entries</Label>
          </div>
        </TooltipTrigger>
        <TooltipContent>
          <p className="text-sm">
            Merges entries from the same day with matching project, activity,
            and note.
          </p>
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
};
