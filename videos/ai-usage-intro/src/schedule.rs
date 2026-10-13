//! What `schedule install` sets up: a week on one machine. Each day at 12:30 the job runs and
//! the machine's card on the Machines tab hears from it; over a weekend with the machine off
//! nothing runs, and the missed run is caught up at the next login. The week is an
//! explanatory drawing; the machine's card and the status lines are the app's and the CLI's
//! own.
use crate::*;

const SECONDS: f32 = 12.0;

/// The week after the first sync on Thursday 27 Aug.
const DAYS: [&str; 7] = ["Fre", "Lör", "Sön", "Mån", "Tis", "Ons", "Tors"];
/// What each day's sync adds to the machine's estimate; nothing while it is off. Monday's
/// catches up on the weekend.
const ADDED: [f32; 7] = [286.10, 0.0, 0.0, 455.30, 342.60, 318.90, 301.70];
/// The first day the machine is off, and the day it catches up.
const WEEKEND: usize = 1;
const CATCH_UP: usize = 3;

const TRACK_LEFT: f32 = 100.0;
const TRACK_RIGHT: f32 = 1006.0;
const TRACK_Y: f32 = 560.0;
const SWEEP_AT: f32 = 1.4;
const SWEEP: f32 = 6.8;
/// How long a sync's pulse travels from the day to the Machines card.
const FLIGHT: f32 = 0.55;
const STATUS_AT: f32 = 8.7;

/// The Machines tab's card of the machine (`machines-panel.tsx`), larger than on the page.
const MACHINES_X: f32 = 1092.0;
const MACHINES_ZOOM: f32 = 1.8;
const CARD_WIDTH: f32 = 433.0;
const CARD_Y: f32 = 48.0;

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
        // The card starts from what the first sync uploaded; its dates cover every day shown.
        let estimate: f32 = data::first_sync_cost()
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
                    let (to_x, to_y) = (MACHINES_X + 30.0, 460.0);
                    let (bend_x, bend_y) = ((from_x + to_x) / 2.0, 290.0);
                    let x = (1.0 - p).powi(2) * from_x + 2.0 * (1.0 - p) * p * bend_x + p * p * to_x;
                    let y = (1.0 - p).powi(2) * from_y + 2.0 * (1.0 - p) * p * bend_y + p * p * to_y;
                    fframes::svgr!(<circle cx={x} cy={y} r={9.0 - tail as f32 * 1.8} fill={AMBER} opacity={1.0 - tail as f32 * 0.24} />)
                })
            })
            .collect();

        // One mark per tool whose history the machine read in full. Copilot has a log to
        // read from the first sync after its export was turned on.
        let copilot = ease_out(progress(t, day_at(0) + FLIGHT, 0.4));
        let chips = [
            (0, 20.0, 88.0, 1.0),
            (1, 114.0, 124.0, 1.0),
            (2, 244.0, 96.0, copilot),
            (3, lerp(244.0, 346.0, copilot), 64.0, 1.0),
        ];
        let read: Vec<Svgr> = chips
            .iter()
            .map(|(provider, x, width, shown)| {
                fframes::svgr!(<g opacity={*shown}>
                    <rect x={*x} y={CARD_Y + 114.0} width={*width} height="22" rx="11" fill="none" stroke={BORDER} stroke-width="1" />
                    {provider_icon(*provider, x + 9.0, CARD_Y + 119.0, 12.0)}
                    <text x={x + 27.0} y={CARD_Y + 129.5} font-size="12" font-weight="500" fill={FG}>{PROVIDER_NAMES[*provider]}</text>
                    {icon(icons::CHECK, x + width - 21.0, CARD_Y + 119.0, 12.0, FRESH)}
                </g>)
            })
            .collect();
        // A fresh sync pulses quietly beside the time since it.
        let pulse = (t / 2.6).fract();

        let ago = since((playhead - last_x) / 135.0);
        let dot_x = CARD_WIDTH - 20.0 - ago.chars().count() as f32 * 6.2 - 10.0;

        let status = [
            ("$ npx @ponbac/token-ledger@latest schedule status", FG),
            ("  token-ledger-sync.timer - Daily AI usage sync", FG),
            ("  Active: active (waiting)", MUTED),
            ("  Trigger: Fri 2026-09-04 12:30:54 CEST; 23h left", MUTED),
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
                    <rect x={day_x(WEEKEND) - 67.0} y="398" width="269" height="262" rx="18" fill="#000000" opacity="0.28" />
                    {icon(icons::POWER, day_x(WEEKEND) + 3.0, 420.0, 22.0, MUTED)}
                    <text x={day_x(WEEKEND) + 36.0} y="439" font-size="22" font-weight="500" fill={MUTED}>"avstängd"</text>
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
                    <g transform={format!("translate({MACHINES_X} 236) scale({MACHINES_ZOOM})")} font-family={SANS}>
                        {icon(icons::LAPTOP, 0.0, 5.0, 16.0, FG)}
                        <text x="24" y="18" font-size="14" font-weight="600" fill={FG}>"Machines"</text>
                        <rect x="97" y="5.5" width="18" height="15" rx="7.5" fill={MUTED_SURFACE} />
                        <text x="106" y="17" text-anchor="middle" font-size="11" font-weight="600" fill={MUTED}>"1"</text>
                        <path d={format!("M0 32 H{CARD_WIDTH}")} stroke={BORDER} stroke-width="1" opacity="0.6" />
                        <rect x="-4" y="31" width="127" height="2" rx="1" fill={AMBER} />
                        <rect x="0" y={CARD_Y} width={CARD_WIDTH} height="162" rx="16" fill={CARD} stroke={BORDER} stroke-width="1" />
                        <rect x="0" y={CARD_Y} width={CARD_WIDTH} height="162" rx="16" fill={AMBER} opacity={flash * 0.06} />
                        <rect x="0" y={CARD_Y} width={CARD_WIDTH} height="162" rx="16" fill="none" stroke={AMBER} stroke-width="1.5" opacity={flash} />
                        {icon(icons::LAPTOP, 20.0, CARD_Y + 24.0, 16.0, MUTED)}
                        <text x="44" y={CARD_Y + 37.5} font-size="16" font-weight="600" fill={FG}>{data::MACHINE}</text>
                        <text x={CARD_WIDTH - 20.0} y={CARD_Y + 36.0} text-anchor="end" font-size="12" font-weight="500" fill={MUTED}>{ago.clone()}</text>
                        <circle cx={dot_x} cy={CARD_Y + 32.0} r={4.0 + 5.0 * pulse} fill={FRESH} opacity={0.5 * (1.0 - pulse)} />
                        <circle cx={dot_x} cy={CARD_Y + 32.0} r="4" fill={FRESH} />
                        <text x="20" y={CARD_Y + 80.0} font-size="24" font-weight="600" letter-spacing="-0.6" fill={FG}>{money::shown(Currency::Sek, estimate)}</text>
                        <text x="20" y={CARD_Y + 101.0} font-size="12" font-weight="500" fill={MUTED}>{format!("token-ledger {}", data::CLIENT_VERSION)}</text>
                        {icon(icons::GLOBE, 132.0, CARD_Y + 90.5, 12.0, MUTED)}
                        <text x="148" y={CARD_Y + 101.0} font-size="12" font-weight="500" fill={MUTED}>"Europe/Stockholm"</text>
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
