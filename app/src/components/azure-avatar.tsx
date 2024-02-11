import { User } from "@/lib/api/queries/pullRequests";
import { Avatar, AvatarFallback, AvatarImage } from "./ui/avatar";
import { cn } from "@/lib/utils";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";

export function AzureAvatar({
  user,
  disableTooltip,
  className,
}: {
  user: User;
  disableTooltip?: boolean;
  className?: string;
}) {
  return disableTooltip ? (
    <AvatarComponent user={user} className={className} />
  ) : (
    <Tooltip>
      <TooltipTrigger>
        <AvatarComponent user={user} className={className} />
      </TooltipTrigger>
      <TooltipContent>
        <div className="text-sm font-medium">{user.displayName}</div>
      </TooltipContent>
    </Tooltip>
  );
}

function AvatarComponent(props: { user: User; className?: string }) {
  return (
    <Avatar className={cn("size-6", props.className)}>
      <AvatarImage src={props.user.avatarUrl} alt={props.user.displayName} />
      <AvatarFallback>{props.user.displayName[0].toUpperCase()}</AvatarFallback>
    </Avatar>
  );
}
