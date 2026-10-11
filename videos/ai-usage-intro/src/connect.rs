//! The onboarding of the page (`connect-machine-prompt.tsx`, `connect-machine-dialog.tsx`):
//! the empty state with its outline of the dashboard, then the dialog through its first
//! three steps, ending on the copied commands.
use crate::*;

const SECONDS: f32 = 19.5;

const OPEN_AT: f32 = 3.6;
const SYSTEM_NEXT_AT: f32 = 6.6;
const CREATE_AT: f32 = 8.3;
const READY_AT: f32 = 8.8;
const TOKEN_NEXT_AT: f32 = 10.4;
const COPY_AT: f32 = 15.0;

/// The prompt card of the empty state, in page pixels.
const CARD_X: f32 = 384.0;
const CARD_Y: f32 = 90.0;
const CARD_WIDTH: f32 = 448.0;
const CARD_HEIGHT: f32 = 284.0;
const CENTER: f32 = 608.0;

/// The dialog: `sm:max-w-xl`, drawn larger than the page behind it so its text can be read.
pub(crate) const DIALOG_WIDTH: f32 = 576.0;
pub(crate) const DIALOG_ZOOM: f32 = 1.5;
pub(crate) const DIALOG_CENTER_Y: f32 = 640.0;
pub(crate) const CONTENT_TOP: f32 = 110.0;
/// The dialog as deployed is 362 px tall on its short steps.
pub(crate) const SHORT: f32 = 362.0;
const TALL: f32 = 558.0;
/// IBM Plex Mono at 12 px.
const CHARACTER: f32 = 7.2;

/// The heights of the preview's bars, in percent; a shape, not data.
const PREVIEW_BARS: [f32; 14] = [
    30.0, 55.0, 42.0, 70.0, 20.0, 8.0, 12.0, 64.0, 88.0, 52.0, 76.0, 40.0, 10.0, 14.0,
];

#[derive(Debug)]
pub struct Connect;

fn dialog_height(t: f32) -> f32 {
    lerp(SHORT, TALL, ease_in_out(progress(t, TOKEN_NEXT_AT, 0.45)))
}

/// A point of the dialog in video pixels.
fn on_dialog(t: f32, x: f32, y: f32) -> (f32, f32) {
    (
        960.0 + (x - DIALOG_WIDTH / 2.0) * DIALOG_ZOOM,
        DIALOG_CENTER_Y + (y - dialog_height(t) / 2.0) * DIALOG_ZOOM,
    )
}

/// Where the pointer is at `t`, in video pixels.
fn pointer_at(t: f32) -> (f32, f32) {
    let next = (DIALOG_WIDTH - 24.0 - 44.0, dialog_height(t) - 24.0 - 20.0);
    let stops = [
        (1480.0, 930.0),
        (PAGE_X + CENTER * ZOOM, PAGE_Y + (CARD_Y + 206.0) * ZOOM),
        on_dialog(t, next.0, next.1),
        on_dialog(t, 477.0, CONTENT_TOP + 40.0),
        on_dialog(t, next.0, next.1),
        on_dialog(t, 498.0, CONTENT_TOP + 22.0),
        on_dialog(t, 420.0, 330.0),
    ];
    const MOVES: [(f32, f32); 6] = [
        (2.3, 1.0),
        (5.5, 0.8),
        (7.3, 0.8),
        (9.3, 0.8),
        (13.8, 0.9),
        (15.7, 1.0),
    ];
    let mut position = stops[0];
    for (i, (start, duration)) in MOVES.iter().enumerate() {
        let p = ease_in_out(progress(t, *start, *duration));
        position = (
            lerp(stops[i].0, stops[i + 1].0, p),
            lerp(stops[i].1, stops[i + 1].1, p),
        );
        if p < 1.0 {
            break;
        }
    }
    position
}

