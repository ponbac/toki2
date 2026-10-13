//! The Repositories tab (`repositories-panel.tsx`): the repositories that count for no
//! project yet. One is mapped to its time-tracking project, one is the developer's own and
//! is marked Personal / internal, and the rest are mapped at a quicker pace. Beside the list
//! an explanatory drawing, not a screen of the app, shows the cost moving from Unassigned to
//! the projects as it happens.
use crate::page::{self, Waiting};
use crate::*;

const SECONDS: f32 = 17.0;
const GROUP_Y: f32 = 257.0;
const GROUP_WIDTH: f32 = 598.0;
const LIST_Y: f32 = GROUP_Y + 72.0;
const ITEM: f32 = 128.0;
/// The view while mapping: the groups from their top.
const AT_GROUPS: f32 = GROUP_Y - 10.0;

const SCROLL_AT: f32 = 2.8;
const OPEN_AT: f32 = 4.6;
/// When each repository gets its project chosen and when it is acted on, in the order of
/// `data::UNMAPPED`. The second one is marked personal and has no project to choose.
const PICK_AT: [f32; 5] = [5.8, f32::MAX, 11.3, 12.25, 13.2];
const ACT_AT: [f32; 5] = [6.8, 9.3, 11.75, 12.7, 13.65];
const PERSONAL: usize = 1;
const DONE_AT: f32 = 14.3;

/// The drawing of where the cost lies, in video pixels.
const COSTS_X: f32 = 1000.0;
const COSTS_Y: f32 = 241.0;
const COSTS_WIDTH: f32 = 872.0;
const COST_ROW: f32 = 74.0;
/// The cost a full-width bar stands for, in USD.
const COSTS_MAX: f32 = 4_200.0;

/// The projects the combobox offers, by their row in `data::PROJECTS`, and the one picked.
const OPTIONS: [usize; 5] = [1, 2, 3, 0, 4];
const PICKED_OPTION: usize = 3;
const OPTIONS_Y: f32 = LIST_Y + 66.0;
const OPTION: f32 = 32.0;

#[derive(Debug)]
pub struct Repositories;

/// How far repository `index` is into giving up its place in the list after it was acted
/// on. It has faded out before the ones below it start to move up, so no two overlap.
fn gone(t: f32, index: usize) -> f32 {
    ease_in_out(progress(t, ACT_AT[index] + 0.4, 0.4))
}

/// How much of repository `index` is still visible.
fn visible(t: f32, index: usize) -> f32 {
    1.0 - progress(t, ACT_AT[index] + 0.2, 0.2)
}

/// A group of repositories (`KeyGroup`): its mark, name and count, and a note.
#[allow(clippy::too_many_arguments)]
fn group<'a>(
    x: f32,
    height: f32,
    glyph: &'static str,
    title: &'a str,
    count: usize,
    attention: bool,
    notes: &[&'a str],
) -> Svgr<'a> {
    let ink = if attention { AMBER } else { FG };
    let count = count.to_string();
    let chip = count.len() as f32 * 6.6 + 16.0;
    let notes: Vec<Svgr> = notes
        .iter()
        .enumerate()
        .map(|(line, note)| fframes::svgr!(<text x={x + 20.0} y={GROUP_Y + 56.0 + line as f32 * 16.0} font-size="12" font-weight="500" fill={MUTED}>{*note}</text>))
        .collect();
    fframes::svgr!(
        <g font-family={SANS}>
            <rect x={x} y={GROUP_Y} width={GROUP_WIDTH} height={height} rx="16" fill={CARD} stroke={BORDER} stroke-width="1" />
            <rect x={x} y={GROUP_Y} width={GROUP_WIDTH} height={height} rx="16" fill="none" stroke={AMBER} stroke-width="1" opacity={if attention { 0.5 } else { 0.0 }} />
            {icon(glyph, x + 20.0, GROUP_Y + 21.0, 16.0, if attention { AMBER } else { MUTED })}
            <text x={x + 44.0} y={GROUP_Y + 34.0} font-size="14" font-weight="600" fill={ink}>{title}</text>
            <rect x={x + GROUP_WIDTH - 20.0 - chip} y={GROUP_Y + 20.5} width={chip} height="19" rx="9.5" fill={if attention { AMBER } else { MUTED_SURFACE }} opacity={if attention { 0.1 } else { 1.0 }} />
            <text x={x + GROUP_WIDTH - 20.0 - chip / 2.0} y={GROUP_Y + 34.0} text-anchor="middle" font-size="11" font-weight="600" fill={if attention { AMBER } else { MUTED }}>{count}</text>
            {notes}
        </g>
    )
}

