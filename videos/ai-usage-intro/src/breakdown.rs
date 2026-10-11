//! Two cards of the page side by side: spend per project, switched to tokens by a click, and
//! the token mix per tool.
use crate::*;

const SECONDS: f32 = 10.0;
const CARD_WIDTH: f32 = 598.0;
const CARD_HEIGHT: f32 = 440.0;
const MIX_X: f32 = CARD_WIDTH + 20.0;

const BAR_LEFT: f32 = 224.0;
const BAR_MAX: f32 = 250.0;
const BARS_TOP: f32 = 100.0;
const BAR_PITCH: f32 = 54.0;

const TOKENS_AT: f32 = 5.0;
/// The Tokens option of the project card's control.
const TOKENS_OPTION: (f32, f32) = (547.0, 34.0);

/// Tokens per project in billions, in the order of the rows below. They sum to the total.
const TOKENS: [f32; 6] = [6.3, 5.4, 4.6, 3.9, 3.0, 1.6];

/// Token categories with the app's colours (`TOKEN_COLORS`), and their share for all tools.
const CATEGORIES: [(&str, &str, &str); 4] = [
    ("Input", "#d55181", "471.2M (1.9%)"),
    ("Cache read", "#008300", "23.9B (96.4%)"),
    ("Cache write", "#9085e9", "347.2M (1.4%)"),
    ("Output", "#e66767", "81.6M (0.3%)"),
];
/// Shares per row in percent: input, cache read, cache write, output.
const MIX: [(&str, [f32; 4]); 5] = [
    ("All", [1.9, 96.4, 1.4, 0.3]),
    ("Codex", [2.4, 97.2, 0.0, 0.4]),
    ("Claude Code", [0.4, 95.9, 3.5, 0.2]),
    ("Copilot", [97.0, 0.0, 0.0, 3.0]),
    ("Pi", [3.2, 96.3, 0.0, 0.5]),
];
const MIX_LEFT: f32 = 106.0;
const MIX_WIDTH: f32 = 472.0;

#[derive(Debug)]
pub struct Breakdown;

impl Scene for Breakdown {
    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(SECONDS)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::from([AudioTrack::new("click.wav", Second(TOKENS_AT)..Eof).gain_db(-7.0)])
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = frame.seconds();
        let tokens = ease_in_out(progress(t, TOKENS_AT, 0.6));

        // Projects by cost, with Unassigned where its cost ranks.
        let rows = [
            (data::PROJECTS[0].0, data::PROJECTS[0].1, PROVIDER_COLORS[0]),
            (data::PROJECTS[1].0, data::PROJECTS[1].1, PROVIDER_COLORS[0]),
            (data::PROJECTS[2].0, data::PROJECTS[2].1, PROVIDER_COLORS[0]),
            ("Unassigned", data::UNASSIGNED_COST, OTHER),
            (data::PROJECTS[3].0, data::PROJECTS[3].1, PROVIDER_COLORS[0]),
            (data::PROJECTS[4].0, data::PROJECTS[4].1, PROVIDER_COLORS[0]),
        ];
        let projects: Vec<Svgr> = rows
            .iter()
            .enumerate()
            .map(|(i, (label, cost, color))| {
                let grow = ease_out(progress(t, 0.5 + i as f32 * 0.08, 0.8));
                let y = BARS_TOP + i as f32 * BAR_PITCH;
                let by_cost = cost / 4200.0 * BAR_MAX;
                let by_tokens = TOKENS[i] / 6.6 * BAR_MAX;
                let width = (lerp(by_cost, by_tokens, tokens) * grow).max(0.5);
                fframes::svgr!(<g>
                    <text x={BAR_LEFT - 10.0} y={y + 16.0} text-anchor="end" font-size="11" font-weight="500" fill={MUTED}>{*label}</text>
                    <rect x={BAR_LEFT} y={y} width={width} height="24" rx="4" fill={*color} />
                    <text x={BAR_LEFT + width + 8.0} y={y + 16.0} font-size="11" font-weight="500" fill={FG} opacity={grow * (1.0 - progress(t, TOKENS_AT, 0.2))}>{usd(cost * grow)}</text>
                    <text x={BAR_LEFT + width + 8.0} y={y + 16.0} font-size="11" font-weight="500" fill={FG} opacity={progress(t, TOKENS_AT + 0.25, 0.3)}>{format!("{:.1}B", TOKENS[i])}</text>
                </g>)
            })
            .collect();

