import { Button } from "@/components/ui/button";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
} from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { cn } from "@/lib/utils";
import { LogOut, Settings, User } from "lucide-react";

export function Terms() {
  return (
    <div className="flex w-full justify-center">
      <Card>
        <CardHeader>
          <CardTitle>Your terms</CardTitle>
          <CardDescription>
            View and edit your terms here. You can also access terms from other
            users within your user group.
          </CardDescription>
        </CardHeader>
        <CardContent className="">
          <Separator className="mb-6" />
          <ScrollArea>
            <ul className="flex flex-col gap-2">
              <TermListItem
                name="Term 1"
                description="This is a description of term 1"
              />
              <TermListItem
                name="Term 2"
                description="This is a description of term 2"
              />
              <TermListItem
                name="Term 3"
                description="This is a description of term 3"
              />
            </ul>
          </ScrollArea>
        </CardContent>
      </Card>
    </div>
  );
}

function TermListItem(props: {
  name: string;
  description: string;
  className?: string;
}) {
  return (
    <li className={cn("flex flex-row items-center gap-4", props.className)}>
      <div className="flex flex-col">
        <h3 className="text-lg font-semibold">{props.name}</h3>
        <p className="text-sm text-gray-500">{props.description}</p>
      </div>
      <div className="ml-auto flex flex-row gap-4">
        <Button variant="ghost" size="icon">
          <User className="h-6 w-6" />
        </Button>
        <Button variant="ghost" size="icon">
          <Settings className="h-6 w-6" />
        </Button>
        <Button variant="ghost" size="icon">
          <LogOut className="h-6 w-6" />
        </Button>
      </div>
    </li>
  );
}
