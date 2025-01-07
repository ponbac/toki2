import { createFileRoute } from "@tanstack/react-router";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { LogIn } from "lucide-react";
import { api } from "@/lib/api/api";
import { useMutation } from "@tanstack/react-query";

type LoginSearchParams = {
  next?: string;
};

export const Route = createFileRoute("/login")({
  component: LoginComponent,
  validateSearch: (search: Record<string, unknown>): LoginSearchParams => {
    return {
      next: (search.next as string) || undefined,
    };
  },
});

function LoginComponent() {
  const { next } = Route.useSearch();

  const { mutate: login, isPending } = useMutation({
    mutationFn: (next?: string) =>
      api.post("login", { searchParams: next ? { next } : undefined }).text(),
    onSuccess: (authUrl) => {
      // eslint-disable-next-line react-compiler/react-compiler
      window.location.href = authUrl;
    },
  });

  return (
    <main className="flex h-screen items-center justify-center">
      <Card className="min-w-[25rem] max-w-sm">
        <CardHeader>
          <CardTitle>Sign in</CardTitle>
          <CardDescription>
            Only Azure AD is currently supported.
          </CardDescription>
        </CardHeader>
        <CardFooter>
          <Button
            className="w-full"
            onClick={() => login(next)}
            disabled={isPending}
          >
            <LogIn className="mr-2 h-4 w-4" />
            Sign in with Azure
          </Button>
        </CardFooter>
      </Card>
    </main>
  );
}
