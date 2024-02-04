import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { mutations } from "@/lib/api/mutations/mutations";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import {
  AddRepositoryBody,
  addRepositorySchema,
} from "@/lib/api/mutations/repositories";
import { Button } from "@/components/ui/button";
import {
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from "@/components/ui/form";
import { Input } from "@/components/ui/input";
import { Plus } from "lucide-react";
import { toast } from "sonner";

export const Route = createFileRoute("/_layout/repositories/add")({
  component: AddRepositoryComponent,
});

function AddRepositoryComponent() {
  const navigate = useNavigate({ from: Route.fullPath });

  const { mutate: addRepository, isPending: isAdding } =
    mutations.useAddRepository({
      onSuccess: () => {
        navigate({ to: ".." });
        toast.success("Repository added successfully.");
      },
      onError: () => {
        toast.error(
          "Could not add repository. Make sure your inputs are correct.",
        );
      },
    });

  const form = useForm<AddRepositoryBody>({
    resolver: zodResolver(addRepositorySchema),
    defaultValues: {
      organization: "",
      project: "",
      repoName: "",
      token: "",
    },
  });

  function onSubmit(values: AddRepositoryBody) {
    addRepository(values);
  }

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) {
          navigate({ to: ".." });
        }
      }}
    >
      <DialogContent>
        <Form {...form}>
          <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-8">
            <DialogHeader>
              <DialogTitle>Add new repository</DialogTitle>
              <DialogDescription className="text-balance">
                You can find the required information by inspecting your DevOps
                URL:{" "}
                <code>
                  dev.azure.com/[organization]/[project]/_git/[repository]
                </code>
              </DialogDescription>
            </DialogHeader>
            <div className="flex flex-col gap-2">
              <FormField
                control={form.control}
                name="organization"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Organization</FormLabel>
                    <FormControl>
                      <Input placeholder="Game Freak" {...field} />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />
              <FormField
                control={form.control}
                name="project"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Project</FormLabel>
                    <FormControl>
                      <Input placeholder="Arceus" {...field} />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />
              <FormField
                control={form.control}
                name="repoName"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Repository</FormLabel>
                    <FormControl>
                      <Input placeholder="rusty_api" {...field} />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />
              <FormField
                control={form.control}
                name="token"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Token</FormLabel>
                    <FormControl>
                      <Input
                        placeholder="uxl2cp6blpkljpajfsn5vr3afeecg5z9w4vz5f2bruwfiago52ak"
                        {...field}
                      />
                    </FormControl>
                    <FormDescription>
                      PAT with proper permissions to access the repository.
                    </FormDescription>
                    <FormMessage />
                  </FormItem>
                )}
              />
            </div>
            <DialogFooter>
              <Button
                type="submit"
                size="sm"
                className="flex items-center gap-1.5 transition-colors"
                disabled={isAdding}
              >
                <Plus size="1.25rem" />
                Add repository
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  );
}
