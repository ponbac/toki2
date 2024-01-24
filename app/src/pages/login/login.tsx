import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { LogIn } from "lucide-react";

export function Login() {
  return (
    <main className="flex h-screen items-center justify-center">
      <Card className="max-w-sm">
        <CardHeader>
          <CardTitle>Sign in</CardTitle>
          <CardDescription>
            Use the credentials provided to you by your institution's
            administator.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form>
            <div className="grid w-full items-center gap-4">
              <div className="flex flex-col space-y-1.5">
                <Label htmlFor="username">Username</Label>
                <Input id="username" placeholder="smoothie-slurper" />
              </div>
              <div className="flex flex-col space-y-1.5">
                <Label htmlFor="password">Password</Label>
                <Input
                  id="password"
                  type="password"
                  placeholder="secret_xyz_c4t"
                />
              </div>
            </div>
          </form>
        </CardContent>
        <CardFooter className="flex-row-reverse">
          <Button
            onClick={() => {
              localStorage.setItem("isAuthenticated", "true");
              window.location.reload();
            }}
          >
            <LogIn className="mr-2 h-4 w-4" />
            Sign in
          </Button>
        </CardFooter>
      </Card>
    </main>
  );
}
