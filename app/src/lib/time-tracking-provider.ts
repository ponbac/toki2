export const TIME_TRACKING_PROVIDER_URL =
  import.meta.env.VITE_TIME_TRACKING_PROVIDER_URL?.trim() ||
  "https://my.kleer.se/web/";

export const KLEER_TIME_REPORTING_MONTH_URL =
  "https://my.kleer.se/web2/time-reporting/month";

export function kleerTimeReportingWeekUrl({
  isoWeek,
  isoWeekYear,
}: {
  isoWeek: number;
  isoWeekYear: number;
}) {
  return `https://my.kleer.se/web2/time-reporting/week/${isoWeekYear}/${isoWeek}`;
}