        let mix: Vec<Svgr> = MIX
            .iter()
            .enumerate()
            .map(|(row, (label, shares))| {
                let y = 104.0 + row as f32 * 38.0;
                let shown = ease_out(progress(t, 0.9 + row as f32 * 0.12, 0.9)) * MIX_WIDTH;
                let mut x = MIX_LEFT;
                let parts: Vec<Svgr> = shares
                    .iter()
                    .enumerate()
                    .filter(|(_, share)| **share > 0.0)
                    .map(|(category, share)| {
                        let from = x;
                        x += share / 100.0 * MIX_WIDTH;
                        // Each part is cut where the wipe has reached, less a 1.5 px gap.
                        let width = (x.min(MIX_LEFT + shown) - from - 1.5).max(0.01);
                        fframes::svgr!(<rect x={from} y={y} width={width} height="22" rx="3" fill={CATEGORIES[category].1} opacity={if width > 0.01 { 1.0 } else { 0.0 }} />)
                    })
                    .collect();
                fframes::svgr!(<g>
                    <text x={MIX_LEFT - 10.0} y={y + 15.0} text-anchor="end" font-size="11" font-weight="500" fill={MUTED}>{*label}</text>
                    {parts}
                </g>)
            })
            .collect();
        let legend: Vec<Svgr> = CATEGORIES
            .iter()
            .enumerate()
            .map(|(i, (name, color, value))| {
                let x = 20.0 + (i % 2) as f32 * 270.0;
                let y = 322.0 + (i / 2) as f32 * 26.0;
                fframes::svgr!(<g opacity={ramp(&frame, 1.8 + i as f32 * 0.1)}>
                    <circle cx={x + 5.0} cy={y - 4.0} r="4.5" fill={*color} />
                    <text x={x + 18.0} y={y} font-size="12" font-weight="500" fill={MUTED}>{*name}</text>
                    <text x={x + 24.0 + name.len() as f32 * 6.3} y={y} font-size="12" font-weight="600" fill={FG}>{*value}</text>
                </g>)
            })
            .collect();

        let reach = ease_in_out(progress(t, 3.9, 0.9));
        let leave = ease_in_out(progress(t, TOKENS_AT + 0.8, 1.0));
        let pointer_x = lerp(lerp(470.0, TOKENS_OPTION.0, reach), 500.0, leave);
        let pointer_y = lerp(lerp(300.0, TOKENS_OPTION.1, reach), 120.0, leave);

        fframes::svgr!(
            <g opacity={scene_fade(t, SECONDS)}>
                {caption(&frame, "4", "Per projekt och tokentyp", "Vilka projekt som drar mest, i kostnad eller tokens. Nästan allt är cacheläsningar.")}
                <g transform={page_transform()} font-family={SANS}>
                    <g opacity={ramp(&frame, 0.2)} transform={Transform::translate(0, rise(&frame, 0.2, 18.0))}>
                        {card(0.0, 0.0, CARD_WIDTH, CARD_HEIGHT, "Spend per project", "API-equivalent estimate in USD. ? marks usage without a known price.")}
                        {segmented(CARD_WIDTH - 20.0, 20.0, "Spend", "Tokens", 52.0, 58.0, tokens)}
                        {projects}
                    </g>
                    <g opacity={ramp(&frame, 0.32)} transform={Transform::translate(0, rise(&frame, 0.32, 18.0))}>
                        <g transform={Transform::translate(MIX_X, 0)}>
                            {card(0.0, 0.0, CARD_WIDTH, CARD_HEIGHT, "Token mix", "Shares of each token category. Input excludes cache reads and writes;")}
                            <text x="20" y="73" font-size="12" font-weight="500" fill={MUTED}>"reasoning counts as output."</text>
                            {mix}
                            {legend}
                        </g>
                    </g>
                    {pointer(t, pointer_x, pointer_y, &[TOKENS_AT], progress(t, 3.6, 0.3).min(1.0 - progress(t, TOKENS_AT + 1.4, 0.4)))}
                </g>
            </g>
        )
    }
}