/// One repository waiting to be mapped (`KeyItem`): its key, when it was last used, the
/// project to map it to, and the way to mark it personal instead.
fn waiting<'a>(
    y: f32,
    index: usize,
    chosen: Option<&'a str>,
    focused: f32,
    mapping: f32,
    marking: f32,
) -> Svgr<'a> {
    let (key, last_used, _, _) = data::UNMAPPED[index];
    fframes::svgr!(
        <g font-family={SANS}>
            {page::repository(key, 20.0, y + 15.0, 14.0, MONO, FG)}
            <text x="578" y={y + 15.0} text-anchor="end" font-size="12" font-weight="500" fill={MUTED}>{last_used}</text>
            <rect x="20" y={y + 26.0} width="498" height="36" rx="9" fill={BG} stroke={BORDER} stroke-width="1" />
            <rect x="20" y={y + 26.0} width="498" height="36" rx="9" fill="none" stroke={AMBER} stroke-width="1.5" opacity={focused * 0.8} />
            <text x="33" y={y + 49.0} font-size="14" font-weight="500" fill={if chosen.is_some() { FG } else { MUTED }}>{chosen.unwrap_or("Choose project")}</text>
            {icon(icons::CHEVRONS_UP_DOWN, 490.0, y + 36.0, 16.0, MUTED)}
            <g opacity={if chosen.is_some() { 1.0 - mapping * 0.3 } else { 0.5 }}>
                {button(526.0, y + 26.0, 52.0, 36.0, "Map", ButtonStyle::Primary, None, None)}
            </g>
            <g opacity={1.0 - marking * 0.3}>
                {button(20.0, y + 68.0, 196.0, 36.0, "Mark as Personal / internal", ButtonStyle::Outline, None, None)}
            </g>
            <path d={format!("M20 {} H578", y + 116.0)} stroke={BORDER} stroke-width="1" opacity="0.6" />
        </g>
    )
}

/// A toast in the corner of the frame, with one or two lines under its headline.
fn toast<'a>(shown: f32, title: String, detail: &'a str) -> Svgr<'a> {
    fframes::svgr!(
        <g opacity={shown} transform={Transform::translate(0, (1.0 - shown) * 16.0)} font-family={SANS}>
            <rect x="1172" y="950" width="700" height="92" rx="14" fill={CARD} stroke={BORDER} stroke-width="1.5" />
            <circle cx="1212" cy="996" r="13" fill={OK} />
            {icon(icons::CHECK, 1203.0, 987.0, 18.0, CARD)}
            <text x="1240" y="988" font-size="21" font-weight="600" fill={FG}>{title}</text>
            <text x="1240" y="1018" font-size="19" font-weight="500" fill={MUTED}>{detail}</text>
        </g>
    )
}