/// The dashboard's outline behind the prompt: four tiles and a chart, without figures.
pub(crate) fn preview<'a>() -> Svgr<'a> {
    let tiles: Vec<Svgr> = (0..4)
        .map(|i| {
            let x = i as f32 * 307.0;
            fframes::svgr!(<g>
                <rect x={x} y="0" width="295" height="88" rx="16" fill={CARD} stroke={BORDER} stroke-width="1" />
                <rect x={x + 16.0} y="16" width="175" height="8" rx="4" fill={MUTED_SURFACE} />
                <rect x={x + 16.0} y="34" width="105" height="20" rx="6" fill={MUTED_SURFACE} />
                <rect x={x + 16.0} y="64" width="210" height="8" rx="4" fill={MUTED_SURFACE} />
            </g>)
        })
        .collect();
    let bars: Vec<Svgr> = PREVIEW_BARS
        .iter()
        .enumerate()
        .map(|(i, percent)| {
            let width = (1176.0 - 13.0 * 8.0) / 14.0;
            let x = 20.0 + i as f32 * (width + 8.0);
            let height = percent * 2.88;
            let top = 444.0 - height;
            let split = top + height * 0.4;
            let radius = 6.0_f32.min(height * 0.4);
            fframes::svgr!(<g>
                <path d={format!("M{x} {split} V{} Q{x} {top} {} {top} H{} Q{} {top} {} {} V{split} Z", top + radius, x + radius, x + width - radius, x + width, x + width, top + radius)} fill={PROVIDER_COLORS[1]} />
                <rect x={x} y={split} width={width} height={444.0 - split} fill={PROVIDER_COLORS[0]} />
            </g>)
        })
        .collect();
    fframes::svgr!(
        <g opacity="0.5">
            {tiles}
            <rect x="0" y="108" width="1216" height="356" rx="16" fill={CARD} stroke={BORDER} stroke-width="1" />
            <rect x="20" y="128" width="128" height="8" rx="4" fill={MUTED_SURFACE} />
            {bars}
        </g>
    )
}

/// "No machine is syncing yet": what to expect, the tools that are read, and the way in.
fn prompt<'a>(pressed: f32) -> Svgr<'a> {
    let tools = [
        ("Codex", 31.0),
        ("Claude Code", 67.0),
        ("Grok", 25.0),
        ("Copilot", 43.0),
        ("Pi", 11.0),
    ];
    let mut x = CENTER - 162.5;
    let marks: Vec<Svgr> = tools
        .iter()
        .enumerate()
        .map(|(i, (name, width))| {
            let at = x;
            x += 20.0 + width + 12.0;
            // Grok is third in the app's list and has no slot among this video's four tools.
            let drawn = match i {
                0 | 1 => provider_icon(i, at, CARD_Y + 161.0, 14.0),
                2 => mark(data::GROK_PATH, at, CARD_Y + 161.0, 14.0, "#199e70"),
                _ => provider_icon(i - 1, at, CARD_Y + 161.0, 14.0),
            };
            fframes::svgr!(<g>
                {drawn}
                <text x={at + 20.0} y={CARD_Y + 172.0} font-size="12" font-weight="500" fill={MUTED}>{*name}</text>
            </g>)
        })
        .collect();
    fframes::svgr!(
        <g font-family={SANS}>
            <rect x={CARD_X + 2.0} y={CARD_Y + 10.0} width={CARD_WIDTH - 4.0} height={CARD_HEIGHT} rx="18" fill="#000000" opacity="0.35" />
            <rect x={CARD_X} y={CARD_Y} width={CARD_WIDTH} height={CARD_HEIGHT} rx="16" fill={CARD} stroke={BORDER} stroke-width="1" />
            <rect x={CENTER - 22.0} y={CARD_Y + 24.0} width="44" height="44" rx="12" fill={AMBER} opacity="0.1" />
            {icon(icons::LAPTOP, CENTER - 10.0, CARD_Y + 36.0, 20.0, AMBER)}
            <text x={CENTER} y={CARD_Y + 101.0} text-anchor="middle" font-size="18" font-weight="600" letter-spacing="-0.4" fill={FG}>"No machine is syncing yet"</text>
            <text x={CENTER} y={CARD_Y + 124.0} text-anchor="middle" font-size="14" font-weight="500" fill={MUTED}>"Connect one and this page fills with your usage per day,"</text>
            <text x={CENTER} y={CARD_Y + 144.0} text-anchor="middle" font-size="14" font-weight="500" fill={MUTED}>"project and model. It takes about two minutes."</text>
            {marks}
            <g opacity={1.0 - pressed * 0.15}>
                {button(CENTER - 95.0, CARD_Y + 186.0, 190.0, 40.0, "Connect a machine", ButtonStyle::Primary, Some(icons::PLUG), None)}
            </g>
            {icon(icons::SHIELD_CHECK, CENTER - 197.0, CARD_Y + 242.0, 14.0, MUTED)}
            <text x={CENTER - 177.0} y={CARD_Y + 253.0} font-size="12" font-weight="500" fill={MUTED}>"Uploads usage totals and repository names, never prompts or code."</text>
        </g>
    )
}

