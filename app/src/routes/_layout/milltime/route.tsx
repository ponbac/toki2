import { createFileRoute } from "@tanstack/react-router";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  useMilltimeActions,
  useMilltimeIsAuthenticating,
} from "@/hooks/useMilltimeContext";
import { useMilltimeData } from "@/hooks/useMilltimeData";
import React from "react";

export const Route = createFileRoute("/_layout/milltime")({
  component: MilltimeComponent,
});

function MilltimeComponent() {
  const { authenticate, setNewTimerDialogOpen } = useMilltimeActions();
  const isAuthenticating = useMilltimeIsAuthenticating();

  const [activeProjectId, setActiveProjectId] = React.useState<string>();

  const { projects, activities, isAuthenticated } = useMilltimeData({
    projectId: activeProjectId,
  });

  return (
    <div>
      <h1>Milltime</h1>
      {!isAuthenticated && (
        <form
          onSubmit={(e) => {
            e.preventDefault();
            const formData = new FormData(e.target as HTMLFormElement);
            const username = formData.get("username") as string;
            const password = formData.get("password") as string;

            authenticate({
              username,
              password,
            });
          }}
        >
          <Card className="mx-auto max-w-sm">
            <CardHeader>
              <CardTitle className="text-xl">Authenticate</CardTitle>
              <CardDescription>
                Allow Toki to access your Milltime account.
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="grid gap-4">
                <div className="grid gap-2">
                  <Label htmlFor="username">Username</Label>
                  <Input
                    id="username"
                    name="username"
                    type="text"
                    placeholder="pbac"
                    required
                  />
                </div>
                <div className="grid gap-2">
                  <Label htmlFor="password">Password</Label>
                  <Input id="password" name="password" type="password" />
                </div>
                <Button
                  type="submit"
                  className="w-full"
                  disabled={isAuthenticating}
                >
                  Authenticate
                </Button>
              </div>
            </CardContent>
          </Card>
        </form>
      )}
      <ul>
        {projects?.map((project) => (
          <li
            key={project.id}
            onClick={() => setActiveProjectId(project.projectId)}
          >
            {project.projectName}
          </li>
        ))}
        {activities?.map((activity) => (
          <li key={activity.projectId}>{activity.activityName}</li>
        ))}
      </ul>
      <Button onClick={() => setNewTimerDialogOpen(true)}>New Timer</Button>
    </div>
  );
}