/// One row of the drawing: a name, its bar and its cost in kronor. `gained` of the cost has
/// just arrived and is shown in the accent colour while `flash` lasts.
#[allow(clippy::too_many_arguments)]
fn cost_row<'a>(
    row: usize,
    name: &'a str,
    ink: &'a str,
    bar: &'a str,
    cost: f32,
    gained: f32,
    flash: f32,
    shown: f32,
) -> Svgr<'a> {
    let y = COSTS_Y + 118.0 + row as f32 * COST_ROW;
    let full = COSTS_WIDTH - 80.0;
    let width = (full * cost / COSTS_MAX).max(0.01);
    let fresh = (full * gained / COSTS_MAX).min(width).max(0.01);
    // A cost on its way to nothing counts down in whole kronor.
    let amount = if cost * money::SEK_PER_USD < 100.0 {
        format!("{} kr", (cost * money::SEK_PER_USD).round())
    } else {
        money::shown(Currency::Sek, cost)
    };
    fframes::svgr!(
        <g opacity={shown}>
            <text x={COSTS_X + 40.0} y={y + 24.0} font-size="24" font-weight="500" fill={ink}>{name}</text>
            <text x={COSTS_X + COSTS_WIDTH - 40.0} y={y + 24.0} text-anchor="end" font-size="24" font-weight="600" fill={FG}>{amount}</text>
            <rect x={COSTS_X + 40.0} y={y + 38.0} width={full} height="10" rx="5" fill={MUTED_SURFACE} />
            <rect x={COSTS_X + 40.0} y={y + 38.0} width={width} height="10" rx="5" fill={bar} />
            <rect x={COSTS_X + 40.0 + width - fresh} y={y + 38.0} width={fresh} height="10" rx="5" fill={AMBER} opacity={flash} />
        </g>
    )
}

