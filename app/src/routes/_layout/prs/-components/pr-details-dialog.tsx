import { AzureAvatar } from "@/components/azure-avatar";
import BranchLink from "@/components/branch-link";
import { PRLink } from "@/components/pr-link";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { ListPullRequest } from "@/lib/api/queries/pullRequests";
import { queries } from "@/lib/api/queries/queries";
import { mutations } from "@/lib/api/mutations/mutations";
import { useSuspenseQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import dayjs from "dayjs";
import {
  ClipboardCopy,
  CodeXmlIcon,
  MessageCircleCodeIcon,
} from "lucide-react";
import { toast } from "sonner";
import React from "react";
import { PRNotificationSettings } from "./pr-notification-settings";
import { TimerSelectionForm } from "./timer-selection-form";

interface PRDetailsDialogProps {
  pr: ListPullRequest;
  prId: string;
  parentSearch: Record<string, any>;
}

export function PRDetailsDialog({ pr, prId, parentSearch }: PRDetailsDialogProps) {
  const navigate = useNavigate();

  const { data: differs } = useSuspenseQuery(queries.differs());
  const { data: allProjects } = useSuspenseQuery(queries.listProjects());
  const [selectedProject, setSelectedProject] = React.useState<string | null>(null);
  const [selectedActivity, setSelectedActivity] = React.useState<string | null>(null);
  const [timeReportMode, setTimeReportMode] = React.useState<"review" | "develop" | null>(null);

  const differ = differs.find(d => d.repoName === pr?.repoName);
  const connectedProjects = allProjects.filter(p => differ?.milltimeProjectIds.includes(p.projectId));

  const { data: activities } = useSuspenseQuery({
    ...queries.listActivities(selectedProject ?? ""),
    enabled: !!selectedProject,
  });

  const { mutate: startTimer } = mutations.useStartTimer({
    onSuccess: () => {
      toast.success("Timer started successfully");
      setSelectedProject(null);
      setSelectedActivity(null);
      setTimeReportMode(null);
    },
  });

  const getTimeReportText = (mode: "review" | "develop") => {
    let text = "";
    const workItem = pr?.workItems.at(0);
    if (!workItem) {
      text = `!${prId} - ${mode === "review" ? "[CR] " : ""}${pr?.title}`;
    } else {
      const parentWorkItem = workItem.parentId;
      text = `${parentWorkItem ? `#${parentWorkItem} ` : ""}#${workItem.id} - ${mode === "review" ? "[CR] " : ""}${workItem.title}`;
    }
    return text;
  };

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
    toast.info(
      <div className="flex flex-row items-center">
        <ClipboardCopy className="mr-2 inline-block" size="1.25rem" />
        <p className="text-pretty">
          Copied <span className="font-mono">{text}</span> to clipboard
        </p>
      </div>,
    );
  };

  const startTimeReport = () => {
    if (!timeReportMode || !selectedProject || !selectedActivity) return;

    const text = getTimeReportText(timeReportMode);
    copyToClipboard(text);

    const project = allProjects.find(p => p.projectId === selectedProject);
    if (!project) {
      toast.error("Project not found");
      return;
    }

    const activity = activities?.find(a => a.activity === selectedActivity);
    if (!activity) {
      toast.error("Activity not found");
      return;
    }

    startTimer({
      projectId: project.projectId,
      projectName: project.projectName,
      activity: activity.activity,
      activityName: activity.activityName,
      userNote: text,
      regDay: dayjs().format("YYYY-MM-DD"),
      weekNumber: dayjs().week(),
    });
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) {
          navigate({ to: "..", search: parentSearch });
        }
      }}
    >
      <DialogContent className="max-w-5xl">
        <DialogHeader>
          <DialogTitle className="flex flex-row items-center gap-2">
            <AzureAvatar user={pr.createdBy} className="size-8" />
            <PRLink data={pr}>
              <h1 className="text-xl font-semibold">{pr.title}</h1>
            </PRLink>
          </DialogTitle>
          <DialogDescription>
            <BranchLink sourceBranch={pr.sourceBranch} targetBranch={pr.targetBranch} />
          </DialogDescription>
        </DialogHeader>
        <DialogFooter className="pt-2">
          <PRNotificationSettings pullRequest={pr} />
          <Popover open={timeReportMode === "review"} onOpenChange={(open) => {
            if (open) {
              setTimeReportMode("review");
            } else {
              setTimeReportMode(null);
              setSelectedProject(null);
              setSelectedActivity(null);
            }
          }}>
            <PopoverTrigger asChild>
              <Button
                autoFocus
                variant="outline"
                size="sm"
                className="flex gap-2"
              >
                <MessageCircleCodeIcon className="size-4" />
                Review
              </Button>
            </PopoverTrigger>
            <PopoverContent className="w-80">
              <TimerSelectionForm
                projects={connectedProjects}
                selectedProject={selectedProject}
                setSelectedProject={setSelectedProject}
                selectedActivity={selectedActivity}
                setSelectedActivity={setSelectedActivity}
                activities={activities}
                onStartTimer={startTimeReport}
              />
            </PopoverContent>
          </Popover>
          <Popover open={timeReportMode === "develop"} onOpenChange={(open) => {
            if (open) {
              setTimeReportMode("develop");
            } else {
              setTimeReportMode(null);
              setSelectedProject(null);
              setSelectedActivity(null);
            }
          }}>
            <PopoverTrigger asChild>
              <Button
                autoFocus
                variant="default"
                size="sm"
                className="flex gap-2"
              >
                <CodeXmlIcon className="size-4" />
                Develop
              </Button>
            </PopoverTrigger>
            <PopoverContent className="w-80">
              <TimerSelectionForm
                projects={connectedProjects}
                selectedProject={selectedProject}
                setSelectedProject={setSelectedProject}
                selectedActivity={selectedActivity}
                setSelectedActivity={setSelectedActivity}
                activities={activities}
                onStartTimer={startTimeReport}
              />
            </PopoverContent>
          </Popover>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}