/// Step 1: the machine's system, with the browser's own preselected.
fn system_step<'a>() -> Svgr<'a> {
    let systems = [
        ("macOS", data::APPLE_PATH),
        ("Linux", data::TUX_PATH),
        ("Windows", data::WINDOWS_PATH),
    ];
    let options: Vec<Svgr> = systems
        .iter()
        .enumerate()
        .map(|(i, (name, path))| {
            let width = (528.0 - 16.0) / 3.0;
            let x = 24.0 + i as f32 * (width + 8.0);
            let selected = i == 1;
            let ink = if selected { FG } else { MUTED };
            fframes::svgr!(<g>
                <rect x={x} y={CONTENT_TOP} width={width} height="112" rx="12" fill={AMBER} opacity={if selected { 0.1 } else { 0.0 }} />
                <rect x={x} y={CONTENT_TOP} width={width} height="112" rx="12" fill="none" stroke={if selected { AMBER } else { BORDER }} stroke-width="1" />
                {mark(path, x + width / 2.0 - 14.0, CONTENT_TOP + 16.0, 28.0, ink)}
                <text x={x + width / 2.0} y={CONTENT_TOP + 67.0} text-anchor="middle" font-size="14" font-weight="500" fill={ink}>{*name}</text>
                <text x={x + width / 2.0} y={CONTENT_TOP + 90.0} text-anchor="middle" font-size="11" font-weight="500" fill={MUTED}>{if selected { "This browser" } else { "" }}</text>
            </g>)
        })
        .collect();
    fframes::svgr!(
        <g>
            {options}
            <g font-size="12" font-weight="500" fill={MUTED}>
                <text x="24" y={CONTENT_TOP + 136.0}>"Pick the system of the machine you are connecting. token-ledger runs through npx and needs"</text>
                <text x="24" y={CONTENT_TOP + 152.0}>"Node.js 24 or newer: check with node --version, and install it with your package manager,"</text>
                <text x="24" y={CONTENT_TOP + 168.0}>"mise or nvm."</text>
            </g>
        </g>
    )
}

