//! The hour-of-week heatmap fills in as a wave from Monday morning, and the sessions table
//! under it lists what the hours are made of.
use crate::*;

const SECONDS: f32 = 10.5;
const CARD_HEIGHT: f32 = 286.0;
const GRID_LEFT: f32 = 62.0;
const GRID_TOP: f32 = 96.0;
const CELL_WIDTH: f32 = 44.2;
const CELL_HEIGHT: f32 = 20.0;
const GAP: f32 = 3.0;
const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

const SESSIONS_Y: f32 = CARD_HEIGHT + 14.0;
const SESSIONS_AT: f32 = 3.6;
/// A row of the sessions table.
struct Session {
    hours: &'static str,
    project: &'static str,
    /// The line under the project: its repository, or why it has no project.
    under: &'static str,
    /// The tool, as an index into `PROVIDER_NAMES`.
    tool: usize,
    model: &'static str,
    machine: &'static str,
    tokens: &'static str,
    cost: &'static str,
}

const SESSIONS: [Session; 3] = [
    Session {
        hours: "11 Oct, 16:00–18:00",
        project: "Nordvik Energi – Kundportal",
        under: "github.com/spinit-ab/nordvik-kundportal",
        tool: 1,
        model: "claude-opus-5-5",
        machine: "jobbdator",
        tokens: "66.1M",
        cost: "$19.59",
    },
    Session {
        hours: "11 Oct, 14:00–16:00",
        project: "Fjällbo Logistik – Ruttplanering",
        under: "github.com/spinit-ab/fjallbo-rutter",
        tool: 0,
        model: "gpt-5.6-sol",
        machine: "jobbdator",
        tokens: "41.8M",
        cost: "$12.40",
    },
    Session {
        hours: "11 Oct, 13:00–15:00",
        project: "github.com/spinit-ab/nordvik-portal-api",
        under: "Unassigned",
        tool: 3,
        model: "gpt-6-astra",
        machine: "hemdator",
        tokens: "19.6M",
        cost: "$7.54",
    },
];

#[derive(Debug)]
pub struct Heatmap {
    hours: [[f32; 24]; 7],
    max: f32,
}

impl Heatmap {
    pub fn new() -> Self {
        let hours = data::hours();
        let max = hours.iter().flatten().copied().fold(0.0, f32::max);
        Self { hours, max }
    }
}

impl Scene for Heatmap {
    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(SECONDS)
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = frame.seconds();

        let cells: Vec<Svgr> = self
            .hours
            .iter()
            .enumerate()
            .flat_map(|(weekday, row)| row.iter().enumerate().map(move |(hour, cost)| (weekday, hour, *cost)))
            .map(|(weekday, hour, cost)| {
                let x = GRID_LEFT + hour as f32 * (CELL_WIDTH + GAP);
                let y = GRID_TOP + weekday as f32 * (CELL_HEIGHT + GAP);
                // The wave runs along the hours, a little later on each weekday.
                let shown = ease_out(progress(t, 0.8 + hour as f32 * 0.075 + weekday as f32 * 0.06, 0.5));
                let step = ((cost / self.max) * SEQUENTIAL.len() as f32) as usize;
                let fill = if cost > 0.0 { SEQUENTIAL[step.min(SEQUENTIAL.len() - 1)] } else { "#1b1f28" };
                let inset = (1.0 - shown) * 5.0;
                fframes::svgr!(<g>
                    <rect x={x} y={y} width={CELL_WIDTH} height={CELL_HEIGHT} rx="4" fill="#1b1f28" />
                    <rect x={x + inset} y={y + inset / 2.0} width={CELL_WIDTH - inset * 2.0} height={CELL_HEIGHT - inset} rx="4" fill={fill} opacity={shown} />
                </g>)
            })
            .collect();

