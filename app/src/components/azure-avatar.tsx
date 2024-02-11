import { User } from "@/lib/api/queries/pullRequests";
import { Avatar, AvatarFallback, AvatarImage } from "./ui/avatar";
import { cn } from "@/lib/utils";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";

export function AzureAvatar({
  user,
  className,
}: {
  user: User;
  className?: string;
}) {
  return (
    <Tooltip>
      <TooltipTrigger>
        <Avatar className={cn("size-6", className)}>
          <AvatarImage src={user.avatarUrl} alt={user.displayName} />
          <AvatarFallback>{user.displayName[0].toUpperCase()}</AvatarFallback>
        </Avatar>
      </TooltipTrigger>
      <TooltipContent>
        <div className="text-sm font-medium">{user.displayName}</div>
      </TooltipContent>
    </Tooltip>
  );
}
