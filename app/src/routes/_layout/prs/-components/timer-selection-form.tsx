import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Activity, ProjectSearchItem } from "@/lib/api/queries/milltime";

interface TimerSelectionFormProps {
  projects: ProjectSearchItem[];
  selectedProject: string | null;
  setSelectedProject: (value: string) => void;
  selectedActivity: string | null;
  setSelectedActivity: (value: string) => void;
  activities: Activity[] | undefined;
  onStartTimer: () => void;
}

export function TimerSelectionForm({
  projects,
  selectedProject,
  setSelectedProject,
  selectedActivity,
  setSelectedActivity,
  activities,
  onStartTimer,
}: TimerSelectionFormProps) {
  if (!projects.length) {
    return (
      <div className="flex flex-col gap-4">
        <p className="text-sm text-muted-foreground">
          No Milltime projects connected to this repository. Connect projects in the Repositories view.
        </p>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-2">
        <h4 className="font-medium">Select Project</h4>
        <Select
          value={selectedProject ?? undefined}
          onValueChange={(value) => {
            setSelectedProject(value);
            setSelectedActivity("");
          }}
        >
          <SelectTrigger>
            <SelectValue placeholder="Select a project" />
          </SelectTrigger>
          <SelectContent>
            {projects.map((project) => (
              <SelectItem key={project.projectId} value={project.projectId}>
                {project.projectName}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      {selectedProject && (
        <div className="flex flex-col gap-2">
          <h4 className="font-medium">Select Activity</h4>
          <Select
            value={selectedActivity ?? undefined}
            onValueChange={setSelectedActivity}
          >
            <SelectTrigger>
              <SelectValue placeholder="Select an activity" />
            </SelectTrigger>
            <SelectContent>
              {activities?.map((activity) => (
                <SelectItem key={activity.activity} value={activity.activity}>
                  {activity.activityName}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      )}
      <Button
        disabled={!selectedProject || !selectedActivity}
        onClick={onStartTimer}
      >
        Start Timer
      </Button>
    </div>
  )
}