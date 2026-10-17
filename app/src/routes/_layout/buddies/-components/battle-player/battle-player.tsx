import "@fontsource/press-start-2p/latin-400.css";
import "./battle-player.css";
import {
  type UseQueryResult,
  useQueries,
  useQueryClient,
} from "@tanstack/react-query";
import { useAtom } from "jotai/react";
import * as React from "react";
import { useMediaQuery } from "@/hooks/useMediaQuery";
import type { BattleDetail } from "@/lib/api/queries/buddy";
import { buddyReplaySpeedAtom } from "@/lib/buddy/preferences";
import { type BuddySheet, buddyQueries } from "@/lib/buddy/sheet";
import { cn } from "@/lib/utils";
import { BattleDirector, type ReplayStatus } from "./director";
import { SCREEN_HEIGHT, SCREEN_WIDTH } from "./screen";
import { buildBattleScript } from "./script";

const PIXEL_FONT = '8px "Press Start 2P"';

/** The replay starts without the pixel font if it takes longer than this. */
const FONT_TIMEOUT_MS = 3_000;

const MAX_SCALE = 3;

/** Height the page keeps for the header and controls when fitting the screen. */
const RESERVED_HEIGHT = 300;

type SheetsState = {
  /** Every species' sprites, once all have loaded. */
  readonly sheets: readonly BuddySheet[] | null;
  readonly failed: boolean;
};

/**
 * Defined once, and returning plain data, so the query cache's structural
 * sharing keeps the result's identity until a sheet really changes; a new
 * identity would restart the replay.
 */
function combineSheets(results: UseQueryResult<BuddySheet>[]): SheetsState {
  const loaded = results.flatMap((result) =>
    result.data ? [result.data] : [],
  );
  return {
    sheets: loaded.length === results.length ? loaded : null,
    failed: results.some((result) => result.isError),
  };
}

/**
 * The scale of the 240x160 screen that fits `width` and `height`. From 2x up
 * it moves in half steps, so a double-size sprite's pixels stay whole screen
 * pixels; a phone gets an exact fit.
 */
function fitScale(width: number, height: number): number {
  const fit = Math.min(
    width / SCREEN_WIDTH,
    Math.max(SCREEN_HEIGHT, height) / SCREEN_HEIGHT,
  );
  return fit >= 2
    ? Math.min(MAX_SCALE, Math.floor(fit * 2) / 2)
    : Math.max(0.5, fit);
}

function useScreenScale(ref: React.RefObject<HTMLElement | null>): number {
  const [scale, setScale] = React.useState(2);
  React.useEffect(() => {
    const element = ref.current;
    if (!element) {
      return;
    }
    const measure = () =>
      setScale(
        fitScale(element.clientWidth, window.innerHeight - RESERVED_HEIGHT),
      );
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    window.addEventListener("resize", measure);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", measure);
    };
  }, [ref]);
  return scale;
}

/** Whether the pixel font has loaded, or has had its chance to. */
function useFontReady(): boolean {
  const [ready, setReady] = React.useState(false);
  React.useEffect(() => {
    let live = true;
    const done = () => {
      if (live) {
        setReady(true);
      }
    };
    const timeout = window.setTimeout(done, FONT_TIMEOUT_MS);
    document.fonts.load(PIXEL_FONT).then(done, done);
    return () => {
      live = false;
      window.clearTimeout(timeout);
    };
  }, []);
  return ready;
}

/**
 * Replays a battle on a Gen 3 style battle screen, from the viewer's side:
 * their buddies bottom left, seen from behind. What it shows depends only on
 * the battle, so it can start over or skip to the end at any time. With
 * reduced motion nothing moves; the text is stepped through a page at a
 * time.
 */
