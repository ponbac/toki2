import * as React from "react";
import { User } from "@/lib/api/queries/pullRequests";
import { Avatar, AvatarFallback, AvatarImage } from "./ui/avatar";
import { cn } from "@/lib/utils";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";
import { useAtomValue } from "jotai/react";
import { enablePokemonAvatarFallbackAtom } from "@/lib/avatar-preferences";
import { useAvatarSourceWithFallback } from "@/hooks/useAvatarSourceWithFallback";
import { useTheme } from "@/hooks/useTheme";
import {
  avatarFallbackTextColorFromSeed,
  avatarRingColorFromSeed,
  buildAvatarSources,
  pokemonAvatarUrlFromSeed,
} from "@/lib/avatar";

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

function AvatarComponent(props: {
  user: User;
  className?: string;
}) {
  const { resolvedTheme } = useTheme();
  const enablePokemonAvatarFallback = useAtomValue(
    enablePokemonAvatarFallbackAtom,
  );
  const pokemonAvatarSrc = React.useMemo(
    () =>
      enablePokemonAvatarFallback
        ? pokemonAvatarUrlFromSeed(props.user.uniqueName)
        : undefined,
    [enablePokemonAvatarFallback, props.user.uniqueName],
  );

  const avatarSources = React.useMemo(
    () => buildAvatarSources({
      preferredSources: [props.user.avatarUrl],
      pokemonSeed: props.user.uniqueName,
      enablePokemonFallback: enablePokemonAvatarFallback,
    }),
    [
      props.user.avatarUrl,
      props.user.uniqueName,
      enablePokemonAvatarFallback,
    ],
  );
  const { avatarSrc, onLoadingStatusChange, failedSources } =
    useAvatarSourceWithFallback(avatarSources);

  const remainingSourceCount = React.useMemo(
    () => avatarSources.filter((source) => !failedSources.has(source)).length,
    [avatarSources, failedSources],
  );
  const isLetterFallbackActive =
    avatarSources.length === 0 || remainingSourceCount === 0;
  const isPokemonFallbackActive =
    !!pokemonAvatarSrc &&
    avatarSrc === pokemonAvatarSrc &&
    !failedSources.has(pokemonAvatarSrc);
  const pokemonRingColor = React.useMemo(
    () => avatarRingColorFromSeed(props.user.uniqueName, resolvedTheme),
    [props.user.uniqueName, resolvedTheme],
  );
  const fallbackInitialColor = React.useMemo(
    () => avatarFallbackTextColorFromSeed(props.user.uniqueName, resolvedTheme),
    [props.user.uniqueName, resolvedTheme],
  );
  const fallbackInitial = props.user.displayName?.[0]?.toUpperCase() ?? "?";

  return (
    <Avatar
      className={cn("size-[26px]", props.className)}
      style={
        isPokemonFallbackActive
          ? { boxShadow: `inset 0 0 0 2px ${pokemonRingColor}` }
          : undefined
      }
    >
      <AvatarImage
        src={avatarSrc}
        alt={props.user.displayName}
        onLoadingStatusChange={onLoadingStatusChange}
      />
      <AvatarFallback
        style={isLetterFallbackActive ? { color: fallbackInitialColor } : undefined}
      >
        {fallbackInitial}
      </AvatarFallback>
    </Avatar>
  );
}
