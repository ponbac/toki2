//! What `schedule install` sets up: a week on one machine. Each day at 12:30 the job runs and
//! the Machines card of the page hears from it; over a weekend with the machine off nothing
//! runs, and the missed run is caught up at the next login. The week is an explanatory
//! drawing; the Machines card and the status lines are the app's and the CLI's own.
use crate::*;

const SECONDS: f32 = 12.0;

const DAYS: [&str; 7] = ["Tors", "Fre", "Lör", "Sön", "Mån", "Tis", "Ons"];
/// What each day's sync adds to the machine's estimate; nothing while it is off.
const ADDED: [f32; 7] = [286.10, 342.60, 0.0, 0.0, 455.30, 318.90, 301.70];
const FIRST_UPLOAD: f32 = 3_296.40;
const CATCH_UP: usize = 4;

const TRACK_LEFT: f32 = 100.0;
const TRACK_RIGHT: f32 = 1006.0;
const TRACK_Y: f32 = 560.0;
const SWEEP_AT: f32 = 1.4;
const SWEEP: f32 = 6.8;
/// How long a sync's pulse travels from the day to the Machines card.
const FLIGHT: f32 = 0.55;
const STATUS_AT: f32 = 8.7;

/// The Machines card, in page pixels at the page's zoom.
const MACHINES_X: f32 = 1092.0;

#[derive(Debug)]
pub struct Schedule;

fn day_x(day: usize) -> f32 {
    148.0 + day as f32 * 135.0
}

/// When the sweep reaches `day`, in scene seconds.
fn day_at(day: usize) -> f32 {
    SWEEP_AT + SWEEP * (day_x(day) - TRACK_LEFT) / (TRACK_RIGHT - TRACK_LEFT)
}

fn off(day: usize) -> bool {
    ADDED[day] == 0.0
}

/// "Last sync ..." as the app's relative time puts it, `days` after the sync.
fn since(days: f32) -> String {
    match days {
        d if d < 0.09 => "a few seconds ago".to_owned(),
        d if d < 0.9 => format!("{} hours ago", ((d * 24.0).round() as usize).max(2)),
        d if d < 1.5 => "a day ago".to_owned(),
        d => format!("{} days ago", d.round() as usize),
    }
}

impl Scene for Schedule {
    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(SECONDS)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap(Some(
            (0..DAYS.len())
                .filter(|day| !off(*day))
                .map(|day| {
                    AudioTrack::new("click.wav", Second(day_at(day) + FLIGHT)..Eof).gain_db(-9.0)
                })
                .collect(),
        ))
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = frame.seconds();
        let sweep = progress(t, SWEEP_AT, SWEEP);
        let playhead = lerp(TRACK_LEFT, TRACK_RIGHT, sweep);

        // The syncs that have landed on the server, and how far the sweep is past the last.
        let landed = |day: usize| !off(day) && t >= day_at(day) + FLIGHT;
        let last = (0..DAYS.len()).rev().find(|day| landed(*day));
        let last_x = last.map_or(TRACK_LEFT, day_x);
        let estimate: f32 = FIRST_UPLOAD
            + (0..DAYS.len())
                .filter(|day| !off(*day))
                .map(|day| ADDED[day] * ease_out(progress(t, day_at(day) + FLIGHT, 0.5)))
                .sum::<f32>();
        let flash = last.map_or(0.0, |day| 1.0 - progress(t, day_at(day) + FLIGHT, 0.9));

        let days: Vec<Svgr> = DAYS
            .iter()
            .enumerate()
            .map(|(day, name)| {
                let x = day_x(day);
                let ran = progress(t, day_at(day), 0.2);
                let time = if day == CATCH_UP { "08:07" } else { "12:30" };
                let node = if off(day) {
                    fframes::svgr!(<circle cx={x} cy={TRACK_Y} r="17" fill={CARD} stroke={OTHER} stroke-width="3" stroke-dasharray="5 6" />)
                } else {
                    fframes::svgr!(<g>
                        <circle cx={x} cy={TRACK_Y} r="18" fill={CARD} stroke={BORDER} stroke-width="3" />
                        <circle cx={x} cy={TRACK_Y} r={(18.0 * ease_out(ran)).max(0.01)} fill={OK} />
                        <path opacity={ran} transform={format!("translate({} {})", x - 9.0, TRACK_Y - 8.0)} d="M0 8 L6 14 L18 1" fill="none" stroke={CARD} stroke-width="3.5" stroke-linecap="round" stroke-linejoin="round" />
                    </g>)
                };
                fframes::svgr!(<g>
                    <text x={x} y={TRACK_Y - 44.0} text-anchor="middle" font-family={MONO} font-size="22" font-weight="500"
                        fill={if day == CATCH_UP { AMBER } else { MUTED }} opacity={if off(day) { 0.0 } else { 0.45 + 0.55 * ran }}>{time}</text>
                    {node}
                    <text x={x} y={TRACK_Y + 64.0} text-anchor="middle" font-size="26" font-weight="500" fill={if off(day) { MUTED } else { FG }}>{*name}</text>
                </g>)
            })
            .collect();

