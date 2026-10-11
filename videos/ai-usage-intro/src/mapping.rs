//! Unassigned repositories. A pointer maps the first one to a project by hand and marks the
//! second, which is not client work, as Personal / internal; the rest are mapped faster and
//! faster. Each one leaves the list and its cost moves in the chart beside it, until
//! Unassigned is gone.
use crate::*;

const SECONDS: f32 = 15.5;
const CARD_WIDTH: f32 = 598.0;
const CARD_HEIGHT: f32 = 548.0;
const ROWS_TOP: f32 = 178.0;
const ROW: f32 = 114.0;

const OPEN_AT: f32 = 1.9;
/// When each repository's project is chosen, and when it is mapped or marked.
const PICK_AT: [f32; 5] = [3.05, f32::MAX, 8.35, 9.2, 9.9];
const ACT_AT: [f32; 5] = [4.0, 6.4, 8.8, 9.6, 10.25];
const PERSONAL: usize = 1;
const DONE_AT: f32 = 11.2;
/// The option the pointer picks: the first project of `data::PROJECTS`, fourth in the list.
const PICKED_OPTION: usize = 3;
const OPTIONS: [usize; 5] = [1, 2, 3, 0, 4];
const OPTIONS_TOP: f32 = ROWS_TOP + 64.0;
const OPTION: f32 = 32.0;

const CHART_X: f32 = CARD_WIDTH + 20.0;
const BAR_LEFT: f32 = 224.0;
const BAR_SCALE: f32 = 250.0 / 4200.0;
const BARS_TOP: f32 = 96.0;
const BAR_PITCH: f32 = 58.0;

#[derive(Debug)]
pub struct Mapping;

/// Where the pointer is at `t`, in UI pixels: it rests, then eases between its targets. The
/// next repository always moves up under it, so it only travels between the controls.
fn pointer_at(t: f32) -> (f32, f32) {
    const MAP: (f32, f32) = (554.0, ROWS_TOP + 42.0);
    const STOPS: [(f32, f32); 7] = [
        (720.0, 440.0),
        (300.0, ROWS_TOP + 42.0),
        (
            330.0,
            OPTIONS_TOP + 4.0 + PICKED_OPTION as f32 * OPTION + 18.0,
        ),
        MAP,
        (126.0, ROWS_TOP + 84.0),
        MAP,
        (700.0, 380.0),
    ];
    const MOVES: [(f32, f32); 6] = [
        (0.9, 0.85),
        (2.3, 0.6),
        (3.3, 0.6),
        (5.3, 0.8),
        (7.6, 0.7),
        (11.0, 0.9),
    ];
    let mut position = STOPS[0];
    for (i, (start, duration)) in MOVES.iter().enumerate() {
        let p = ease_in_out(progress(t, *start, *duration));
        position = (
            lerp(STOPS[i].0, STOPS[i + 1].0, p),
            lerp(STOPS[i].1, STOPS[i + 1].1, p),
        );
        if p < 1.0 {
            break;
        }
    }
    position
}

/// One unmapped repository: its key, when it was last used, a project select with Map, and
/// the way to mark it as not client work.
fn repository<'a>(
    y: f32,
    key: &'a str,
    last_used: &'a str,
    selected: Option<&'a str>,
    focused: f32,
    marking: f32,
) -> Svgr<'a> {
    fframes::svgr!(
        <g>
            <text x="20" y={y + 13.0} font-family={MONO} font-size="13" font-weight="500" fill={FG}>{key}</text>
            <text x="578" y={y + 13.0} text-anchor="end" font-size="11" font-weight="500" fill={MUTED}>{last_used}</text>
            <rect x="20" y={y + 24.0} width="500" height="36" rx="8" fill={BG} stroke={BORDER} stroke-width="1" />
            <rect x="20" y={y + 24.0} width="500" height="36" rx="8" fill="none" stroke={AMBER} stroke-width="1.5" opacity={focused * 0.8} />
            <text x="34" y={y + 46.5} font-size="13" font-weight="500" fill={FG}>{selected.unwrap_or("Choose project")}</text>
            <path transform={format!("translate(502 {})", y + 42.0)} d="M-3.5 -2 L0 -5.5 L3.5 -2 M-3.5 2 L0 5.5 L3.5 2" fill="none" stroke={MUTED} stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
            <rect x="530" y={y + 24.0} width="48" height="36" rx="8" fill={AMBER} opacity={if selected.is_some() { 1.0 } else { 0.5 }} />
            <text x="554" y={y + 46.5} text-anchor="middle" font-size="13" font-weight="600" fill={BG}>"Map"</text>
            <rect x="20" y={y + 66.0} width="204" height="32" rx="8" fill={BG} stroke={BORDER} stroke-width="1" />
            <rect x="20" y={y + 66.0} width="204" height="32" rx="8" fill={HOVER} opacity={marking} />
            <text x="122" y={y + 86.5} text-anchor="middle" font-size="13" font-weight="500" fill={FG}>{if marking > 0.5 { "Marking..." } else { "Mark as Personal / internal" }}</text>
            <path d={format!("M20 {} H578", y + 106.0)} stroke={BORDER} stroke-width="1" opacity="0.6" />
        </g>
    )
}

