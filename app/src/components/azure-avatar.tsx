import { User } from "@/lib/api/queries/pullRequests";
import { Avatar, AvatarFallback, AvatarImage } from "./ui/avatar";
import { cn } from "@/lib/utils";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";

export function AzureAvatar({
  user,
  disableTooltip,
  className,
  overrideAvatarUrl,
}: {
  user: User;
  disableTooltip?: boolean;
  className?: string;
  overrideAvatarUrl?: string;
}) {
  return disableTooltip ? (
    <AvatarComponent
      user={user}
      className={className}
      overrideAvatarUrl={overrideAvatarUrl}
    />
  ) : (
    <Tooltip>
      <TooltipTrigger>
        <AvatarComponent
          user={user}
          className={className}
          overrideAvatarUrl={overrideAvatarUrl}
        />
      </TooltipTrigger>
      <TooltipContent>
        <div className="text-sm font-medium">{user.displayName}</div>
      </TooltipContent>
    </Tooltip>
  );
}

function AvatarComponent(props: {
  user: User;
  className?: string;
  overrideAvatarUrl?: string;
}) {
  const fallbackSrc =
    props.overrideAvatarUrl ?? props.user.avatarUrl ?? undefined;

  return (
    <Avatar className={cn("size-6", props.className)}>
      <AvatarImage src={fallbackSrc} alt={props.user.displayName} />
      <AvatarFallback>{props.user.displayName[0].toUpperCase()}</AvatarFallback>
    </Avatar>
  );
}