/// Step 2: name the token and create it in place; then it reports the token as ready.
fn token_step<'a>(t: f32) -> Svgr<'a> {
    if t >= READY_AT {
        return fframes::svgr!(
            <g opacity={progress(t, READY_AT, 0.2)}>
                <rect x="24" y={CONTENT_TOP} width="528" height="104" rx="12" fill={AMBER} opacity="0.05" />
                <rect x="24" y={CONTENT_TOP} width="528" height="104" rx="12" fill="none" stroke={AMBER} stroke-width="1" opacity="0.3" />
                {icon(icons::CHECK, 37.0, CONTENT_TOP + 14.0, 16.0, AMBER)}
                <text x="65" y={CONTENT_TOP + 27.0} font-size="14" font-weight="500" fill={FG}>"token-ledger Linux"</text>
                <text x="194" y={CONTENT_TOP + 27.0} font-family={MONO} font-size="12" font-weight="400" fill={MUTED}>"toki_3f9a1c7e…"</text>
                <text x="300" y={CONTENT_TOP + 27.0} font-size="14" font-weight="500" fill={FG}>"is ready."</text>
                <g font-size="12" font-weight="500" fill={MUTED}>
                    <text x="65" y={CONTENT_TOP + 49.0}>"Its secret is filled into the commands of the next step. Toki shows it only"</text>
                    <text x="65" y={CONTENT_TOP + 67.0}>"this once, so finish the setup before you leave this page."</text>
                    <text x="65" y={CONTENT_TOP + 89.0}>"Revoked it since? Create another token."</text>
                </g>
            </g>
        );
    }
    let creating = t >= CREATE_AT;
    fframes::svgr!(
        <g>
            <text x="24" y={CONTENT_TOP + 11.0} font-size="12" font-weight="500" fill={FG}>"Token name"</text>
            <rect x="24" y={CONTENT_TOP + 20.0} width="370" height="40" rx="10" fill={BG} stroke={BORDER} stroke-width="1" />
            <text x="37" y={CONTENT_TOP + 45.0} font-size="14" font-weight="500" fill={FG}>"token-ledger Linux"</text>
            <g opacity={if creating { 0.5 } else { 1.0 }}>
                {button(402.0, CONTENT_TOP + 20.0, 150.0, 40.0, if creating { "Creating..." } else { "Create token" }, ButtonStyle::Primary, Some(icons::KEY), None)}
            </g>
            <g font-size="12" font-weight="500" fill={MUTED}>
                <text x="24" y={CONTENT_TOP + 84.0}>"Name it after the machine. A token per machine lets you revoke one machine alone, under API"</text>
                <text x="24" y={CONTENT_TOP + 100.0}>"tokens in Account settings."</text>
            </g>
        </g>
    )
}

/// One numbered command of the third step.
fn command<'a>(y: f32, label: &'a str, lines: Svgr<'a>) -> Svgr<'a> {
    fframes::svgr!(
        <g>
            <text x="36" y={y + 11.0} font-size="12" font-weight="500" fill={MUTED}>{label}</text>
            <g font-family={MONO} font-size="12" font-weight="400" fill={FG}>{lines}</g>
        </g>
    )
}

/// Step 3: the three commands with the server and the token filled in, and Copy all.
fn commands_step<'a>(copied: f32) -> Svgr<'a> {
    let top = CONTENT_TOP;
    let token_x = 36.0 + CHARACTER * 15.0;
    let after_token = token_x + CHARACTER * data::TOKEN.len() as f32;
    fframes::svgr!(
        <g>
            <rect x="24" y={top} width="528" height="228" rx="12" fill="#151922" stroke={BORDER} stroke-width="1" />
            <path d={format!("M24 {} H552", top + 44.0)} stroke={BORDER} stroke-width="1" />
            {mark(data::TUX_PATH, 36.0, top + 15.0, 14.0, MUTED)}
            <text x="58" y={top + 26.5} font-size="12" font-weight="500" fill={MUTED}>"Linux · bash"</text>
            <g opacity={1.0 - copied * 0.25}>
                {button(450.0, top + 6.0, 96.0, 32.0, "Copy all", ButtonStyle::Outline, Some(icons::COPY), None)}
            </g>
            {command(top + 56.0, "1. Save the server and your token", fframes::svgr!(<g>
                <text x="36" y={top + 90.0}>"TOKI_API_TOKEN="</text>
                <text x={token_x} y={top + 90.0} fill={AMBER}>{data::TOKEN}</text>
                <text x={after_token} y={top + 90.0}>" npx --yes"</text>
                <text x="36" y={top + 110.0}>{format!("@ponbac/token-ledger@latest login --server {}", data::SERVER)}</text>
            </g>))}
            {command(top + 126.0, "2. Upload the last 14 days", fframes::svgr!(
                <text x="36" y={top + 160.0}>"npx --yes @ponbac/token-ledger@latest sync"</text>
            ))}
            {command(top + 176.0, "3. Sync every day at 12:30 with a systemd user timer", fframes::svgr!(
                <text x="36" y={top + 210.0}>"npx --yes @ponbac/token-ledger@latest schedule install"</text>
            ))}
            {icon(icons::SHIELD_CHECK, 24.0, top + 240.0, 14.0, AMBER)}
            <g font-size="12" font-weight="500" fill={MUTED}>
                <text x="46" y={top + 251.0}>"Only totals leave the machine: tokens and estimated cost per hour, session, project, tool"</text>
                <text x="46" y={top + 267.0}>"and model, with the machine's name and time zone. A project is its configured name or"</text>
                <text x="46" y={top + 283.0}>"its repository, as host/owner/repo. Never prompts, responses, code or file paths."</text>
                <text x="46" y={top + 299.0}>"Everything that is uploaded."</text>
                <text x="24" y={top + 325.0}>"A missed daily run runs within ten minutes of your next login. Without systemd, as in WSL,"</text>
                <text x="24" y={top + 341.0}>"schedule it with cron instead. Copilot is read only once its file export is on. See the"</text>
                <text x="24" y={top + 357.0}>"setup details."</text>
            </g>
        </g>
    )
}

