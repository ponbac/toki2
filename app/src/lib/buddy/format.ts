/** Formats elapsed seconds for effect text: "1h 24m", "24m", or "40s" under a minute. */
export function formatBuddyDuration(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);

  if (hours > 0) {
    return `${hours}h ${minutes}m`;
  }
  return minutes > 0 ? `${minutes}m` : `${total}s`;
}

/** Formats whole minutes without spaces for the status plate: "5h36m", "8h", "24m". */
export function formatBuddyMinutes(minutes: number): string {
  const total = Math.max(0, Math.round(minutes));
  const hours = Math.floor(total / 60);
  const rest = total % 60;

  if (hours === 0) {
    return `${rest}m`;
  }
  return rest === 0 ? `${hours}h` : `${hours}h${String(rest).padStart(2, "0")}m`;
}
