import { useQuery } from "@tanstack/react-query";
import { useAtom } from "jotai/react";
import { LockIcon, PlusIcon, RotateCcwIcon, ShuffleIcon } from "lucide-react";
import * as React from "react";
import { apiErrorToast } from "@/lib/api/errors";
import { buddyMutations } from "@/lib/api/mutations/buddy";
import {
  type Buddy,
  type BuddyProfile,
  type BuddyProfileUpdate,
  buddyApiQueries,
} from "@/lib/api/queries/buddy";
import { userQueries } from "@/lib/api/queries/user";
import { formatBuddyXp } from "@/lib/buddy/format";
import { forgetLegacyBuddyChoice } from "@/lib/buddy/legacy-choice";
import { buddyLevelProgress, buddyXpForLevel } from "@/lib/buddy/level";
import {
  type BuddyEnergy,
  timerBuddyEnergyAtom,
} from "@/lib/buddy/preferences";
import { useStartingBuddy } from "@/lib/buddy/profile";
import { BUDDY_CREDITS_URL } from "@/lib/buddy/sheet";
import {
  BUDDY_SPECIES_LIST,
  type BuddySpeciesId,
  buddySpeciesName,
  defaultBuddySpeciesId,
  parseBuddySpeciesId,
  randomBuddySpeciesId,
} from "@/lib/buddy/species";
import { cn } from "@/lib/utils";
import { Combobox } from "../combobox";
import { Button } from "../ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "../ui/dialog";
import { Label } from "../ui/label";
import { Progress } from "../ui/progress";
import { Switch } from "../ui/switch";
import { Tooltip, TooltipContent, TooltipTrigger } from "../ui/tooltip";
import { BuddyPreview } from "./buddy-preview";
import { TodayXp } from "./today-xp";

const NAME_PUNCTUATION = /[^\p{L}\p{N}]+/gu;

const SPECIES_ITEMS = BUDDY_SPECIES_LIST.map((species) => ({
  value: String(species.id),
  label: `${species.name} #${species.id}`,
  // Spellings without the punctuation, so "mr mime" and "farfetchd" still match.
  keywords: `${species.name.replace(NAME_PUNCTUATION, " ")} ${species.name.replace(NAME_PUNCTUATION, "")}`,
}));

const ENERGY_OPTIONS: ReadonlyArray<{ value: BuddyEnergy; label: string }> = [
  { value: "calm", label: "Calm" },
  { value: "lively", label: "Lively" },
];

const CARD_CLASS = "rounded-md border border-border/60 bg-muted/30 px-3 py-2";

/**
 * Settings for the user's timer buddies: on or off, the lead and the partner,
 * energy, today's XP and battle notifications. Controlled, so any page can
 * open it.
 *
 * Pass `trigger` to render the button that opens it, which gets the focus back
 * when it closes. Set `nested` when it opens over another dialog, for a lighter
 * overlay.
 */
export function BuddySettingsDialog({
  open,
  onOpenChange,
  trigger,
  nested = false,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  trigger?: React.ReactNode;
  nested?: boolean;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {trigger && <DialogTrigger asChild>{trigger}</DialogTrigger>}
      <DialogContent
        // The dialog underneath already darkens the page.
        overlayClassName={nested ? "bg-black/40" : undefined}
        className="max-h-[85vh] max-w-md grid-rows-[auto_minmax(0,1fr)_auto] overflow-hidden"
      >
        <DialogHeader>
          <DialogTitle>Timer buddies</DialogTitle>
          <DialogDescription>
            Pokemon that live on your timer, level up as you log hours and
            battle your colleagues&apos; buddies.
          </DialogDescription>
        </DialogHeader>
        <BuddySettingsContent />
      </DialogContent>
    </Dialog>
  );
}

function BuddySettingsContent() {
  const profileQuery = useQuery(buddyApiQueries.profile());

  return (
    <>
      <div className="flex min-h-0 flex-col gap-3 overflow-y-auto py-2 pr-1">
        {profileQuery.data !== undefined ? (
          <BuddySettingsForm profile={profileQuery.data} />
        ) : profileQuery.isError ? (
          <div className="flex items-center justify-between gap-3" role="alert">
            <p className="text-xs text-destructive">
              Could not load your buddies.
            </p>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={async () => {
                await profileQuery.refetch();
              }}
            >
              Retry
            </Button>
          </div>
        ) : (
          <p className="text-xs text-muted-foreground" role="status">
            Loading your buddies...
          </p>
        )}
      </div>

      <DialogFooter className="flex-row items-center justify-end">
        <a
          href={BUDDY_CREDITS_URL}
          target="_blank"
          rel="noreferrer"
          className="text-xs text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
        >
          Sprite credits
        </a>
      </DialogFooter>
    </>
  );
}

