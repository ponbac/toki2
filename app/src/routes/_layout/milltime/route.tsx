import React from "react";
import { milltimeMutations } from "@/lib/api/mutations/milltime";
import { createFileRoute } from "@tanstack/react-router";
import { milltimeQueries } from "@/lib/api/queries/milltime";
import { useQuery } from "@tanstack/react-query";
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

export const Route = createFileRoute("/_layout/milltime")({
  component: MilltimeComponent,
});

function MilltimeComponent() {
  const [isAuthenticated, setIsAuthenticated] = React.useState(() =>
    isMilltimeCookiesPresent(),
  );

  const { mutate: authenticate, isPending: isAuthenticating } =
    milltimeMutations.useAuthenticate({
      onSuccess: () => {
        setIsAuthenticated(true);
      },
      onError: () => {
        setIsAuthenticated(false);
        clearMilltimeCookies();
      },
    });

  const { data: projects } = useQuery({
    ...milltimeQueries.listProjects(),
    // enabled: isAuthenticated,
    enabled: false,
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
          <li key={project.id}>{project.project_name}</li>
        ))}
      </ul>
    </div>
  );
}

function isMilltimeCookiesPresent() {
  return document.cookie.includes("mt_user");
}

function clearMilltimeCookies() {
  document.cookie = "mt_user=; expires=Thu, 01 Jan 1970 00:00:00 UTC; path=/;";
  document.cookie =
    "mt_password=; expires=Thu, 01 Jan 1970 00:00:00 UTC; path=/;";
}
