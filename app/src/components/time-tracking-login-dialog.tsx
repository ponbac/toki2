import { LogInIcon } from "lucide-react";
import { Button } from "./ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./ui/dialog";
import React from "react";
import { Input } from "./ui/input";
import { Label } from "./ui/label";
import { toast } from "sonner";
import { useTimeTrackingActions, useTimeTrackingIsAuthenticating } from "@/hooks/useTimeTrackingStore";

export const TimeTrackingLoginDialog = (props: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) => {
  const { authenticate } = useTimeTrackingActions();
  const isAuthenticating = useTimeTrackingIsAuthenticating();

  const onSubmit = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    const formData = new FormData(e.target as HTMLFormElement);
    const username = formData.get("username") as string;
    const password = formData.get("password") as string;

    authenticate(
      {
        username,
        password,
      },
      () => {
        props.onOpenChange(false);
        toast.success(
          "Authenticated with Milltime, you can now access all Milltime features.",
        );
      },
    );
  };

  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent>
        <form className="flex flex-col gap-4" onSubmit={onSubmit}>
          <DialogHeader>
            <DialogTitle className="flex flex-row items-center gap-2">
              Sign in to Milltime
            </DialogTitle>
            <DialogDescription>
              You need to authenticate with Milltime to continue.
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-4">
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
              <Input id="password" name="password" type="password" required />
            </div>
          </div>
          <DialogFooter>
            <Button
              type="submit"
              variant="default"
              size="sm"
              className="flex gap-2"
              disabled={isAuthenticating}
            >
              <LogInIcon className="size-5" />
              Sign in
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
};