/// How much of a step from 0 to 1 `value` has made, eased, over `width` centered on 0.
fn step(value: f32, width: f32) -> f32 {
    ease_in_out((value / width + 0.5).clamp(0.0, 1.0))
}

/// A bar of the project chart with its label and value; `gained` px at its end flash amber.
fn chart_bar<'a>(
    rank: f32,
    label: &'a str,
    cost: f32,
    grow: f32,
    color: &'a str,
    gained: f32,
    flash: f32,
) -> Svgr<'a> {
    let y = BARS_TOP + rank * BAR_PITCH;
    let width = (cost * BAR_SCALE * grow).max(0.5);
    let gained = gained.clamp(0.5, width);
    fframes::svgr!(<g>
        <text x={BAR_LEFT - 10.0} y={y + 16.0} text-anchor="end" font-size="11" font-weight="500" fill={MUTED}>{label}</text>
        <rect x={BAR_LEFT} y={y} width={width} height="24" rx="4" fill={color} />
        <rect x={BAR_LEFT + width - gained} y={y} width={gained} height="24" rx="4" fill={AMBER} opacity={flash} />
        <text x={BAR_LEFT + width + 8.0} y={y + 16.0} font-size="11" font-weight="500" fill={FG} opacity={grow}>{usd(cost * grow)}</text>
    </g>)
}

impl Scene for Mapping {
    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(SECONDS)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::from([
            AudioTrack::new("click.wav", Second(OPEN_AT)..Eof).gain_db(-7.0),
            AudioTrack::new("click.wav", Second(PICK_AT[0])..Eof).gain_db(-7.0),
            AudioTrack::new("chime.wav", Second(ACT_AT[0])..Eof).gain_db(-5.0),
            AudioTrack::new("chime.wav", Second(ACT_AT[1])..Eof).gain_db(-6.0),
            AudioTrack::new("click.wav", Second(ACT_AT[2])..Eof).gain_db(-7.0),
            AudioTrack::new("click.wav", Second(ACT_AT[3])..Eof).gain_db(-7.0),
            AudioTrack::new("chime.wav", Second(ACT_AT[4])..Eof).gain_db(-5.0),
        ])
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = frame.seconds();
        let (pointer_x, pointer_y) = pointer_at(t);
        let open = progress(t, OPEN_AT, 0.14).min(1.0 - progress(t, PICK_AT[0], 0.12));

        // A handled repository leaves, and the ones below close the gap. Those that do not
        // fit the card yet are below its fold.
        let closed = |repository: usize| ease_in_out(progress(t, ACT_AT[repository] + 0.3, 0.45));
        let repositories: Vec<Svgr> = data::UNMAPPED
            .iter()
            .enumerate()
            .rev()
            .map(|(i, (key, last_used, _, project))| {
                let above: f32 = (0..i).map(closed).sum();
                let y = ROWS_TOP + (i as f32 - above) * ROW;
                let selected = project
                    .filter(|_| t >= PICK_AT[i])
                    .map(|project| data::PROJECTS[project].0);
                // The first select is focused while its list is open; the others as they fill.
                let focused = if i == 0 {
                    open
                } else {
                    progress(t, PICK_AT[i] - 0.1, 0.1) * (1.0 - progress(t, PICK_AT[i] + 0.1, 0.3))
                };
                let marking = if i == PERSONAL {
                    progress(t, ACT_AT[i], 0.05)
                } else {
                    0.0
                };
                let fits = 1.0 - progress(y, ROWS_TOP + 2.0 * ROW + 6.0, 30.0);
                fframes::svgr!(<g opacity={(1.0 - progress(t, ACT_AT[i] + 0.1, 0.25)) * fits}>
                    {repository(y, key, last_used, selected, focused, marking)}
                </g>)
            })
            .collect();

