//! The three commands of the dialog, pasted into a terminal. The lines they print are what
//! token-ledger 0.5.1 prints, set a little nicer: a check before each result, the token in
//! amber and shortened, and the tools shown as marks while their history is read.
use crate::*;

const SECONDS: f32 = 12.5;
const LEFT: f32 = 146.0;
const FIRST_LINE: f32 = 372.0;
const LINE: f32 = 50.0;
/// IBM Plex Mono at 24 px advances 0.6 em per character.
const CHARACTER: f32 = 14.4;
/// Pasted, not typed: fast.
const SPEED: f32 = 70.0;

const LOGIN_AT: f32 = 0.9;
const SYNC_AT: f32 = 3.9;
const SCHEDULE_AT: f32 = 7.6;
const SYNCED_AT: f32 = 6.1;
const SCHEDULED_AT: f32 = 9.0;

const CLI: &str = "npx --yes @ponbac/token-ledger@latest";

#[derive(Debug)]
pub struct Terminal;

fn line_y(line: usize) -> f32 {
    FIRST_LINE + line as f32 * LINE
}

fn check<'a>(line: usize) -> Svgr<'a> {
    fframes::svgr!(
        <path transform={format!("translate({} {})", LEFT + 1.0, line_y(line) - 17.0)} d="M0 9 L6 15 L18 2" fill="none" stroke={OK} stroke-width="3" stroke-linecap="round" stroke-linejoin="round" />
    )
}

/// A prompt and a command of coloured parts, shown up to what was typed by `t`. Returns the
/// line and where the caret stands after it.
fn command<'a>(t: f32, line: usize, start: f32, parts: &[(&'a str, &'a str)]) -> (Svgr<'a>, f32) {
    let mut left = ((t - start) * SPEED).max(0.0) as usize;
    let mut column = 2;
    let spans: Vec<Svgr> = parts
        .iter()
        .map(|(text, color)| {
            let count = text.chars().count().min(left);
            let end = text
                .char_indices()
                .nth(count)
                .map_or(text.len(), |(index, _)| index);
            let x = LEFT + CHARACTER * column as f32;
            left -= count;
            column += count;
            fframes::svgr!(<text x={x} y={line_y(line)} fill={*color}>{&text[..end]}</text>)
        })
        .collect();
    let shown = if t >= start - 0.25 { 1.0 } else { 0.0 };
    (
        fframes::svgr!(<g opacity={shown}>
            <text x={LEFT} y={line_y(line)} fill={AMBER}>"$"</text>
            {spans}
        </g>),
        LEFT + CHARACTER * column as f32,
    )
}

/// A result line with a check, fading in at `start`.
fn result<'a>(frame: &Frame, line: usize, start: f32, text: String) -> Svgr<'a> {
    fframes::svgr!(
        <g opacity={ramp(frame, start)} transform={Transform::translate(0, rise(frame, start, 8.0))}>
            {check(line)}
            <text x={LEFT + CHARACTER * 2.0} y={line_y(line)} fill={FG}>{text}</text>
        </g>
    )
}

