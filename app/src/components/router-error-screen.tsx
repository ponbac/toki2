import { useQueryErrorResetBoundary } from "@tanstack/react-query"
import { Link, useRouter } from "@tanstack/react-router"
import { Home, RefreshCw, SearchX, TriangleAlert } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"

function formatError(error: unknown) {
  if (error instanceof Error) {
    return error.stack ?? error.message
  }

  if (typeof error === "string") {
    return error
  }

  try {
    return JSON.stringify(error, null, 2)
  } catch {
    return "Unknown error"
  }
}

function ScreenShell(props: {
  icon: React.ReactNode
  title: string
  description: string
  children?: React.ReactNode
  actions: React.ReactNode
}) {
  return (
    <main className="flex min-h-[75vh] w-full items-center justify-center p-6 md:p-10">
      <Card className="card-elevated w-full max-w-2xl">
        <CardHeader className="space-y-4">
          <div className="flex items-center gap-3">
            <div className="rounded-md bg-primary/10 p-2 text-primary">
              {props.icon}
            </div>
            <div>
              <CardTitle>{props.title}</CardTitle>
              <CardDescription className="mt-1">
                {props.description}
              </CardDescription>
            </div>
          </div>
        </CardHeader>
        {props.children ? <CardContent>{props.children}</CardContent> : null}
        <CardFooter className="flex-wrap justify-center gap-3">
          {props.actions}
        </CardFooter>
      </Card>
    </main>
  )
}

export function RouterErrorScreen(props: { error: Error; reset: () => void }) {
  const router = useRouter()
  const { reset } = useQueryErrorResetBoundary()
  const errorDetails = formatError(props.error)

  const handleRetry = () => {
    reset()
    props.reset()
    void router.invalidate()
  }

  return (
    <ScreenShell
      icon={<TriangleAlert className="size-5" />}
      title="Something went wrong"
      description="The page failed to load correctly. You can retry or go back home."
      actions={
        <>
          <Button onClick={handleRetry} className="btn-glow h-10 min-w-40">
            <RefreshCw className="size-4" />
            Try again
          </Button>
          <Button asChild variant="outline" className="h-10 min-w-40">
            <Link to="/">
              <Home className="size-4" />
              Go back home
            </Link>
          </Button>
        </>
      }
    >
      <div className="space-y-2">
        <p className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
          Error details
        </p>
        <pre className="max-h-56 overflow-auto rounded-md border bg-muted/40 p-3 text-xs leading-relaxed text-destructive">
          {errorDetails}
        </pre>
      </div>
    </ScreenShell>
  )
}

export function RouterNotFoundScreen(props: { data: unknown }) {
  void props.data

  return (
    <ScreenShell
      icon={<SearchX className="size-5" />}
      title="Page not found"
      description="This route does not exist or is no longer available."
      actions={
        <Button asChild className="btn-glow h-10 min-w-40">
          <Link to="/">
            <Home className="size-4" />
            Go back home
          </Link>
        </Button>
      }
    />
  )
}
