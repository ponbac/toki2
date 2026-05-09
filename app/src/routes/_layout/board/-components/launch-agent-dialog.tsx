import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import type { Differ } from "@/lib/api/queries/differs";
import type { BoardWorkItem } from "@/lib/api/queries/workItems";
import { Bot, Loader2 } from "lucide-react";
import { useMemo, useState } from "react";

export type AgentLaunchTargetRepo = {
  provider: "azureDevOps";
  organization: string;
  project: string;
  repoName: string;
  defaultBranch: string;
};

export function LaunchAgentDialog({
  item,
  organization,
  project,
  repositories,
  open,
  isLaunching,
  onOpenChange,
  onLaunch,
}: {
  item: BoardWorkItem | null;
  organization: string;
  project: string;
  repositories: Differ[];
  open: boolean;
  isLaunching: boolean;
  onOpenChange: (open: boolean) => void;
  onLaunch: (payload: {
    targetRepo: AgentLaunchTargetRepo;
    prompt?: string;
  }) => void;
}) {
  const targetRepos = useMemo(
    () =>
      repositories
        .filter(
          (repo) =>
            repo.followed &&
            repo.organization === organization &&
            repo.project === project &&
            !repo.isInvalid,
        )
        .sort((a, b) => a.repoName.localeCompare(b.repoName)),
    [organization, project, repositories],
  );
  const [selectedRepoName, setSelectedRepoName] = useState("");
  const [prompt, setPrompt] = useState("");
  const effectiveSelectedRepoName =
    selectedRepoName || targetRepos[0]?.repoName || "";
  const selectedRepo = targetRepos.find(
    (repo) => repo.repoName === effectiveSelectedRepoName,
  );
  const canLaunch = item !== null && selectedRepo !== undefined && !isLaunching;

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          setPrompt("");
          setSelectedRepoName("");
        }
        onOpenChange(nextOpen);
      }}
    >
      <DialogContent className="max-w-xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Bot className="h-5 w-5" />
            Launch agent
          </DialogTitle>
          <DialogDescription>
            Plan first mode only. The agent will create a plan before changing
            files.
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-4">
          <div className="rounded-md border border-border/60 bg-muted/30 p-3">
            <p className="text-xs font-medium text-muted-foreground">
              Work item
            </p>
            <p className="mt-1 text-sm font-medium leading-snug">
              {item ? `${item.id}: ${item.title}` : "No work item selected"}
            </p>
          </div>

          <div className="grid gap-2">
            <Label htmlFor="agent-target-repo">Target repository</Label>
            <Select
              value={effectiveSelectedRepoName}
              onValueChange={setSelectedRepoName}
              disabled={isLaunching || targetRepos.length === 0}
            >
              <SelectTrigger id="agent-target-repo">
                <SelectValue placeholder="Select a followed repository" />
              </SelectTrigger>
              <SelectContent>
                {targetRepos.map((repo) => (
                  <SelectItem key={repo.repoName} value={repo.repoName}>
                    {repo.repoName}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {targetRepos.length === 0 && (
              <p className="text-xs text-muted-foreground">
                Follow a repository in this project before launching an agent.
              </p>
            )}
          </div>

          <div className="grid gap-2">
            <Label htmlFor="agent-extra-instructions">Extra instruction</Label>
            <Textarea
              id="agent-extra-instructions"
              value={prompt}
              onChange={(event) => setPrompt(event.target.value)}
              placeholder="Optional constraints, preferences, or context."
              className="min-h-28"
              disabled={isLaunching}
            />
          </div>
        </div>

        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={isLaunching}
          >
            Cancel
          </Button>
          <Button
            type="button"
            disabled={!canLaunch}
            onClick={() => {
              if (!selectedRepo) {
                return;
              }

              setPrompt("");
              onLaunch({
                targetRepo: {
                  provider: "azureDevOps",
                  organization: selectedRepo.organization,
                  project: selectedRepo.project,
                  repoName: selectedRepo.repoName,
                  defaultBranch: "main",
                },
                prompt: prompt.trim() || undefined,
              });
            }}
          >
            {isLaunching ? (
              <Loader2 className="mr-2 h-4 w-4 animate-spin" />
            ) : (
              <Bot className="mr-2 h-4 w-4" />
            )}
            Start plan
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
