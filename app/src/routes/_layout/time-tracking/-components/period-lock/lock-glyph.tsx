import { motion, useReducedMotion } from "framer-motion";

/** A padlock whose shackle swings open, and snaps down when the time is locked. */
export function LockGlyph(props: { closed: boolean; className?: string }) {
  const reduceMotion = useReducedMotion();

  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
      className={props.className}
    >
      <motion.path
        d="M8 11V7a4 4 0 0 1 8 0v4"
        style={{ transformOrigin: "16px 11px" }}
        initial={false}
        animate={
          props.closed ? { y: 0, rotate: 0 } : { y: -2.5, rotate: 24 }
        }
        transition={
          reduceMotion
            ? { duration: 0 }
            : { type: "spring", stiffness: 700, damping: 14 }
        }
      />
      <rect
        x="4.5"
        y="11"
        width="15"
        height="10"
        rx="2.5"
        fill="currentColor"
        fillOpacity={0.18}
      />
    </svg>
  );
}
