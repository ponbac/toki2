import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { Bug, SearchX, TriangleAlert } from "lucide-react";
import { useState } from "react";
import { z } from "zod";

const errorTestSearchSchema = z.object({
  loaderError: z.boolean().optional().catch(false),
});

export const Route = createFileRoute("/_layout/error-test")({
  validateSearch: errorTestSearchSchema,
  beforeLoad: ({ search }) => {
    if (search.loaderError) {
      throw new Error("Temporary test loader error from /error-test");
    }
  },
  component: ErrorTestPage,
});

function ErrorTestPage() {
  const navigate = useNavigate({ from: Route.fullPath });
  const [renderError, setRenderError] = useState(false);

  if (renderError) {
    throw new Error("Temporary test render error from /error-test");
  }

  return (
    <main className="flex w-full items-center justify-center p-4 md:p-8">
      <Card className="card-elevated w-full max-w-3xl">
        <CardHeader>
          <CardTitle>Error Screen Test Route</CardTitle>
          <p className="text-sm text-muted-foreground">
            Temporary route to validate the global error boundary and not-found
            screen. Remove after manual QA.
          </p>
        </CardHeader>
        <CardContent className="grid gap-3 sm:grid-cols-2">
          <Button
            onClick={() =>
              navigate({
                search: () => ({ loaderError: true }),
              })
            }
            className="justify-start gap-2"
          >
            <TriangleAlert className="size-4" />
            Trigger loader error
          </Button>

          <Button
            variant="outline"
            onClick={() => setRenderError(true)}
            className="justify-start gap-2"
          >
            <Bug className="size-4" />
            Trigger render error
          </Button>

          <Button
            variant="secondary"
            className="justify-start gap-2"
            onClick={() => window.location.assign("/__error-screen-404-test__")}
          >
            <SearchX className="size-4" />
            Trigger 404 screen
          </Button>

          <Button
            variant="ghost"
            className="justify-start"
            onClick={() =>
              navigate({
                search: () => ({ loaderError: false }),
              })
            }
          >
            Reset test route state
          </Button>
        </CardContent>
      </Card>
    </main>
  );
}