        let weekdays: Vec<Svgr> = WEEKDAYS
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let y = GRID_TOP + i as f32 * (CELL_HEIGHT + GAP) + 14.5;
                fframes::svgr!(<text x="20" y={y} font-size="11" font-weight="500" fill={MUTED}>{*name}</text>)
            })
            .collect();
        let hours: Vec<Svgr> = (0..8)
            .map(|i| {
                let x = GRID_LEFT + (i * 3) as f32 * (CELL_WIDTH + GAP) + CELL_WIDTH / 2.0;
                fframes::svgr!(<text x={x} y={GRID_TOP - 9.0} text-anchor="middle" font-size="11" font-weight="500" fill={MUTED}>{format!("{:02}", i * 3)}</text>)
            })
            .collect();
        let ramp_steps: Vec<Svgr> = SEQUENTIAL
            .iter()
            .enumerate()
            .map(|(i, color)| fframes::svgr!(<rect x={56.0 + i as f32 * 15.0} y="263" width="13" height="10" rx="2" fill={*color} />))
            .collect();

        let sessions: Vec<Svgr> = SESSIONS
            .iter()
            .enumerate()
            .map(|(i, session)| {
                let y = SESSIONS_Y + 106.0 + i as f32 * 46.0;
                let start = SESSIONS_AT + 0.5 + i as f32 * 0.16;
                fframes::svgr!(<g opacity={ramp(&frame, start)} transform={Transform::translate(0, rise(&frame, start, 8.0))}>
                    <path d={format!("M20 {} H1196", y - 8.0)} stroke={BORDER} stroke-width="1" />
                    <text x="30" y={y + 14.0} font-size="13" font-weight="500" fill={FG}>{session.hours}</text>
                    <text x="250" y={y + 14.0} font-size="13" font-weight="500" fill={FG}>{session.project}</text>
                    <text x="250" y={y + 30.0} font-size="11" font-weight="500" fill={MUTED}>{session.under}</text>
                    {provider_icon(session.tool, 640.0, y + 3.0, 13.0)}
                    <text x="659" y={y + 14.0} font-size="13" font-weight="500" fill={FG}>{PROVIDER_NAMES[session.tool]}</text>
                    <text x="640" y={y + 30.0} font-size="11" font-weight="500" fill={MUTED}>{session.model}</text>
                    <text x="860" y={y + 14.0} font-size="13" font-weight="500" fill={MUTED}>{session.machine}</text>
                    <text x="1076" y={y + 14.0} text-anchor="end" font-size="13" font-weight="500" fill={FG}>{session.tokens}</text>
                    <text x="1186" y={y + 14.0} text-anchor="end" font-size="13" font-weight="500" fill={FG}>{session.cost}</text>
                </g>)
            })
            .collect();

        fframes::svgr!(
            <g opacity={scene_fade(t, SECONDS)}>
                {caption_sequence(&frame, "4", "När, och i vilka sessioner", &[("Uppskattad kostnad per timme i veckan, summerad över perioden du valt.", 0.0), ("Och varje session för sig. Bara du ser dina timmar och sessioner.", SESSIONS_AT + 0.3)])}
                <g transform={page_transform()} font-family={SANS}>
                    <g opacity={ramp(&frame, 0.2)} transform={Transform::translate(0, rise(&frame, 0.2, 18.0))}>
                        {card(0.0, 0.0, 1216.0, CARD_HEIGHT, "When you use AI", "Estimated API-equivalent spend per hour of the week, in Europe/Stockholm, summed over the range.")}
                        {segmented(1196.0, 20.0, "Spend", "Tokens", 52.0, 58.0, 0.0)}
                        {weekdays}
                        {hours}
                        {cells}
                        <text x="20" y="272" font-size="11" font-weight="500" fill={MUTED}>"Less"</text>
                        {ramp_steps}
                        <text x="168" y="272" font-size="11" font-weight="500" fill={MUTED}>{format!("More (up to {})", usd(self.max))}</text>
                    </g>
                    <g opacity={ramp(&frame, SESSIONS_AT)} transform={Transform::translate(0, rise(&frame, SESSIONS_AT, 18.0))}>
                        {card(0.0, SESSIONS_Y, 1216.0, 250.0, "Sessions", "2,418 sessions in the range. Active hours in Europe/Stockholm; costs are API-equivalent estimates in USD.")}
                        <g font-size="12" font-weight="500" fill={MUTED}>
                            <text x="30" y={SESSIONS_Y + 88.0}>"Active hours ↓"</text>
                            <text x="250" y={SESSIONS_Y + 88.0}>"Project"</text>
                            <text x="640" y={SESSIONS_Y + 88.0}>"Tool and models"</text>
                            <text x="860" y={SESSIONS_Y + 88.0}>"Machine"</text>
                            <text x="1076" y={SESSIONS_Y + 88.0} text-anchor="end">"Tokens"</text>
                            <text x="1186" y={SESSIONS_Y + 88.0} text-anchor="end">"Est. cost"</text>
                        </g>
                        {sessions}
                    </g>
                </g>
            </g>
        )
    }
}