/// The dialog's surface, title, step line and progress, `height` px tall on step `step`.
pub(crate) fn dialog_shell<'a>(
    height: f32,
    step: usize,
    title: &'a str,
    step_icon: &'static str,
) -> Svgr<'a> {
    let segments: Vec<Svgr> = (0..4)
        .map(|i| {
            let width = (528.0 - 18.0) / 4.0;
            fframes::svgr!(<rect x={24.0 + i as f32 * (width + 6.0)} y="90" width={width} height="4" rx="2" fill={if i <= step { AMBER } else { MUTED_SURFACE }} />)
        })
        .collect();
    fframes::svgr!(
        <g>
            <rect x="0" y="8" width={DIALOG_WIDTH} height={height} rx="14" fill="#000000" opacity="0.4" />
            <rect x="0" y="0" width={DIALOG_WIDTH} height={height} rx="12" fill={BG} stroke={BORDER} stroke-width="1" />
            <text x="24" y="42" font-size="18" font-weight="600" letter-spacing="-0.4" fill={FG}>"Connect a machine"</text>
            {icon(icons::CLOSE, 544.0, 16.0, 16.0, MUTED)}
            {icon(step_icon, 24.0, 55.0, 16.0, AMBER)}
            <text x="48" y="68" font-size="14" font-weight="500" fill={MUTED}>{format!("Step {} of 4 · {}", step + 1, title)}</text>
            {segments}
        </g>
    )
}

impl Scene for Connect {
    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(SECONDS)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::from([
            AudioTrack::new("click.wav", Second(OPEN_AT)..Eof).gain_db(-7.0),
            AudioTrack::new("click.wav", Second(SYSTEM_NEXT_AT)..Eof).gain_db(-7.0),
            AudioTrack::new("click.wav", Second(CREATE_AT)..Eof).gain_db(-7.0),
            AudioTrack::new("click.wav", Second(TOKEN_NEXT_AT)..Eof).gain_db(-7.0),
            AudioTrack::new("chime.wav", Second(COPY_AT)..Eof).gain_db(-7.0),
        ])
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = frame.seconds();
        let (pointer_x, pointer_y) = pointer_at(t);
        let step = if t < SYSTEM_NEXT_AT {
            0
        } else if t < TOKEN_NEXT_AT {
            1
        } else {
            2
        };
        let step_start = [OPEN_AT, SYSTEM_NEXT_AT, TOKEN_NEXT_AT][step];
        let titles = [
            "Pick the machine's system",
            "Create an API token",
            "Run this in a terminal",
        ];
        let step_icons = [icons::LAPTOP, icons::KEY, icons::TERMINAL];
        let height = dialog_height(t);