/** `profile` is `null` before the user enrols. */
function BuddySettingsForm({ profile }: { profile: BuddyProfile | null }) {
  const id = React.useId();
  const { data: me } = useQuery({ ...userQueries.me(), staleTime: Infinity });
  const update = buddyMutations.useUpdateBuddyProfile({
    onError: apiErrorToast("Failed to update your buddies"),
  });

  // Controls show the change being saved, and stay disabled until it lands.
  const saving = update.isPending;
  const pending: BuddyProfileUpdate = saving ? (update.variables ?? {}) : {};
  const enabled = pending.enabled ?? profile?.enabled ?? false;
  const defaultSpeciesId = me ? defaultBuddySpeciesId(me.email) : null;
  const starting = useStartingBuddy();

  const setEnabled = (next: boolean) => {
    if (profile) {
      update.mutate({ enabled: next });
    } else if (starting.speciesId !== null) {
      update.mutate(
        { enabled: next, leadSpeciesId: starting.speciesId },
        { onSuccess: forgetLegacyBuddyChoice },
      );
    }
  };

  return (
    <>
      <SwitchCard
        id={`${id}-enabled`}
        label="Timer buddies"
        hint={
          profile || starting.speciesId === null
            ? "Show your buddies on the timer and let colleagues battle them."
            : `Show your buddies on the timer and let colleagues battle them. You start with ${buddySpeciesName(starting.speciesId)}, ${starting.picked ? "the Pokemon you picked" : "your avatar's Pokemon"}.`
        }
        checked={enabled}
        disabled={saving || (!profile && starting.speciesId === null)}
        onCheckedChange={setEnabled}
      />

      {profile && enabled && (
        <>
          <LeadCard
            lead={profile.lead}
            speciesId={pending.leadSpeciesId ?? profile.lead.speciesId}
            defaultSpeciesId={defaultSpeciesId}
            disabled={saving}
            onPick={(speciesId) => update.mutate({ leadSpeciesId: speciesId })}
          />
          <PartnerCard
            id={id}
            profile={profile}
            leadSpeciesId={pending.leadSpeciesId ?? profile.lead.speciesId}
            partnerSpeciesId={
              pending.partnerSpeciesId ?? profile.partner?.speciesId ?? null
            }
            visible={pending.partnerVisible ?? profile.partner?.visible ?? true}
            disabled={saving}
            onUpdate={(change) => update.mutate(change)}
          />
          <EnergyCard id={`${id}-energy`} />
          <TodayXp
            today={profile.today}
            syncedAt={profile.hoursSyncedAt}
            twoBuddies={profile.partner !== null}
            className={CARD_CLASS}
          />
          <SwitchCard
            id={`${id}-battle-notifications`}
            label="Battle notifications"
            hint="Get a push notification when a colleague battles your buddies. Needs Toki notifications turned on for this device."
            checked={pending.battleNotifications ?? profile.battleNotifications}
            disabled={saving}
            onCheckedChange={(next) =>
              update.mutate({ battleNotifications: next })
            }
          />
        </>
      )}
    </>
  );
}

function SwitchCard({
  id,
  label,
  hint,
  checked,
  disabled,
  onCheckedChange,
}: {
  id: string;
  label: string;
  hint: string;
  checked: boolean;
  disabled: boolean;
  onCheckedChange: (checked: boolean) => void;
}) {
  return (
    <div className={CARD_CLASS}>
      <div className="flex items-start justify-between gap-3">
        <div className="space-y-0.5">
          <Label htmlFor={id} className="text-sm font-medium text-foreground">
            {label}
          </Label>
          <p className="text-xs text-muted-foreground">{hint}</p>
        </div>
        <Switch
          id={id}
          checked={checked}
          disabled={disabled}
          onCheckedChange={onCheckedChange}
        />
      </div>
    </div>
  );
}