impl Scene for Terminal {
    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(SECONDS)
    }

    fn audio(&self) -> AudioMap<'_> {
        // Times are relative to the scene. Typing stays far in the background.
        AudioMap::from([
            AudioTrack::new("typing.wav", Second(LOGIN_AT)..Eof).gain_db(-13.0),
            AudioTrack::new("typing.wav", Second(SYNC_AT)..Eof).gain_db(-13.0),
            AudioTrack::new("typing.wav", Second(SCHEDULE_AT)..Eof).gain_db(-13.0),
            AudioTrack::new("chime.wav", Second(SYNCED_AT)..Eof).gain_db(-7.0),
            AudioTrack::new("click.wav", Second(SCHEDULED_AT)..Eof).gain_db(-7.0),
        ])
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = frame.seconds();

        let (login, login_caret) = command(
            t,
            0,
            LOGIN_AT,
            &[
                ("TOKI_API_TOKEN=", FG),
                ("toki_3f9a…0486", AMBER),
                (" ", FG),
                (CLI, FG),
                (" login --server https://toki-api.spinit.se", FG),
            ],
        );
        let (sync, sync_caret) = command(t, 3, SYNC_AT, &[(CLI, FG), (" sync", FG)]);
        let (schedule, schedule_caret) =
            command(t, 8, SCHEDULE_AT, &[(CLI, FG), (" schedule install", FG)]);

        // The caret follows the command being typed, then waits on a fresh prompt.
        let (caret_x, caret_line) = if t < SYNC_AT - 0.25 {
            (login_caret, 0)
        } else if t < SCHEDULE_AT - 0.25 {
            (sync_caret, 3)
        } else if t < SCHEDULED_AT + 0.5 {
            (schedule_caret, 8)
        } else {
            (LEFT + CHARACTER * 2.0, 10)
        };
        let running = (t > 2.5 && t < SYNC_AT - 0.25)
            || (t > 4.6 && t < SCHEDULE_AT - 0.25)
            || (t > 8.5 && t < SCHEDULED_AT + 0.5);
        let caret = if running || (t * 1.6).fract() > 0.55 {
            0.0
        } else {
            0.9
        };

        // Copilot keeps no log before the next step of the dialog turns its export on.
        let tools: Vec<Svgr> = [0, 1, 3]
            .into_iter()
            .enumerate()
            .map(|(place, i)| {
                let start = 4.8 + place as f32 * 0.25;
                let x = LEFT + CHARACTER * 2.0 + [0.0, 172.0, 428.0][place];
                let width = PROVIDER_NAMES[i].len() as f32 * CHARACTER;
                fframes::svgr!(
                    <g opacity={ramp(&frame, start)}>
                        {provider_icon(i, x, line_y(4) - 20.0, 23.0)}
                        <text x={x + 33.0} y={line_y(4)} fill={MUTED}>{PROVIDER_NAMES[i]}</text>
                        <path opacity={ramp(&frame, start + 0.3)} transform={format!("translate({} {})", x + 43.0 + width, line_y(4) - 15.0)} d="M0 8 L5 13 L15 2" fill="none" stroke={OK} stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" />
                    </g>
                )
            })
            .collect();

        fframes::svgr!(
            <g opacity={scene_fade(t, SECONDS)}>
                {caption(&frame, "2", "Klistra in i terminalen", "Tre kommandon: spara server och token, ladda upp datorns historik, schemalägg en daglig synk.")}
                <g opacity={ramp(&frame, 0.1)} transform={Transform::translate(0, rise(&frame, 0.1, 36.0))}>
                    <rect x="96" y="250" width="1728" height="690" rx="22" fill="#0b0d13" stroke={BORDER} stroke-width="1.5" />
                    <path d="M96 312 H1824" stroke={BORDER} stroke-width="1.5" />
                    <circle cx="136" cy="281" r="8" fill="#3a3f4b" />
                    <circle cx="164" cy="281" r="8" fill="#3a3f4b" />
                    <circle cx="192" cy="281" r="8" fill="#3a3f4b" />
                    <text x="960" y="289" text-anchor="middle" font-family={MONO} font-size="22" font-weight="500" fill={MUTED}>{format!("{}: ~", data::MACHINE)}</text>
                    <g font-family={MONO} font-size="24" font-weight="500">
                        {login}
                        {result(&frame, 1, 2.9, "Saved the server and API token to ~/.config/token-ledger/sync.json".to_owned())}
                        {sync}
                        {tools}
                        {result(&frame, 5, SYNCED_AT, format!("Synced {} · 2026-08-14 through 2026-08-27 (Europe/Stockholm)", data::MACHINE))}
                        <text x={LEFT + CHARACTER * 2.0} y={line_y(6)} fill={MUTED} opacity={ramp(&frame, SYNCED_AT + 0.3)}>{format!("812 hourly buckets · {} sessions · {} API estimate · server stored 812", (data::SESSIONS * data::first_sync_cost() / data::TOTAL_COST).round(), usd(data::first_sync_cost()))}</text>
                        {schedule}
                        {result(&frame, 9, SCHEDULED_AT, "Scheduled a daily sync at 12:30 as a systemd user timer.".to_owned())}
                        <text x={LEFT} y={line_y(10)} fill={AMBER} opacity={progress(t, SCHEDULED_AT + 0.5, 0.1)}>"$"</text>
                        <rect x={caret_x} y={line_y(caret_line) - 22.0} width="13" height="29" fill={FG} opacity={caret} />
                    </g>
                </g>
            </g>
        )
    }
}