        let opened = progress(t, OPEN_AT, 0.22);
        let dialog = format!(
            "translate(960 {DIALOG_CENTER_Y}) scale({}) translate({} {})",
            DIALOG_ZOOM * lerp(0.95, 1.0, ease_out(opened)),
            -DIALOG_WIDTH / 2.0,
            -height / 2.0
        );

        let content = match step {
            0 => system_step(),
            1 => token_step(t),
            _ => commands_step(progress(t, COPY_AT, 0.1) * (1.0 - progress(t, COPY_AT + 0.1, 0.3))),
        };
        // Next waits for the token on the second step.
        let next_enabled = step != 1 || t >= READY_AT;
        let next_width = if step == 2 { 126.0 } else { 88.0 };

        let toast = progress(t, COPY_AT + 0.1, 0.25).min(1.0 - progress(t, COPY_AT + 3.4, 0.3));

        fframes::svgr!(
            <g opacity={scene_fade(t, SECONDS)}>
                <g transform={page_transform()}>
                    <g opacity={ramp(&frame, 0.2)} transform={Transform::translate(0, rise(&frame, 0.2, 18.0))}>
                        {preview()}
                    </g>
                    <g opacity={ramp(&frame, 0.5)} transform={Transform::translate(0, rise(&frame, 0.5, 24.0))}>
                        {prompt(progress(t, OPEN_AT - 0.06, 0.06))}
                    </g>
                </g>
                <rect width="1920" height="1080" fill="#000000" opacity={opened * 0.8} />
                {caption_sequence(&frame, "1", "Koppla din dator", &[
                    ("Första gången visar sidan AI usage en guide. Det tar ungefär två minuter.", 0.0),
                    ("Välj system och skapa en API-token. Guiden fyller i server och token i kommandona.", OPEN_AT + 0.3),
                ])}
                <g opacity={opened} transform={dialog} font-family={SANS}>
                    {dialog_shell(height, step, titles[step], step_icons[step])}
                    // The commands wait for the dialog to grow around them.
                    <g opacity={progress(t, step_start + if step == 2 { 0.35 } else { 0.0 }, 0.2)}>{content}</g>
                    <g opacity={if step == 0 { 0.5 } else { 1.0 }}>
                        {button(24.0, height - 64.0, 84.0, 40.0, "Back", ButtonStyle::Ghost, Some(icons::ARROW_LEFT), None)}
                    </g>
                    <g opacity={if next_enabled { 1.0 } else { 0.5 }}>
                        {button(DIALOG_WIDTH - 24.0 - next_width, height - 64.0, next_width, 40.0, if step == 2 { "I ran them" } else { "Next" }, ButtonStyle::Primary, None, Some(icons::ARROW_RIGHT))}
                    </g>
                </g>
                <g opacity={toast} transform={Transform::translate(0, (1.0 - toast) * 16.0)} font-family={SANS}>
                    <rect x="1456" y="964" width="416" height="68" rx="14" fill={CARD} stroke={BORDER} stroke-width="1.5" />
                    <circle cx="1496" cy="998" r="13" fill={OK} />
                    {icon(icons::CHECK, 1487.0, 989.0, 18.0, CARD)}
                    <text x="1524" y="1006" font-size="22" font-weight="600" fill={FG}>"Commands copied"</text>
                </g>
                <g transform={format!("translate({pointer_x} {pointer_y}) scale({ZOOM})")}>
                    {pointer(t, 0.0, 0.0, &[OPEN_AT, SYSTEM_NEXT_AT, CREATE_AT, TOKEN_NEXT_AT, COPY_AT], progress(t, 1.9, 0.3).min(1.0 - progress(t, 17.0, 0.4)))}
                </g>
            </g>
        )
    }
}