function LeadCard({
  lead,
  speciesId,
  defaultSpeciesId,
  disabled,
  onPick,
}: {
  lead: Buddy;
  speciesId: BuddySpeciesId;
  defaultSpeciesId: BuddySpeciesId | null;
  disabled: boolean;
  onPick: (speciesId: BuddySpeciesId) => void;
}) {
  return (
    <div className={cn(CARD_CLASS, "space-y-3 py-3")}>
      <BuddyLevel role="Lead" buddy={lead} speciesId={speciesId} />
      <SpeciesPicker
        label="Lead Pokemon"
        value={speciesId}
        placeholder="Pokemon"
        disabled={disabled}
        onPick={onPick}
      >
        <IconButton
          label="Back to my default"
          disabled={
            disabled ||
            defaultSpeciesId === null ||
            defaultSpeciesId === speciesId
          }
          onClick={() => defaultSpeciesId !== null && onPick(defaultSpeciesId)}
        >
          <RotateCcwIcon className="size-4" />
        </IconButton>
      </SpeciesPicker>
    </div>
  );
}

function PartnerCard({
  id,
  profile,
  leadSpeciesId,
  partnerSpeciesId,
  visible,
  disabled,
  onUpdate,
}: {
  id: string;
  profile: BuddyProfile;
  leadSpeciesId: BuddySpeciesId;
  /** The partner's species, or the one being picked; `null` before either. */
  partnerSpeciesId: BuddySpeciesId | null;
  visible: boolean;
  disabled: boolean;
  onUpdate: (change: BuddyProfileUpdate) => void;
}) {
  const { lead, partner, partnerUnlockLevel } = profile;
  const leadName = buddySpeciesName(leadSpeciesId);
  const pickPartner = (speciesId: BuddySpeciesId) =>
    onUpdate({ partnerSpeciesId: speciesId });

  if (partner && partnerSpeciesId !== null) {
    const visibleId = `${id}-partner-visible`;
    return (
      <div className={cn(CARD_CLASS, "space-y-3 py-3")}>
        <BuddyLevel
          role="Partner"
          buddy={partner}
          speciesId={partnerSpeciesId}
        />
        <SpeciesPicker
          label="Partner Pokemon"
          value={partnerSpeciesId}
          placeholder="Pokemon"
          disabled={disabled}
          onPick={pickPartner}
        />
        <div className="flex items-start justify-between gap-3 border-t border-border/60 pt-3">
          <div className="space-y-0.5">
            <Label
              htmlFor={visibleId}
              className="text-sm font-medium text-foreground"
            >
              Show on screen
            </Label>
            <p className="text-xs text-muted-foreground">
              A hidden partner still fights in battles.
            </p>
          </div>
          <Switch
            id={visibleId}
            checked={visible}
            disabled={disabled}
            onCheckedChange={(next) => onUpdate({ partnerVisible: next })}
          />
        </div>
      </div>
    );
  }

  if (lead.level < partnerUnlockLevel) {
    const unlockXp = buddyXpForLevel(partnerUnlockLevel);
    const xpToGo = Math.max(0, unlockXp - lead.xp);
    return (
      <div className={cn(CARD_CLASS, "flex items-center gap-3 py-3")}>
        <EmptyTile>
          <LockIcon className="size-5" />
        </EmptyTile>
        <div className="min-w-0 flex-1 space-y-1.5">
          <div className="space-y-0.5">
            <p className="text-sm font-medium">Partner</p>
            <p className="text-xs text-muted-foreground">
              Unlocks when {leadName} reaches Lv {partnerUnlockLevel}
            </p>
          </div>
          <Progress
            value={
              unlockXp > 0 ? Math.min(100, (lead.xp / unlockXp) * 100) : 100
            }
            aria-label={`Progress to Lv ${partnerUnlockLevel}`}
            className="h-1.5"
          />
          <p className="text-xs tabular-nums text-muted-foreground">
            {formatBuddyXp(xpToGo)} to go
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className={cn(CARD_CLASS, "space-y-3 py-3")}>
      <div className="flex items-center gap-3">
        {partnerSpeciesId === null ? (
          <EmptyTile>
            <PlusIcon className="size-5" />
          </EmptyTile>
        ) : (
          <BuddyPreview speciesId={partnerSpeciesId} />
        )}
        <div className="min-w-0 flex-1 space-y-0.5">
          <p className="text-sm font-medium">Partner</p>
          <p className="text-xs text-muted-foreground">
            {leadName} can bring a second buddy. It starts with no XP, then gets
            the same XP as {leadName} and fights next to it.
          </p>
        </div>
      </div>
      <SpeciesPicker
        label="Partner Pokemon"
        value={partnerSpeciesId}
        placeholder="Pick a partner"
        disabled={disabled}
        onPick={pickPartner}
      />
    </div>
  );
}

/** A buddy's tile, name, level and progress toward the next level. */
function BuddyLevel({
  role,
  buddy,
  speciesId,
}: {
  role: string;
  buddy: Buddy;
  /** Shown instead of the buddy's species while a new one is being saved. */
  speciesId: BuddySpeciesId;
}) {
  const name = buddySpeciesName(speciesId);
  const { fraction, xpToNext } = buddyLevelProgress(buddy);

  return (
    <div className="flex items-center gap-3">
      <BuddyPreview speciesId={speciesId} />
      <div className="min-w-0 flex-1 space-y-1.5">
        <div className="flex items-baseline justify-between gap-2">
          <p className="min-w-0 truncate text-sm font-medium">
            {name}{" "}
            <span className="font-normal text-muted-foreground">· {role}</span>
          </p>
          <span className="shrink-0 text-sm font-semibold tabular-nums">
            Lv {buddy.level}
          </span>
        </div>
        <Progress
          value={fraction * 100}
          aria-label={`${name}'s progress to the next level`}
          className="h-1.5"
        />
        <p className="text-xs tabular-nums text-muted-foreground">
          {xpToNext === null
            ? "Max level"
            : `${formatBuddyXp(xpToNext)} to Lv ${buddy.level + 1}`}
        </p>
      </div>
    </div>
  );
}

/**
 * Picks a Pokemon from the list, or a random one; `label` names the list for
 * screen readers, and `children` are extra buttons.
 */
function SpeciesPicker({
  label,
  value,
  placeholder,
  disabled,
  onPick,
  children,
}: {
  label: string;
  value: BuddySpeciesId | null;
  placeholder: string;
  disabled: boolean;
  onPick: (speciesId: BuddySpeciesId) => void;
  children?: React.ReactNode;
}) {
  return (
    <div className="flex items-center gap-1.5">
      <div className="min-w-0 flex-1">
        <Combobox
          ariaLabel={label}
          items={SPECIES_ITEMS}
          placeholder={placeholder}
          searchPlaceholder="Search by name or number..."
          emptyMessage="No Pokemon found"
          value={value === null ? "" : String(value)}
          disabled={disabled}
          onChange={(picked) => {
            // The combobox reports "" when the current item is picked again.
            const speciesId = parseBuddySpeciesId(Number(picked));
            if (picked !== "" && speciesId !== null) {
              onPick(speciesId);
            }
          }}
        />
      </div>
      <IconButton
        label="Random Pokemon"
        disabled={disabled}
        onClick={() => onPick(randomBuddySpeciesId())}
      >
        <ShuffleIcon className="size-4" />
      </IconButton>
      {children}
    </div>
  );
}

function IconButton({
  label,
  disabled,
  onClick,
  children,
}: {
  label: string;
  disabled: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type="button"
          variant="outline"
          size="icon"
          className="shrink-0"
          disabled={disabled}
          onClick={onClick}
        >
          {children}
          <span className="sr-only">{label}</span>
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

/** A preview-sized tile for a buddy that is not there yet. */
function EmptyTile({ children }: { children: React.ReactNode }) {
  return (
    <div
      aria-hidden="true"
      className="flex size-16 shrink-0 items-center justify-center rounded-md border border-dashed border-border/60 bg-background text-muted-foreground"
    >
      {children}
    </div>
  );
}

/** Calm or lively, a preference of this device. */
function EnergyCard({ id }: { id: string }) {
  const [energy, setEnergy] = useAtom(timerBuddyEnergyAtom);

  return (
    <div className={cn(CARD_CLASS, "flex items-center justify-between gap-3")}>
      <div className="space-y-0.5">
        <p id={id} className="text-sm font-medium">
          Energy
        </p>
        <p className="text-xs text-muted-foreground">
          How restless your buddies are on this device.
        </p>
      </div>
      <div
        role="group"
        aria-labelledby={id}
        className="flex shrink-0 rounded-md border border-border/60 p-0.5"
      >
        {ENERGY_OPTIONS.map((option) => (
          <button
            key={option.value}
            type="button"
            aria-pressed={energy === option.value}
            onClick={() => setEnergy(option.value)}
            className="rounded px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring aria-pressed:bg-accent aria-pressed:text-foreground"
          >
            {option.label}
          </button>
        ))}
      </div>
    </div>
  );
}