impl Scene for Repositories {
    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(SECONDS)
    }

    fn audio(&self) -> AudioMap<'_> {
        let mut tracks = vec![
            AudioTrack::new("click.wav", Second(OPEN_AT)..Eof).gain_db(-7.0),
            AudioTrack::new("click.wav", Second(PICK_AT[0])..Eof).gain_db(-7.0),
        ];
        for (index, at) in ACT_AT.iter().enumerate() {
            tracks.push(AudioTrack::new("click.wav", Second(*at)..Eof).gain_db(-7.0));
            if index < 2 {
                tracks.push(AudioTrack::new("chime.wav", Second(at + 0.25)..Eof).gain_db(-6.0));
            }
        }
        tracks.push(AudioTrack::new("chime.wav", Second(ACT_AT[4] + 0.3)..Eof).gain_db(-5.0));
        AudioMap(Some(tracks))
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = frame.seconds();
        let scroll = lerp(
            page::AT_TABS,
            AT_GROUPS,
            ease_in_out(progress(t, SCROLL_AT, 1.0)),
        );

        // What is left of each repository's place in the list, and how many still wait.
        let left: Vec<f32> = (0..data::UNMAPPED.len())
            .map(|index| 1.0 - gone(t, index))
            .collect();
        let waiting_count = ACT_AT.iter().filter(|at| t < **at + 0.25).count();
        let attention = waiting_count > 0;
        let to_map_height = (68.0 + ITEM * left.iter().sum::<f32>()).max(80.0);

        let mut y = LIST_Y;
        let items: Vec<Svgr> = (0..data::UNMAPPED.len())
            .map(|index| {
                let at = y;
                y += ITEM * left[index];
                if left[index] <= 0.0 {
                    return Svgr::empty();
                }
                let chosen = (t >= PICK_AT[index])
                    .then(|| data::UNMAPPED[index].3.map(|project| data::PROJECTS[project].0))
                    .flatten();
                let focused = if index == 0 && (OPEN_AT..PICK_AT[0]).contains(&t) { 1.0 } else { 0.0 };
                let acted = progress(t, ACT_AT[index], 0.1);
                let (mapping, marking) = if index == PERSONAL { (0.0, acted) } else { (acted, 0.0) };
                fframes::svgr!(<g opacity={visible(t, index)}>{waiting(at, index, chosen, focused, mapping, marking)}</g>)
            })
            .collect();

        // The combobox of the first repository, open until its project is picked.
        let open = progress(t, OPEN_AT, 0.15) * (1.0 - progress(t, PICK_AT[0], 0.12));
        let options: Vec<Svgr> = OPTIONS
            .iter()
            .enumerate()
            .map(|(row, project)| {
                let at = OPTIONS_Y + 44.0 + row as f32 * OPTION;
                let hovered = row == PICKED_OPTION && t >= OPEN_AT + 0.75;
                fframes::svgr!(<g>
                    <rect x="24" y={at} width="490" height={OPTION} rx="6" fill={HOVER} opacity={if hovered { 1.0 } else { 0.0 }} />
                    <text x="36" y={at + 21.0} font-size="14" font-weight="500" fill={FG}>{data::PROJECTS[*project].0}</text>
                </g>)
            })
            .collect();

        // The pointer keeps its place on screen while the page scrolls under it.
        let first = (LIST_Y - AT_GROUPS) + 44.0;
        let (pointer_x, pointer_y) = page::travel(
            t,
            &[
                (560.0, 470.0),
                (270.0, first),
                (
                    270.0,
                    OPTIONS_Y - AT_GROUPS + 60.0 + PICKED_OPTION as f32 * OPTION,
                ),
                (552.0, first),
                (118.0, first + 42.0),
                (552.0, first),
                (500.0, 300.0),
            ],
            &[
                (3.7, 0.8),
                (5.0, 0.7),
                (6.1, 0.6),
                (8.3, 0.9),
                (10.3, 0.9),
                (DONE_AT, 1.0),
            ],
        );
        let clicks = [
            OPEN_AT, PICK_AT[0], ACT_AT[0], ACT_AT[1], ACT_AT[2], ACT_AT[3], ACT_AT[4],
        ];

        // The toasts of the two repositories that are handled at a readable pace.
        let toasts = [
            (
                ACT_AT[0] + 0.25,
                format!("{} now counts for", data::UNMAPPED[0].0),
                data::PROJECTS[0].0,
            ),
            (
                ACT_AT[1] + 0.25,
                format!("{} is now {}", data::UNMAPPED[PERSONAL].0, data::PERSONAL),
                "Your usage of it is left out of the bill.",
            ),
        ];
        let toasts: Vec<Svgr> = toasts
            .into_iter()
            .map(|(at, title, detail)| {
                toast(
                    progress(t, at, 0.25).min(1.0 - progress(t, at + 2.3, 0.3)),
                    title,
                    detail,
                )
            })
            .collect();

        // Where the cost lies: each repository's cost leaves Unassigned as it is acted on.
        let moved: Vec<f32> = ACT_AT
            .iter()
            .map(|at| ease_out(progress(t, at + 0.25, 0.6)))
            .collect();
        let flash = |index: usize| {
            progress(t, ACT_AT[index] + 0.25, 0.01) * (1.0 - progress(t, ACT_AT[index] + 0.9, 0.8))
        };
        let mut rows: Vec<Svgr> = data::PROJECTS
            .iter()
            .enumerate()
            .map(|(project, (name, cost))| {
                let from = (0..data::UNMAPPED.len())
                    .find(|index| data::UNMAPPED[*index].3 == Some(project));
                let gained = from.map_or(0.0, |index| data::UNMAPPED[index].2 * moved[index]);
                cost_row(
                    project,
                    name,
                    FG,
                    "#8d8d8b",
                    cost + gained,
                    gained,
                    from.map_or(0.0, flash),
                    1.0,
                )
            })
            .collect();
        let unassigned: f32 = data::UNMAPPED
            .iter()
            .zip(&moved)
            .map(|(repository, moved)| repository.2 * (1.0 - moved))
            .sum();
        rows.push(cost_row(
            5,
            "Unassigned",
            AMBER,
            OTHER,
            unassigned,
            0.0,
            0.0,
            lerp(1.0, 0.4, progress(t, ACT_AT[4] + 0.6, 0.5)),
        ));
        let personal = data::UNMAPPED[PERSONAL].2 * moved[PERSONAL];
        rows.push(cost_row(
            6,
            "Personal / internal, utanför fakturan",
            MUTED,
            OTHER,
            personal,
            personal,
            flash(PERSONAL),
            progress(t, ACT_AT[PERSONAL] + 0.25, 0.4),
        ));
        let costs = ramp(&frame, SCROLL_AT + 0.5);

        fframes::svgr!(
            <g opacity={scene_fade(t, SECONDS)}>
                {caption_sequence(&frame, "6", "Koppla repon till projekt", &[
                    ("Fliken Repositories visar de repon som inte hör till något projekt än.", 0.0),
                    ("Välj projekt och tryck Map. Kopplingen gäller allas användning av repot, även bakåt.", OPEN_AT - 0.5),
                    ("Egna repon markerar du Personal / internal. De hålls utanför fakturan, och admin ser inte namnet.", ACT_AT[PERSONAL] - 0.9),
                    ("Resten går fort.", PICK_AT[2] - 0.4),
                    ("Unassigned är borta. Kostnaden ligger på rätt projekt.", DONE_AT),
                ])}
                <g opacity={ramp(&frame, 0.2)} transform={Transform::translate(0, rise(&frame, 0.2, 18.0))}>
                    {page::view(scroll, fframes::svgr!(<g font-family={SANS}>
                        {page::tabs(2.0, &Waiting { to_map: waiting_count, ..Waiting::all() })}
                        <g font-size="14" font-weight="500" fill={MUTED}>
                            <text x="0" y="172">"Usage of these repositories counts for no time-tracking project. A repository without a mapping shows its name only"</text>
                            <text x="0" y="192">"to the developers who use it. Mapping it shows the name to admins and applies to everyone’s usage of it, including"</text>
                            <text x="0" y="212">"past usage. One that is not client work can be marked Personal / internal instead: your usage of it is left out of the bill,"</text>
                            <text x="0" y="232">"and nothing is mapped, so admins do not see its name."</text>
                        </g>
                        {group(0.0, to_map_height, icons::CIRCLE_DASHED, "To map", waiting_count, attention, &[if attention { "Once a repository is mapped, only an admin can change it, so check the project first." } else { "No repository is waiting to be mapped." }])}
                        {items}
                        <g opacity={open}>
                            <rect x="22" y={OPTIONS_Y + 4.0} width="498" height={52.0 + OPTION * 5.0} rx="10" fill="#000000" opacity="0.35" />
                            <rect x="20" y={OPTIONS_Y} width="498" height={52.0 + OPTION * 5.0} rx="10" fill={POPOVER} stroke={BORDER} stroke-width="1" />
                            {icon(icons::SEARCH, 33.0, OPTIONS_Y + 13.0, 15.0, MUTED)}
                            <text x="58" y={OPTIONS_Y + 26.0} font-size="14" font-weight="500" fill={MUTED}>"Search projects..."</text>
                            <path d={format!("M20 {} H518", OPTIONS_Y + 40.0)} stroke={BORDER} stroke-width="1" />
                            {options}
                        </g>
                        {pointer(t, pointer_x, pointer_y + scroll, &clicks, progress(t, 3.4, 0.3).min(1.0 - progress(t, SECONDS - 1.6, 0.4)))}
                    </g>))}
                </g>
                <g opacity={costs} transform={Transform::translate(rise(&frame, SCROLL_AT + 0.5, 40.0), 0)} font-family={SANS}>
                    <rect x={COSTS_X} y={COSTS_Y} width={COSTS_WIDTH} height={118.0 + 7.0 * COST_ROW + 16.0} rx="24" fill={CARD} stroke={BORDER} stroke-width="1.5" />
                    <text x={COSTS_X + 40.0} y={COSTS_Y + 58.0} font-size="32" font-weight="600" letter-spacing="-0.6" fill={FG}>"Var kostnaden ligger"</text>
                    <text x={COSTS_X + 40.0} y={COSTS_Y + 90.0} font-size="20" font-weight="500" fill={MUTED}>"Den flyttar från Unassigned när ett repo kopplas."</text>
                    {rows}
                </g>
                {toasts}
            </g>
        )
    }
}