        // Each sync flies from its day to the Machines card, with a short tail.
        let pulses: Vec<Svgr> = (0..DAYS.len())
            .filter(|day| !off(*day))
            .flat_map(|day| (0..4).map(move |tail| (day, tail)))
            .filter_map(|(day, tail)| {
                let p = (t - day_at(day)) / FLIGHT - tail as f32 * 0.07;
                (p > 0.0 && p < 1.0).then(|| {
                    let p = ease_in_out(p);
                    let (from_x, from_y) = (day_x(day), TRACK_Y - 18.0);
                    let (to_x, to_y) = (MACHINES_X + 30.0, 452.0);
                    let (bend_x, bend_y) = ((from_x + to_x) / 2.0, 290.0);
                    let x = (1.0 - p).powi(2) * from_x + 2.0 * (1.0 - p) * p * bend_x + p * p * to_x;
                    let y = (1.0 - p).powi(2) * from_y + 2.0 * (1.0 - p) * p * bend_y + p * p * to_y;
                    fframes::svgr!(<circle cx={x} cy={y} r={9.0 - tail as f32 * 1.8} fill={AMBER} opacity={1.0 - tail as f32 * 0.24} />)
                })
            })
            .collect();

        let read: Vec<Svgr> = [(0, 0.0), (1, 66.0), (3, 164.0)]
            .iter()
            .map(|(provider, offset)| {
                let x = 146.0 + offset;
                fframes::svgr!(<g>
                    {provider_icon(*provider, x, 201.0, 13.0)}
                    <text x={x + 18.0} y="212" font-size="12" font-weight="500" fill={MUTED}>{PROVIDER_NAMES[*provider]}</text>
                </g>)
            })
            .collect();

        let status = [
            ("$ npx @ponbac/token-ledger@latest schedule status", FG),
            ("  token-ledger-sync.timer - Daily AI usage sync", FG),
            ("  Active: active (waiting)", MUTED),
            ("  Trigger: Thu 2026-10-22 12:30:54 CEST; 23h left", MUTED),
        ];
        let status_lines: Vec<Svgr> = status
            .iter()
            .enumerate()
            .map(|(i, (line, color))| {
                fframes::svgr!(<text x="1124" y={706.0 + i as f32 * 40.0} fill={*color} opacity={ramp(&frame, STATUS_AT + 0.2 + i as f32 * 0.18)}>{*line}</text>)
            })
            .collect();

