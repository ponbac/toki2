import * as React from "react";
import { useQuery } from "@tanstack/react-query";

import { User } from "@/lib/api/queries/pullRequests";
import { Avatar, AvatarFallback, AvatarImage } from "./ui/avatar";
import { cn } from "@/lib/utils";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";
import { userQueries } from "@/lib/api/queries/user";
import { API_URL } from "@/lib/api/api";

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
  const { data: me } = useQuery({
    ...userQueries.me(),
    staleTime: Infinity,
  });

  const isCurrentUser =
    me && me.email.toLowerCase() === props.user.uniqueName.toLowerCase();

  const [version, setVersion] = React.useState(0);

  React.useEffect(() => {
    setVersion((v) => v + 1);
  }, [me?.hasAvatar]);

  let src: string | undefined = props.user.avatarUrl;

  if (isCurrentUser) {
    if (me?.hasAvatar) {
      src = `${API_URL}/me/avatar?v=${version}`;
    } else if (me?.picture) {
      src = me.picture;
    }
  }

  return (
    <Avatar className={cn("size-6", props.className)}>
      {src && <AvatarImage src={src} alt={props.user.displayName} />}
      <AvatarFallback>{props.user.displayName[0].toUpperCase()}</AvatarFallback>
    </Avatar>
  );
}
