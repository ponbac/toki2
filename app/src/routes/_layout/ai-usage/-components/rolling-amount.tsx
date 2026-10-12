import * as React from "react";
import { animate, useReducedMotion } from "framer-motion";

/** Where a figure starts from when its unit changes, as a part of itself. */
const SETTLE_FROM = 0.86;

/**
 * A figure that rolls to its new value when it changes, as after another
 * filter is chosen, so the eye sees what moved. When its `unit` changes, as
 * with another currency, it settles from just below instead: the old number
 * means nothing in the new unit. It does not roll in when it first appears,
 * nor for a user who asked for reduced motion.
 */
export function RollingAmount({
  amount,
  unit,
  format,
}: {
  amount: number;
  unit: string;
  format: (amount: number) => string;
}) {
  const [shown, setShown] = React.useState({ amount, unit });
  // What is on screen, for the next roll to start from.
  const current = React.useRef({ amount, unit });
  const reduceMotion = useReducedMotion();

  React.useEffect(() => {
    const from =
      current.current.unit === unit
        ? current.current.amount
        : amount * SETTLE_FROM;
    if (reduceMotion || from === amount) {
      // Nothing to roll, but what is shown must still follow: a later roll,
      // or motion turned back on, starts from here.
      current.current = { amount, unit };
      setShown((shown) =>
        shown.amount === amount && shown.unit === unit
          ? shown
          : { amount, unit },
      );
      return;
    }
    const controls = animate(from, amount, {
      duration: 0.45,
      ease: [0.3, 0.7, 0.2, 1],
      onUpdate: (value) => {
        current.current = { amount: value, unit };
        setShown({ amount: value, unit });
      },
    });
    return () => controls.stop();
  }, [amount, unit, reduceMotion]);

  if (reduceMotion) {
    return <>{format(amount)}</>;
  }
  return (
    <>{format(shown.unit === unit ? shown.amount : amount * SETTLE_FROM)}</>
  );
}