        let options: Vec<Svgr> = OPTIONS
            .iter()
            .enumerate()
            .map(|(i, project)| {
                let y = OPTIONS_TOP + 4.0 + i as f32 * OPTION;
                let hovered = pointer_y >= y && pointer_y < y + OPTION && t > OPEN_AT;
                fframes::svgr!(<g>
                    <rect x="24" y={y} width="492" height={OPTION} rx="6" fill={HOVER} opacity={if hovered { 1.0 } else { 0.0 }} />
                    <text x="36" y={y + 21.0} font-size="13" font-weight="500" fill={FG}>{data::PROJECTS[*project].0}</text>
                </g>)
            })
            .collect();

        // The chart: each handled cost moves from Unassigned to its project, or to Personal.
        let moved: Vec<f32> = (0..5)
            .map(|i| ease_in_out(progress(t, ACT_AT[i] + 0.2, 0.7)) * data::UNMAPPED[i].2)
            .collect();
        let flash = |i: usize| {
            progress(t, ACT_AT[i] + 0.2, 0.15) * (1.0 - progress(t, ACT_AT[i] + 1.3, 1.0))
        };
        let unassigned = (data::UNASSIGNED_COST - moved.iter().sum::<f32>()).max(0.0);
        let personal = moved[PERSONAL];
        // What each project gained, and from which repository.
        let gains: Vec<(f32, f32)> = (0..5)
            .map(|project| {
                data::UNMAPPED
                    .iter()
                    .position(|repository| repository.3 == Some(project))
                    .map_or((0.0, 0.0), |i| (moved[i], flash(i)))
            })
            .collect();
        let costs: Vec<f32> = (0..5).map(|i| data::PROJECTS[i].1 + gains[i].0).collect();
        // Unassigned ranks above what it still outweighs, and slides past the others.
        let outweighs: Vec<f32> = costs
            .iter()
            .map(|cost| step(unassigned - cost, 320.0))
            .collect();
        let outweighs_personal = step(unassigned - personal, 200.0);
        let unassigned_rank: f32 =
            outweighs.iter().map(|above| 1.0 - above).sum::<f32>() + (1.0 - outweighs_personal);

        let project_bars: Vec<Svgr> = (0..5)
            .map(|i| {
                let grow = ease_out(progress(t, 0.5 + i as f32 * 0.07, 0.7));
                chart_bar(
                    i as f32 + outweighs[i],
                    data::PROJECTS[i].0,
                    costs[i],
                    grow,
                    PROVIDER_COLORS[0],
                    gains[i].0 * BAR_SCALE,
                    gains[i].1,
                )
            })
            .collect();

        let toast = progress(t, ACT_AT[PERSONAL] + 0.15, 0.25)
            .min(1.0 - progress(t, ACT_AT[PERSONAL] + 2.0, 0.3));
        let done = ramp(&frame, DONE_AT);