        fframes::svgr!(
            <g opacity={scene_fade(t, SECONDS)}>
                {caption(&frame, "3", "Sedan synkar den själv", "Ett dagligt jobb i systemets egen schemaläggare. Inget ligger och kör däremellan.")}
                <g opacity={ramp(&frame, 0.2)} transform={Transform::translate(0, rise(&frame, 0.2, 24.0))} font-family={SANS}>
                    <rect x="48" y="236" width="1010" height="640" rx="24" fill={CARD} stroke={BORDER} stroke-width="1.5" />
                    {icon(icons::LAPTOP, 86.0, 276.0, 34.0, FG)}
                    <text x="136" y="304" font-size="34" font-weight="600" letter-spacing="-0.6" fill={FG}>{data::MACHINE}</text>
                    <text x="1018" y="302" text-anchor="end" font-family={MONO} font-size="20" font-weight="500" fill={MUTED}>"systemd user timer · varje dag 12:30"</text>
                    <rect x={day_x(2) - 67.0} y="398" width="269" height="262" rx="18" fill="#000000" opacity="0.28" />
                    {icon(icons::POWER, day_x(2) + 3.0, 420.0, 22.0, MUTED)}
                    <text x={day_x(2) + 36.0} y="439" font-size="22" font-weight="500" fill={MUTED}>"avstängd"</text>
                    <path d={format!("M{TRACK_LEFT} {TRACK_Y} H{TRACK_RIGHT}")} stroke={BORDER} stroke-width="3" stroke-linecap="round" />
                    <path d={format!("M{TRACK_LEFT} {TRACK_Y} H{playhead}")} stroke={AMBER} stroke-width="3" stroke-linecap="round" opacity="0.7" />
                    {days}
                    <circle cx={playhead} cy={TRACK_Y} r="7" fill={AMBER} opacity={progress(t, SWEEP_AT - 0.2, 0.2) * (1.0 - progress(t, SWEEP_AT + SWEEP, 0.3))} />
                    <g opacity={ramp(&frame, day_at(CATCH_UP) + 0.1)}>
                        <path d={format!("M{} {} V{}", day_x(CATCH_UP), TRACK_Y + 84.0, TRACK_Y + 112.0)} stroke={AMBER} stroke-width="2" />
                        <text x={day_x(CATCH_UP)} y={TRACK_Y + 144.0} text-anchor="middle" font-size="22" font-weight="600" fill={AMBER}>"ikapp vid inloggning"</text>
                    </g>
                    <text x="88" y="826" font-size="24" font-weight="500" fill={MUTED}>"Missar datorn 12:30 körs synken inom tio minuter efter nästa inloggning."</text>
                </g>
                <g opacity={ramp(&frame, 0.35)} transform={Transform::translate(0, rise(&frame, 0.35, 24.0))}>
                    <g transform={format!("translate({MACHINES_X} {PAGE_Y}) scale({ZOOM})")} font-family={SANS}>
                        {card(0.0, 0.0, 520.0, 250.0, "Machines", "Machines that sync your usage, with all their usage from")}
                        <g font-size="12" font-weight="500" fill={MUTED}>
                            <text x="20" y="72">"1 Oct to 31 Oct, whatever the other filters. One without a"</text>
                            <text x="20" y="89">"sync for more than 7 days is stale: its recent usage is"</text>
                            <text x="20" y="106">"missing, not zero."</text>
                        </g>
                        {button(374.0, 18.0, 126.0, 36.0, "Add machine", ButtonStyle::Outline, Some(icons::PLUS), None)}
                        <rect x="20" y="124" width="480" height="106" rx="12" fill="#171b24" stroke={BORDER} stroke-width="1" />
                        <rect x="20" y="124" width="480" height="106" rx="12" fill={AMBER} opacity={flash * 0.08} />
                        <rect x="20" y="124" width="480" height="106" rx="12" fill="none" stroke={AMBER} stroke-width="1.5" opacity={flash} />
                        {icon(icons::LAPTOP, 32.0, 140.0, 16.0, MUTED)}
                        <text x="56" y="153" font-size="14" font-weight="500" fill={FG}>{data::MACHINE}</text>
                        <text x="488" y="152" text-anchor="end" font-size="12" font-weight="500" fill={MUTED}>"est. in these dates"</text>
                        <text x="382" y="152" text-anchor="end" font-size="12" font-weight="600" fill={FG}>{usd(estimate)}</text>
                        <text x="32" y="174" font-size="12" font-weight="500" fill={MUTED}>{format!("Last sync {} · token-ledger 0.3.0 · Europe/Stockholm", since((playhead - last_x) / 135.0))}</text>
                        <text x="32" y="212" font-size="12" font-weight="500" fill={MUTED}>"History read in full:"</text>
                        {read}
                    </g>
                </g>
                <g opacity={ramp(&frame, STATUS_AT)} transform={Transform::translate(0, rise(&frame, STATUS_AT, 20.0))} font-family={MONO} font-size="20" font-weight="500">
                    <rect x="1092" y="640" width="780" height="236" rx="20" fill="#0b0d13" stroke={BORDER} stroke-width="1.5" />
                    <circle cx="1130" cy="739" r="6" fill={OK} opacity={ramp(&frame, STATUS_AT + 0.38)} />
                    {status_lines}
                </g>
                {pulses}
            </g>
        )
    }
}