export default function BattlePlayer({ battle }: { battle: BattleDetail }) {
  // Only the parts a replay depends on, which keep their identity when the
  // cached battle is marked as seen, so that does not restart it.
  const { role, kind, challenger, defender, events } = battle;
  const script = React.useMemo(
    () => buildBattleScript({ role, kind, challenger, defender, events }),
    [role, kind, challenger, defender, events],
  );
  const speciesIds = React.useMemo(
    () => [
      ...new Set(
        [...challenger.team, ...defender.team].map((mon) => mon.speciesId),
      ),
    ],
    [challenger, defender],
  );
  const { sheets: loadedSheets, failed } = useQueries({
    queries: speciesIds.map((id) => buddyQueries.sheet(id)),
    combine: combineSheets,
  });
  const sheets = React.useMemo(
    () =>
      loadedSheets && new Map(loadedSheets.map((sheet) => [sheet.id, sheet])),
    [loadedSheets],
  );
  const queryClient = useQueryClient();
  const retry = () => {
    for (const id of speciesIds) {
      void queryClient.refetchQueries({
        queryKey: buddyQueries.sheet(id).queryKey,
        predicate: (query) => query.state.status === "error",
      });
    }
  };
  const fontReady = useFontReady();
  const stepped = useMediaQuery("(prefers-reduced-motion: reduce)");
  const [speed, setSpeed] = useAtom(buddyReplaySpeedAtom);
  const [status, setStatus] = React.useState<ReplayStatus>({
    ended: false,
    stop: 0,
  });
  const [message, setMessage] = React.useState("");

  const frameRef = React.useRef<HTMLDivElement>(null);
  const screenRef = React.useRef<HTMLDivElement>(null);
  const directorRef = React.useRef<BattleDirector | null>(null);
  // Read when a director starts, so changing the speed does not restart it.
  const speedRef = React.useRef(speed);
  const scale = useScreenScale(frameRef);
  const ready = sheets !== null && fontReady;

  React.useEffect(() => {
    speedRef.current = speed;
    directorRef.current?.setSpeed(speed);
  }, [speed]);

  React.useEffect(() => {
    const root = screenRef.current;
    if (!root || !sheets || !fontReady) {
      return;
    }
    const director = new BattleDirector({
      root,
      script,
      sheets,
      stepped,
      speed: speedRef.current,
      onStatus: setStatus,
      onMessage: setMessage,
    });
    directorRef.current = director;
    director.start();
    return () => {
      director.destroy();
      directorRef.current = null;
    };
  }, [script, sheets, fontReady, stepped]);

  const moves = events.filter((event) => event.type === "move").length;

  return (
    <section
      aria-label="Battle replay"
      className="rounded-2xl bg-[#2b2d42] p-3 shadow-elevated sm:p-4"
    >
      <div ref={frameRef} className="w-full">
        <div
          className="relative mx-auto overflow-hidden rounded-[3px] bg-[#283040] shadow-[0_0_0_3px_#15161f]"
          style={{
            width: SCREEN_WIDTH * scale,
            height: SCREEN_HEIGHT * scale,
          }}
        >
          <div
            ref={screenRef}
            aria-hidden="true"
            className="bp-screen"
            style={{ transform: `scale(${scale})` }}
          />
          {!ready && (
            <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 bg-[#283040] p-4 text-center">
              {failed ? (
                <>
                  <p className="text-sm text-[#f8f8f8]">
                    Could not load the buddies&apos; sprites.
                  </p>
                  <ControlButton onClick={retry}>Retry</ControlButton>
                </>
              ) : (
                <p
                  role="status"
                  className="font-['Press_Start_2P',ui-monospace,monospace] text-[10px] text-[#f8f8f8]"
                >
                  Loading...
                </p>
              )}
            </div>
          )}
        </div>
      </div>

      <p className="sr-only" aria-live="polite" aria-atomic="true">
        {message}
      </p>

      <div
        className="mx-auto mt-3 flex max-w-full flex-wrap items-center justify-between gap-2"
        style={{ width: SCREEN_WIDTH * scale }}
      >
        <span className="font-mono text-xs text-[#b8bad8]">
          seed {battle.seed} · {moves} {moves === 1 ? "move" : "moves"}
        </span>
        <div className="flex flex-wrap items-center gap-1.5">
          {stepped ? (
            <>
              <span className="font-mono text-xs tabular-nums text-[#b8bad8]">
                {status.stop + 1}/{script.stops.length}
              </span>
              <ControlButton
                disabled={!ready || status.ended}
                onClick={() => directorRef.current?.next()}
              >
                Next
              </ControlButton>
            </>
          ) : (
            <ControlButton
              pressed={speed === 2}
              onClick={() => setSpeed(speed === 2 ? 1 : 2)}
            >
              2×<span className="sr-only"> speed</span>
            </ControlButton>
          )}
          <ControlButton
            disabled={!ready || status.ended}
            onClick={() => directorRef.current?.skip()}
          >
            Skip
          </ControlButton>
          <ControlButton
            disabled={!ready}
            onClick={() => directorRef.current?.replay()}
          >
            Replay
          </ControlButton>
        </div>
      </div>
    </section>
  );
}

/** A pill button in the handheld's colours; `pressed` makes it a toggle. */
function ControlButton({
  pressed,
  disabled,
  onClick,
  children,
}: {
  pressed?: boolean;
  disabled?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={pressed}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "rounded-full bg-[#45486c] px-3 py-1 text-xs font-semibold text-white transition-colors hover:bg-[#565a85] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[#f0c048] focus-visible:ring-offset-2 focus-visible:ring-offset-[#2b2d42] disabled:pointer-events-none disabled:opacity-50 motion-reduce:transition-none",
        pressed && "bg-[#f0c048] text-[#2b2d42] hover:bg-[#f4cf6a]",
      )}
    >
      {children}
    </button>
  );
}