        fframes::svgr!(
            <g opacity={scene_fade(t, SECONDS)}>
                {caption_sequence(&frame, "5", "Koppla repon till projekt", &[
                    ("Unassigned är repon utan projekt. Välj tidrapporteringsprojekt, så hamnar kostnaden rätt.", 0.0),
                    ("Inte kundarbete? Markera repot som Personal / internal i stället.", 5.0),
                    ("Kopplingen gäller hela teamets användning av repot, även bakåt i tiden.", 8.1),
                ])}
                <g transform={page_transform()} font-family={SANS}>
                    <g opacity={ramp(&frame, 0.2)} transform={Transform::translate(0, rise(&frame, 0.2, 18.0))}>
                        {card(0.0, 0.0, CARD_WIDTH, CARD_HEIGHT, "Unassigned repositories", "Usage of these project keys counts for no time-tracking project. A key without a mapping")}
                        <g font-size="12" font-weight="500" fill={MUTED}>
                            <text x="20" y="71">"shows its name only to the developers who use it. Mapping it shows the name to admins and"</text>
                            <text x="20" y="87">"applies to everyone's usage of the key, including past usage. A repository that is not client"</text>
                            <text x="20" y="103">"work can be marked Personal / internal instead: your usage of it is left out of the bill, and"</text>
                            <text x="20" y="119">"nothing is mapped, so admins do not see its name."</text>
                            <text x="20" y="143">"You can map a key that has no mapping yet. Once it is mapped, only an admin can change it,"</text>
                            <text x="20" y="159">"so check the project first."</text>
                        </g>
                        {repositories}
                        <g opacity={done}>
                            <text x="20" y={ROWS_TOP + 14.0} font-size="14" font-weight="500" fill={MUTED}>"No repository is waiting to be mapped."</text>
                            <path d={format!("M20 {} H578", ROWS_TOP + 34.0)} stroke={BORDER} stroke-width="1" />
                            <text x="20" y={ROWS_TOP + 62.0} font-size="14" font-weight="500" fill={FG}>{data::PERSONAL}</text>
                            <text x="20" y={ROWS_TOP + 84.0} font-size="12" font-weight="500" fill={MUTED}>"Your usage of these is left out of the bill. Their names are shown only to the developers"</text>
                            <text x="20" y={ROWS_TOP + 100.0} font-size="12" font-weight="500" fill={MUTED}>"who use them. If one of them maps a repository to a project, that applies to you too."</text>
                            <text x="20" y={ROWS_TOP + 132.0} font-family={MONO} font-size="14" font-weight="500" fill={FG}>{data::UNMAPPED[PERSONAL].0}</text>
                            <text x="566" y={ROWS_TOP + 132.0} text-anchor="end" font-size="13" font-weight="500" fill={FG}>"Undo"</text>
                        </g>
                    </g>
                    <g opacity={ramp(&frame, 0.32)} transform={Transform::translate(0, rise(&frame, 0.32, 18.0))}>
                        <g transform={Transform::translate(CHART_X, 0)}>
                            {card(0.0, 0.0, CARD_WIDTH, CARD_HEIGHT, "Spend per project", "API-equivalent estimate in USD. ? marks usage without a known price.")}
                            {segmented(CARD_WIDTH - 20.0, 20.0, "Spend", "Tokens", 52.0, 58.0, 0.0)}
                            {project_bars}
                            <g opacity={(unassigned / 80.0).min(1.0)}>
                                {chart_bar(unassigned_rank, "Unassigned", unassigned, ease_out(progress(t, 0.71, 0.7)), OTHER, 0.0, 0.0)}
                            </g>
                            <g opacity={progress(t, ACT_AT[PERSONAL] + 0.2, 0.3)}>
                                {chart_bar(5.0 + outweighs_personal, data::PERSONAL, personal, 1.0, OTHER, personal * BAR_SCALE, flash(PERSONAL))}
                            </g>
                            <g opacity={done}>
                                <path d="M20 500 H578" stroke={BORDER} stroke-width="1" />
                                <circle cx="27" cy="521" r="4" fill={OK} />
                                <text x="40" y="525.5" font-size="13" font-weight="600" fill={FG}>"Unassigned är borta. Kostnaden ligger på rätt projekt."</text>
                            </g>
                        </g>
                    </g>
                    <g opacity={open} transform={Transform::translate(0, (1.0 - open) * -4.0)}>
                        <rect x="22" y={OPTIONS_TOP + 4.0} width="500" height={OPTION * 5.0 + 8.0} rx="10" fill="#000000" opacity="0.35" />
                        <rect x="20" y={OPTIONS_TOP} width="500" height={OPTION * 5.0 + 8.0} rx="10" fill={POPOVER} stroke={BORDER} stroke-width="1" />
                        {options}
                    </g>
                    {pointer(t, pointer_x, pointer_y, &[OPEN_AT, PICK_AT[0], ACT_AT[0], ACT_AT[1], ACT_AT[2], ACT_AT[3], ACT_AT[4]], progress(t, 0.6, 0.3).min(1.0 - progress(t, 11.7, 0.4)))}
                </g>
                <g opacity={toast} transform={Transform::translate(0, (1.0 - toast) * 16.0)} font-family={SANS}>
                    <rect x="1182" y="946" width="690" height="92" rx="14" fill={CARD} stroke={BORDER} stroke-width="1.5" />
                    <circle cx="1222" cy="992" r="13" fill={OK} />
                    {icon(icons::CHECK, 1213.0, 983.0, 18.0, CARD)}
                    <text x="1250" y="986" font-size="21" font-weight="600" fill={FG}>{format!("{} is now {}", data::UNMAPPED[PERSONAL].0, data::PERSONAL)}</text>
                    <text x="1250" y="1014" font-size="19" font-weight="500" fill={MUTED}>"Your usage of it is left out of the bill."</text>
                </g>
            </g>
        )
    }
}
