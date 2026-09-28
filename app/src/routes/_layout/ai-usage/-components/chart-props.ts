/** The geometry recharts passes to custom labels and shapes. */
export type MarkGeometry = Readonly<{
  x: number;
  y: number;
  width: number;
  height: number;
  index: number;
}>;

/**
 * Reads a mark's geometry from the loosely typed props recharts passes to a
 * custom label or shape, or null when they lack it.
 */
export function parseMarkGeometry(props: unknown): MarkGeometry | null {
  if (typeof props !== "object" || props === null) {
    return null;
  }
  const { x, y, width, height, index } = {
    x: "x" in props ? props.x : undefined,
    y: "y" in props ? props.y : undefined,
    width: "width" in props ? props.width : undefined,
    height: "height" in props ? props.height : undefined,
    index: "index" in props ? props.index : undefined,
  };
  const number = (value: unknown) =>
    typeof value === "number"
      ? value
      : typeof value === "string" && value !== ""
        ? Number(value)
        : NaN;
  const geometry = {
    x: number(x),
    y: number(y),
    width: number(width),
    height: number(height),
    index: number(index),
  };
  return Object.values(geometry).every(Number.isFinite) ? geometry : null;
}
