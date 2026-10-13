//! The AI usage page as it is drawn by the app: the header with the currency switch, the
//! four tabs, the filters, and the Overview's summary strip, daily chart, "Where it went"
//! and hour heatmap. Everything is in page pixels with the origin at the top left of the
//! page's content; `view` puts a part of it under the caption.
use crate::*;

pub(crate) const TABS_Y: f32 = 96.0;
pub(crate) const FILTERS_Y: f32 = 157.0;
pub(crate) const SUMMARY_Y: f32 = 214.0;
pub(crate) const SUMMARY_HEIGHT: f32 = 140.0;
pub(crate) const CHART_Y: f32 = 376.0;
pub(crate) const CHART_HEIGHT: f32 = 384.0;
/// The row of "Where it went" and the heatmap.
pub(crate) const BOTTOM_Y: f32 = 781.0;
/// Both cards are as tall as the heatmap needs, unless the list is longer.
pub(crate) const BOTTOM_HEIGHT: f32 = 498.0;
pub(crate) const WHERE_WIDTH: f32 = 833.6;

/// The scroll positions the film stops at: the top, the summary with the whole chart, and
/// the bottom row. About 556 page pixels fit under the caption at the page's zoom.
pub(crate) const AT_TOP: f32 = 0.0;
pub(crate) const AT_CHART: f32 = 204.0;
pub(crate) const AT_BOTTOM: f32 = 771.0;
/// A tab other than the Overview, from its tabs down.
pub(crate) const AT_TABS: f32 = 86.0;

const PLOT_LEFT: f32 = 76.0;
const PLOT_RIGHT: f32 = 1190.0;
const PLOT_TOP: f32 = CHART_Y + 90.0;
pub(crate) const PLOT_BOTTOM: f32 = CHART_Y + 308.0;
const BAR_WIDTH: f32 = 15.0;

/// The switch of the header (`currency-switch.tsx`), at its right edge.
const SWITCH_X: f32 = 1013.0;
/// Where a pointer presses each currency.
pub(crate) const DOLLARS_OPTION: (f32, f32) = (SWITCH_X + 102.0, 21.0);
pub(crate) const KRONOR_OPTION: (f32, f32) = (SWITCH_X + 36.0, 21.0);
/// The chart's Model option, and the options of "Where it went".
pub(crate) const MODEL_OPTION: (f32, f32) = (1056.0, CHART_Y + 34.0);
const DIMENSIONS: [(&str, f32); 4] = [
    ("Project", 61.0),
    ("Model", 53.0),
    ("Tool", 44.0),
    ("Machine", 70.0),
];
const DIMENSIONS_RIGHT: f32 = WHERE_WIDTH - 20.0;

/// Part of the page under the caption: `content` in page pixels, scrolled down by `scroll`
/// of them and cut off where the caption's space begins.
pub(crate) fn view<'a>(scroll: f32, content: Svgr<'a>) -> Svgr<'a> {
    let top = PAGE_Y - 10.0;
    fframes::svgr!(
        <g>
            <defs>
                <clipPath id="page-view">
                    <rect x="0" y={top} width="1920" height={1080.0 - top} />
                </clipPath>
                <linearGradient id="page-fade" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0" stop-color={BG} stop-opacity="1" />
                    <stop offset="1" stop-color={BG} stop-opacity="0" />
                </linearGradient>
            </defs>
            <g clip-path="url(#page-view)">
                <g transform={format!("translate({PAGE_X} {}) scale({ZOOM})", PAGE_Y - scroll * ZOOM)}>{content}</g>
            </g>
            // What scrolls out under the caption fades instead of ending on an edge.
            <rect x="0" y={top} width="1920" height="16" fill="url(#page-fade)" opacity={progress(scroll, 0.0, 30.0)} />
        </g>
    )
}

/// One value giving way to the next: the old one leaves upwards as the new one comes in.
pub(crate) fn roll<'a>(from: Svgr<'a>, to: Svgr<'a>, p: f32) -> Svgr<'a> {
    if p <= 0.0 {
        return from;
    }
    if p >= 1.0 {
        return to;
    }
    fframes::svgr!(
        <g>
            <g opacity={1.0 - p} transform={Transform::translate(0, -7.0 * p)}>{from}</g>
            <g opacity={p} transform={Transform::translate(0, 7.0 * (1.0 - p))}>{to}</g>
        </g>
    )
}

/// The page's header: its title and introduction, and the switch that chooses the currency
/// of every estimate, with the rate it converts at under it.
pub(crate) fn header<'a>(currency: Currency) -> Svgr<'a> {
    let options = [
        ("kr", "SEK", Currency::Sek, 3.0, 65.0),
        ("$", "USD", Currency::Usd, 70.0, 64.0),
    ];
    let buttons: Vec<Svgr> = options
        .iter()
        .map(|(symbol, code, option, x, width)| {
            let checked = *option == currency;
            fframes::svgr!(<g>
                <rect x={SWITCH_X + x} y="5" width={*width} height="32" rx="6" fill={BG} stroke={BORDER} stroke-width="1" opacity={if checked { 1.0 } else { 0.0 }} />
                <text x={SWITCH_X + x + 13.0} y="25.5" font-size="13" font-weight="600" fill={if checked { AMBER } else { MUTED }}>{*symbol}</text>
                <text x={SWITCH_X + x + width - 13.0} y="25.5" text-anchor="end" font-size="13" font-weight="500" fill={if checked { FG } else { MUTED }}>{*code}</text>
            </g>)
        })
        .collect();
    let rate = match currency {
        Currency::Sek => fframes::svgr!(
            <text x="1216" y="59" text-anchor="end" font-size="12" font-weight="500" fill={MUTED}>
                <tspan font-weight="600" fill={FG}>{format!("1 USD = {} kr", format!("{:.4}", money::SEK_PER_USD).replace('.', ","))}</tspan>
                <tspan>", October’s average so far"</tspan>
            </text>
        ),
        Currency::Usd => fframes::svgr!(
            <text x="1216" y="59" text-anchor="end" font-size="12" font-weight="500" fill={MUTED}>"As priced, no conversion"</text>
        ),
    };
    fframes::svgr!(
        <g font-family={SANS}>
            <text x="0" y="24" font-size="24" font-weight="600" letter-spacing="-0.6" fill={FG}>"AI usage"</text>
            <g font-size="14" font-weight="500" fill={MUTED}>
                <text x="0" y="50">
                    <tspan>"Your own AI usage, as "</tspan>
                    <tspan font-family={MONO} font-size="13" font-weight="400">"token-ledger sync"</tspan>
                    <tspan>" uploads it from your machines. Only you see these hours and sessions."</tspan>
                </text>
                <text x="0" y="70">"Costs are API-equivalent estimates, not a subscription bill."</text>
            </g>
            {icon(icons::INFO, 385.0, 58.5, 14.0, MUTED)}
            <rect x={SWITCH_X} y="2" width="203" height="38" rx="8" fill="#1b1f28" stroke={BORDER} stroke-width="1" />
            {buttons}
            <text x={SWITCH_X + 149.0} y="25.5" font-size="13" font-weight="600" fill={MUTED}>"€"</text>
            <text x={SWITCH_X + 187.0} y="25.5" text-anchor="end" font-size="13" font-weight="500" fill={MUTED}>"EUR"</text>
            {rate}
        </g>
    )
}

/// What the tabs say is waiting behind them.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Waiting {
    pub sessions: f32,
    /// Repositories still to map; the tab has no mark once there are none.
    pub to_map: usize,
    pub machines: usize,
}

impl Waiting {
    /// The whole range, before any repository is mapped.
    pub(crate) fn all() -> Self {
        Self {
            sessions: data::SESSIONS,
            to_map: data::UNMAPPED.len(),
            machines: 1,
        }
    }

    fn chips(&self) -> [Option<(String, bool)>; 4] {
        [
            None,
            Some((thousands(self.sessions.round() as i64), false)),
            (self.to_map > 0).then(|| (format!("{} to map", self.to_map), true)),
            Some((self.machines.to_string(), false)),
        ]
    }

    /// Where each tab starts and how wide it is.
    fn layout(&self) -> [(f32, f32); 4] {
        const LABELS: [f32; 4] = [62.0, 60.0, 85.0, 65.0];
        let chips = self.chips();
        let mut x = 0.0;
        std::array::from_fn(|tab| {
            let chip = chips[tab]
                .as_ref()
                .map_or(0.0, |(text, _)| 8.0 + chip_width(text));
            let width = 12.0 + 16.0 + 8.0 + LABELS[tab] + chip + 12.0;
            let start = x;
            x += width + 4.0;
            (start, width)
        })
    }
}

fn chip_width(text: &str) -> f32 {
    text.chars().count() as f32 * 6.3 + 12.0
}

/// The page's tabs (`usage-tabs.tsx`). `active` is the tab shown, and may be between two
/// tabs while the mark slides from one to the other.
pub(crate) fn tabs<'a>(active: f32, waiting: &Waiting) -> Svgr<'a> {
    const TABS: [(&str, &str); 4] = [
        ("Overview", icons::CHART_COLUMN),
        ("Sessions", icons::LIST),
        ("Repositories", icons::FOLDER_GIT),
        ("Machines", icons::LAPTOP),
    ];
    let layout = waiting.layout();
    let chips = waiting.chips();
    let drawn: Vec<Svgr> = TABS
        .iter()
        .enumerate()
        .map(|(tab, &(label, glyph))| {
            let (x, width) = layout[tab];
            let ink = if active.round() as usize == tab { FG } else { MUTED };
            let chip = chips[tab].clone().map(|(text, attention)| {
                let chip = chip_width(&text);
                let at = x + width - 12.0 - chip;
                fframes::svgr!(<g>
                    <rect x={at} y={TABS_Y + 12.5} width={chip} height="15" rx="7.5" fill={if attention { AMBER } else { MUTED_SURFACE }} opacity={if attention { 0.1 } else { 1.0 }} />
                    <text x={at + chip / 2.0} y={TABS_Y + 24.0} text-anchor="middle" font-size="11" font-weight="600" fill={if attention { AMBER } else { MUTED }}>{text}</text>
                </g>)
            });
            fframes::svgr!(<g>
                {icon(glyph, x + 12.0, TABS_Y + 12.0, 16.0, ink)}
                <text x={x + 36.0} y={TABS_Y + 25.0} font-size="14" font-weight="600" fill={ink}>{label}</text>
                {chip.unwrap_or_default()}
            </g>)
        })
        .collect();
    let from = layout[(active.floor() as usize).min(3)];
    let to = layout[(active.ceil() as usize).min(3)];
    let slide = ease_in_out(active.fract());
    fframes::svgr!(
        <g font-family={SANS}>
            <path d={format!("M0 {} H1216", TABS_Y + 40.0)} stroke={BORDER} stroke-width="1" opacity="0.6" />
            {drawn}
            <rect x={lerp(from.0, to.0, slide) + 8.0} y={TABS_Y + 39.0} width={lerp(from.1, to.1, slide) - 16.0} height="2" rx="1" fill={AMBER} />
        </g>
    )
}

/// One control of the filter bar: an outlined button with its text, and a chevron for a
/// select.
fn filter<'a>(x: f32, width: f32, text: &'a str, select: bool) -> Svgr<'a> {
    fframes::svgr!(
        <g>
            <rect x={x} y={FILTERS_Y} width={width} height="36" rx="9" fill={BG} stroke={BORDER} stroke-width="1" />
            <text x={x + if select { 12.0 } else { 41.0 }} y={FILTERS_Y + 23.0} font-size="14" font-weight="500" fill={FG}>{text}</text>
            {if select { icon(icons::CHEVRON_DOWN, x + width - 28.0, FILTERS_Y + 10.0, 16.0, MUTED) } else { icon(icons::CALENDAR, x + 16.0, FILTERS_Y + 10.0, 16.0, FG) }}
        </g>
    )
}

/// The dates and filters above the report (`filter-bar.tsx`), with nothing chosen, and the
/// time zone they are in.
pub(crate) fn filters<'a>() -> Svgr<'a> {
    fframes::svgr!(
        <g font-family={SANS}>
            {filter(1.0, 196.0, "14 Aug – 11 Oct 2026", false)}
            {filter(207.0, 126.0, "All tools", true)}
            {filter(343.0, 126.0, "All models", true)}
            {filter(479.0, 126.0, "All projects", true)}
            {filter(615.0, 130.0, "All machines", true)}
            {icon(icons::GLOBE, 1093.0, FILTERS_Y + 11.0, 14.0, MUTED)}
            <text x="1216" y={FILTERS_Y + 22.5} text-anchor="end" font-size="12" font-weight="500" fill={MUTED}>"Europe/Stockholm"</text>
        </g>
    )
}

/// The headline figures of a range and its filters, in USD.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Figures {
    pub cost: f32,
    pub tokens_b: f32,
    pub cache_b: f32,
    /// Output, cache write and input tokens, in millions.
    pub paid_m: [f32; 3],
    pub requests: f32,
    pub sessions: f32,
    pub days: f32,
    /// The cost of usage without a project, when there is any.
    pub unassigned: Option<f32>,
    pub to_map: usize,
}

impl Figures {
    /// The whole mocked range, before any repository is mapped.
    pub(crate) fn all() -> Self {
        Self {
            cost: data::TOTAL_COST,
            tokens_b: data::TOTAL_TOKENS_B,
            cache_b: data::CACHE_READ_TOKENS_B,
            paid_m: data::PAID_TOKENS_M,
            requests: data::REQUESTS,
            sessions: data::SESSIONS,
            days: data::DAYS as f32,
            unassigned: Some(data::UNASSIGNED_COST),
            to_map: data::UNMAPPED.len(),
        }
    }

    /// `share` of every figure over `days` active days, as while the page counts up.
    pub(crate) fn part(&self, share: f32, days: f32) -> Self {
        Self {
            cost: self.cost * share,
            tokens_b: self.tokens_b * share,
            cache_b: self.cache_b * share,
            paid_m: self.paid_m.map(|tokens| tokens * share),
            requests: self.requests * share,
            sessions: self.sessions * share,
            days,
            unassigned: self.unassigned.map(|cost| cost * share),
            to_map: self.to_map,
        }
    }
}

/// Where the four tiles of the summary strip start: `1.2fr 1.4fr 1fr 1fr` of the page.
const TILES: [f32; 5] = [0.0, 317.4, 687.6, 952.3, 1217.0];

/// The surface of the summary strip (`summary-tiles.tsx`): one card divided in four.
pub(crate) fn summary_surface<'a>() -> Svgr<'a> {
    let dividers: Vec<Svgr> = TILES[1..4]
        .iter()
        .map(|x| fframes::svgr!(<path d={format!("M{} {SUMMARY_Y} v{SUMMARY_HEIGHT}", x - 0.5)} stroke={BORDER} stroke-width="1" />))
        .collect();
    fframes::svgr!(
        <g>
            <rect x="0" y={SUMMARY_Y} width="1216" height={SUMMARY_HEIGHT} rx="16" fill={CARD} stroke={BORDER} stroke-width="1" />
            {dividers}
        </g>
    )
}

/// The labels of the four tiles. The last one takes the attention colour while some usage
/// counts for no project.
pub(crate) fn summary_labels<'a>(unassigned: bool) -> Svgr<'a> {
    let labels = [
        (icons::COINS, "Estimated cost", false),
        (icons::LAYERS, "Tokens", false),
        (icons::ACTIVITY, "Requests", false),
        (icons::CIRCLE_DASHED, "Unassigned", unassigned),
    ];
    let drawn: Vec<Svgr> = labels
        .iter()
        .enumerate()
        .map(|(index, &(glyph, label, attention))| {
            let x = TILES[index] + 16.0;
            let ink = if attention { AMBER } else { MUTED };
            fframes::svgr!(<g>
                {icon(glyph, x, SUMMARY_Y + 17.0, 14.0, ink)}
                <text x={x + 20.0} y={SUMMARY_Y + 28.5} font-size="12" font-weight="500" fill={ink}>{label}</text>
            </g>)
        })
        .collect();
    fframes::svgr!(<g font-family={SANS}>{drawn}</g>)
}

/// A tile's figure and the line under it.
fn figure<'a>(index: usize, value: String, detail: String) -> Svgr<'a> {
    let x = TILES[index] + 16.0;
    fframes::svgr!(
        <g font-family={SANS}>
            <text x={x} y={SUMMARY_Y + 61.0} font-size="24" font-weight="600" letter-spacing="-0.6" fill={FG}>{value}</text>
            <text x={x} y={SUMMARY_Y + 84.0} font-size="12" font-weight="500" fill={MUTED}>{detail}</text>
        </g>
    )
}

/// What tile `index` says of `figures` in `currency`, under its label.
pub(crate) fn summary_tile<'a>(index: usize, figures: &Figures, currency: Currency) -> Svgr<'a> {
    let paid: f32 = figures.paid_m.iter().sum();
    match index {
        0 => figure(
            0,
            money::shown(currency, figures.cost),
            format!(
                "{} per active day",
                money::shown(currency, figures.cost / figures.days.max(1.0))
            ),
        ),
        1 => {
            let strip_width = TILES[2] - TILES[1] - 33.0;
            let mut at = TILES[1] + 16.0;
            let mut legend_at = TILES[1] + 16.0;
            let colors = ["#9085e9", "#6761a5", "#494776"];
            let names = ["Output", "Cache write", "Input"];
            let strip: Vec<Svgr> = (0..3)
                .map(|i| {
                    let width = (strip_width - 4.0) * figures.paid_m[i] / paid.max(0.001);
                    let x = at;
                    at += width + 2.0;
                    let label = format!("{} {}", names[i], compact(figures.paid_m[i] * 1e6));
                    let legend = legend_at;
                    legend_at += 14.0 + label.chars().count() as f32 * 5.7 + 12.0;
                    fframes::svgr!(<g>
                        <rect x={x} y={SUMMARY_Y + 97.0} width={width.max(0.01)} height="6" rx="2" fill={colors[i]} />
                        <rect x={legend} y={SUMMARY_Y + 112.0} width="8" height="8" rx="2" fill={colors[i]} />
                        <text x={legend + 14.0} y={SUMMARY_Y + 120.0} font-family={SANS} font-size="11" font-weight="500" fill={MUTED}>{label}</text>
                    </g>)
                })
                .collect();
            fframes::svgr!(<g>
                {figure(1, compact(figures.tokens_b * 1e9), format!("{} cache reads, {} other", compact(figures.cache_b * 1e9), compact(paid * 1e6)))}
                {strip}
            </g>)
        }
        2 => figure(
            2,
            thousands(figures.requests.round() as i64),
            format!(
                "{} sessions on {} days",
                thousands(figures.sessions.round() as i64),
                figures.days.round() as i64
            ),
        ),
        _ => match figures.unassigned {
            Some(cost) => {
                let link = format!("Map {} repositories", figures.to_map);
                let x = TILES[3] + 16.0;
                let after = x + link.chars().count() as f32 * 5.85;
                fframes::svgr!(<g>
                    {figure(3, money::shown(currency, cost), format!("{} of the known cost", percent(cost / figures.cost.max(0.001))))}
                    <text x={x} y={SUMMARY_Y + 106.0} font-family={SANS} font-size="12" font-weight="600" fill={AMBER}>{link}</text>
                    {icon(icons::CHEVRON_RIGHT, after, SUMMARY_Y + 95.0, 14.0, AMBER)}
                </g>)
            }
            None => figure(
                3,
                "None".to_owned(),
                "All usage counts for a project".to_owned(),
            ),
        },
    }
}

/// The whole summary strip of `figures` in `currency`.
pub(crate) fn summary<'a>(figures: &Figures, currency: Currency) -> Svgr<'a> {
    let tiles: Vec<Svgr> = (0..4)
        .map(|index| summary_tile(index, figures, currency))
        .collect();
    fframes::svgr!(
        <g>
            {summary_surface()}
            {summary_labels(figures.unassigned.is_some())}
            {tiles}
        </g>
    )
}

/// A bar segment from `top` to `bottom`, with the top corners rounded by `radius`.
fn segment(x: f32, top: f32, bottom: f32, radius: f32) -> String {
    let right = x + BAR_WIDTH;
    format!(
        "M{x} {bottom} V{} Q{x} {top} {} {top} H{} Q{right} {top} {right} {} V{bottom} Z",
        top + radius,
        x + radius,
        right - radius,
        top + radius
    )
}

/// One day's stacked bar: `parts` from the bottom up, each `scale` px per unit.
fn bar<'a>(x: f32, parts: &[f32], colors: &[&'a str], scale: f32) -> Svgr<'a> {
    let mut bottom = PLOT_BOTTOM;
    let last = parts.iter().rposition(|cost| *cost > 0.0).unwrap_or(0);
    parts
        .iter()
        .enumerate()
        .filter(|(_, cost)| **cost > 0.0)
        .map(|(i, cost)| {
            let height = cost * scale;
            let top = bottom - height;
            // A 1.5 px gap of card surface separates the segments, as in the app.
            let gap = if i == last {
                0.0
            } else {
                1.5_f32.min(height / 2.0)
            };
            let radius = if i == last { 4.0_f32.min(height) } else { 0.0 };
            let path = segment(x, top + gap, bottom, radius);
            bottom = top;
            fframes::svgr!(<path d={path} fill={colors[i]} />)
        })
        .collect()
}

/// What the daily chart shows at one moment.
pub(crate) struct Daily {
    /// The cost of each day per tool in USD, as tall as it is drawn now.
    pub days: Vec<[f32; 4]>,
    /// 0 stacks the days by tool, 1 by model.
    pub by_model: f32,
    pub currency: Currency,
    /// The top of the axis, in `currency`.
    pub axis: f32,
}

impl Daily {
    /// The whole range stacked by tool, in kronor.
    pub(crate) fn all() -> Self {
        Self {
            days: data::DAYS_OF_USAGE.iter().map(|day| day.cost).collect(),
            by_model: 0.0,
            currency: Currency::Sek,
            axis: ALL_AXIS,
        }
    }
}

/// The top of the axis for the whole range, in kronor.
pub(crate) const ALL_AXIS: f32 = 6_000.0;

/// The left edge of day `index`'s bar.
pub(crate) fn day_x(index: usize) -> f32 {
    let band = (PLOT_RIGHT - PLOT_LEFT) / data::DAYS as f32;
    PLOT_LEFT + index as f32 * band + (band - BAR_WIDTH) / 2.0
}

/// The day whose band of the plot holds `x`, if any.
pub(crate) fn day_under(x: f32) -> Option<usize> {
    let band = (PLOT_RIGHT - PLOT_LEFT) / data::DAYS as f32;
    let index = ((x - PLOT_LEFT) / band).floor();
    (index >= 0.0 && (index as usize) < data::DAYS).then_some(index as usize)
}

/// The top of the plot, which a pointer has to be below to be over a day.
pub(crate) const PLOT_TOP_Y: f32 = PLOT_TOP;

/// The daily spend chart (`daily-spend-chart.tsx`): one bar per local day, stacked by tool
/// or by model, with its legend.
pub(crate) fn daily<'a>(chart: &Daily) -> Svgr<'a> {
    let scale = (PLOT_BOTTOM - PLOT_TOP) / chart.axis * chart.currency.per_usd();
    let grid: Vec<Svgr> = (0..=4)
        .map(|i| {
            let y = PLOT_BOTTOM - (PLOT_BOTTOM - PLOT_TOP) * i as f32 / 4.0;
            fframes::svgr!(<g>
                <path d={format!("M{PLOT_LEFT} {y} H{PLOT_RIGHT}")} stroke={GRID} stroke-width="1" />
                <text x={PLOT_LEFT - 8.0} y={y + 4.0} text-anchor="end" font-size="11" font-weight="500" fill={MUTED}>{money::axis(chart.currency, i as f32 * chart.axis / 4.0)}</text>
            </g>)
        })
        .collect();
    let labels = &*data::DAYS_OF_USAGE;
    let bars: Vec<Svgr> = chart
        .days
        .iter()
        .enumerate()
        .map(|(i, cost)| {
            let x = day_x(i);
            // Every fourth day is named, and the last one in place of its neighbour.
            let named = (i % 4 == 0 && i + 4 < data::DAYS) || i + 1 == data::DAYS;
            let tick = if named {
                fframes::svgr!(<text x={x + BAR_WIDTH / 2.0} y={PLOT_BOTTOM + 18.0} text-anchor="middle" font-size="11" font-weight="500" fill={MUTED}>{labels[i].label.as_str()}</text>)
            } else {
                Svgr::empty()
            };
            fframes::svgr!(<g>
                <g opacity={1.0 - chart.by_model}>{bar(x, cost, &PROVIDER_COLORS, scale)}</g>
                <g opacity={chart.by_model}>{bar(x, &data::by_model(*cost), &data::MODEL_COLORS, scale)}</g>
                {tick}
            </g>)
        })
        .collect();

    // The legend names the tools that have usage in the days shown.
    let mut tool_at = 20.0;
    let tools: Vec<Svgr> = (0..4)
        .filter(|tool| chart.days.iter().any(|day| day[*tool] > 0.0))
        .map(|i| {
            let x = tool_at;
            tool_at += [72.0, 108.0, 78.0, 40.0][i];
            fframes::svgr!(<g>
                {provider_icon(i, x, CHART_Y + 345.0, 14.0)}
                <text x={x + 20.0} y={CHART_Y + 356.5} font-size="12" font-weight="500" fill={MUTED}>{PROVIDER_NAMES[i]}</text>
            </g>)
        })
        .collect();
    let mut legend_at = 20.0;
    let models: Vec<Svgr> = (0..data::MODELS.len())
        .map(|i| {
            let x = legend_at;
            legend_at += 16.0 + data::MODELS[i].len() as f32 * 6.45 + 16.0;
            fframes::svgr!(<g>
                <rect x={x} y={CHART_Y + 347.0} width="10" height="10" rx="2" fill={data::MODEL_COLORS[i]} />
                <text x={x + 16.0} y={CHART_Y + 356.5} font-size="12" font-weight="500" fill={MUTED}>{data::MODELS[i]}</text>
            </g>)
        })
        .collect();

    fframes::svgr!(
        <g font-family={SANS}>
            {card(0.0, CHART_Y, 1216.0, CHART_HEIGHT, "Estimated spend per day", "By local day in Europe/Stockholm.")}
            {segments(1196.0, CHART_Y + 20.0, &[("Chart", 50.0), ("Table", 50.0)], 0.0)}
            {segments(1084.0, CHART_Y + 20.0, &[("Tool", 44.0), ("Model", 52.0)], chart.by_model)}
            {grid}
            {bars}
            <g opacity={1.0 - chart.by_model}>{tools}</g>
            <g opacity={chart.by_model}>{models}</g>
        </g>
    )
}

/// The mark before a row of "Where it went".
#[derive(Debug, Clone, Copy)]
pub(crate) enum Mark {
    Project,
    Unassigned,
    Tool(usize),
}

/// What stands under a row's name.
#[derive(Debug, Clone)]
pub(crate) enum Under {
    /// The project's one repository, as a project key.
    Repository(&'static str),
    /// Several repositories, folded behind a chevron.
    Folded(String),
    Text(String),
}

/// One row of "Where it went": a name, its cost in USD, and the colour of its bar.
#[derive(Debug, Clone)]
pub(crate) struct Row {
    pub mark: Mark,
    pub label: &'static str,
    pub under: Under,
    pub cost: f32,
    pub color: &'static str,
}

/// The bar of usage that is no tool's: `hsl(var(--foreground) / 0.55)` over the card.
const PLAIN: &str = "#8d8d8b";

/// The rows of each way to break the usage down, costliest first.
pub(crate) fn rows(dimension: usize) -> Vec<Row> {
    match dimension {
        0 => {
            let mut rows: Vec<Row> = data::PROJECTS
                .iter()
                .zip(data::PROJECT_REPOSITORIES)
                .map(|((name, cost), repositories)| Row {
                    mark: Mark::Project,
                    label: name,
                    under: if repositories.contains('/') {
                        Under::Repository(repositories)
                    } else {
                        Under::Folded(repositories.to_owned())
                    },
                    cost: *cost,
                    color: PLAIN,
                })
                .collect();
            rows.push(Row {
                mark: Mark::Unassigned,
                label: "Unassigned",
                under: Under::Folded(format!(
                    "{} repositories without a project",
                    data::UNMAPPED.len()
                )),
                cost: data::UNASSIGNED_COST,
                color: OTHER,
            });
            rows.sort_by(|a, b| b.cost.total_cmp(&a.cost));
            rows
        }
        1 => data::model_rows()
            .into_iter()
            .map(|(tool, model, cost, requests)| Row {
                mark: Mark::Tool(tool),
                label: model,
                under: Under::Text(format!(
                    "{}, {} requests",
                    PROVIDER_NAMES[tool],
                    thousands(requests.round() as i64)
                )),
                cost,
                color: PROVIDER_COLORS[tool],
            })
            .collect(),
        _ => {
            let requests = data::tool_requests();
            let mut rows: Vec<Row> = data::tool_costs()
                .iter()
                .enumerate()
                .map(|(tool, cost)| Row {
                    mark: Mark::Tool(tool),
                    label: PROVIDER_NAMES[tool],
                    under: Under::Text(format!(
                        "{} requests",
                        thousands(requests[tool].round() as i64)
                    )),
                    cost: *cost,
                    color: PROVIDER_COLORS[tool],
                })
                .collect();
            rows.sort_by(|a, b| b.cost.total_cmp(&a.cost));
            rows
        }
    }
}

const ROWS_Y: f32 = BOTTOM_Y + 101.0;
const ROW: f32 = 52.0;

/// How tall the card is with `rows` rows, which is never less than the heatmap beside it.
pub(crate) fn where_height(rows: usize) -> f32 {
    (ROWS_Y - BOTTOM_Y + rows as f32 * ROW + 52.0).max(BOTTOM_HEIGHT)
}

/// The middle of an option of the card, for a pointer.
pub(crate) fn dimension_at(dimension: usize) -> (f32, f32) {
    let total: f32 = DIMENSIONS.iter().map(|(_, width)| width).sum();
    let before: f32 = DIMENSIONS[..dimension].iter().map(|(_, width)| width).sum();
    (
        DIMENSIONS_RIGHT - 2.0 - total + before + DIMENSIONS[dimension].1 / 2.0,
        BOTTOM_Y + 34.0,
    )
}

/// The card of "Where it went" (`breakdown.tsx`), `height` tall, with `dimension` chosen;
/// the choice may be between two options while it slides.
pub(crate) fn where_surface<'a>(height: f32, dimension: f32) -> Svgr<'a> {
    fframes::svgr!(
        <g>
            {card(0.0, BOTTOM_Y, WHERE_WIDTH, height, "Where it went", "Choose a row to show only its usage on the whole page.")}
            {segments(DIMENSIONS_RIGHT, BOTTOM_Y + 20.0, &DIMENSIONS, dimension)}
        </g>
    )
}

/// The list of "Where it went": `rows` under `heading`, each with its bar grown to `grow` of
/// its length, its tokens, cost in kronor and share of `total` USD.
pub(crate) fn where_rows<'a>(heading: &'a str, rows: &[Row], total: f32, grow: f32) -> Svgr<'a> {
    let right = WHERE_WIDTH - 20.0;
    let max = rows.iter().map(|row| row.cost).fold(0.0_f32, f32::max);
    let tokens = |cost: f32| compact(data::TOTAL_TOKENS_B * 1e9 * cost / data::TOTAL_COST);
    let drawn: Vec<Svgr> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let y = ROWS_Y + i as f32 * ROW;
            let mark = match row.mark {
                Mark::Project => icon(icons::BRIEFCASE, 28.0, y + 10.0, 14.0, MUTED),
                Mark::Unassigned => icon(icons::CIRCLE_DASHED, 28.0, y + 10.0, 14.0, AMBER),
                Mark::Tool(tool) => provider_icon(tool, 28.0, y + 10.0, 14.0),
            };
            let under = match &row.under {
                Under::Repository(key) => repository(key, 52.0, y + 41.0, 12.0, SANS, MUTED),
                Under::Folded(text) => fframes::svgr!(<g>
                    {icon(icons::CHEVRON_RIGHT, 50.0, y + 30.0, 14.0, MUTED)}
                    <text x="66" y={y + 41.0} font-size="12" font-weight="500" fill={MUTED}>{text.clone()}</text>
                </g>),
                Under::Text(text) => fframes::svgr!(
                    <text x="52" y={y + 41.0} font-size="12" font-weight="500" fill={MUTED}>{text.clone()}</text>
                ),
            };
            let width = (174.0 * row.cost / max.max(0.001) * grow).max(2.0);
            fframes::svgr!(<g>
                {mark}
                <text x="52" y={y + 23.0} font-size="14" font-weight="500" fill={FG}>{row.label}</text>
                {under}
                <rect x={right - 446.0} y={y + 21.0} width={width} height="10" rx="2" fill={row.color} />
                <g font-size="14" font-weight="500">
                    <text x={right - 192.0} y={y + 31.0} text-anchor="end" fill={MUTED}>{tokens(row.cost)}</text>
                    <text x={right - 64.0} y={y + 31.0} text-anchor="end" fill={FG}>{money::shown(Currency::Sek, row.cost)}</text>
                    <text x={right - 8.0} y={y + 31.0} text-anchor="end" fill={MUTED}>{percent(row.cost / total)}</text>
                </g>
            </g>)
        })
        .collect();
    let foot = ROWS_Y + rows.len() as f32 * ROW;
    fframes::svgr!(
        <g font-family={SANS}>
            <g font-size="12" font-weight="500" fill={MUTED}>
                <text x="52" y={ROWS_Y - 11.0}>{heading}</text>
                <text x={right - 192.0} y={ROWS_Y - 11.0} text-anchor="end">"Tokens"</text>
                <text x={right - 64.0} y={ROWS_Y - 11.0} text-anchor="end">"Est. cost"</text>
                <text x={right} y={ROWS_Y - 11.0} text-anchor="end">"Share"</text>
            </g>
            {drawn}
            <path d={format!("M20 {foot} H{right}")} stroke={BORDER} stroke-width="1" />
            <g font-size="14" font-weight="600">
                <text x="52" y={foot + 27.0} fill={FG}>"All usage"</text>
                <text x={right - 192.0} y={foot + 27.0} text-anchor="end" fill={MUTED}>{tokens(total)}</text>
                <text x={right - 64.0} y={foot + 27.0} text-anchor="end" fill={FG}>{money::shown(Currency::Sek, total)}</text>
            </g>
        </g>
    )
}

/// A project key as the app shows it (`repo-key.tsx`): the host's mark in place of its
/// domain, then the path. `y` is the text's baseline.
pub(crate) fn repository<'a>(
    key: &'static str,
    x: f32,
    y: f32,
    size: f32,
    family: &'a str,
    ink: &'a str,
) -> Svgr<'a> {
    let (host, path) = match key.split_once('/') {
        Some(("github.com", path)) => (data::GITHUB_PATH, path.to_owned()),
        Some(("dev.azure.com", path)) => (data::AZURE_PATH, path.replace("/_git", "")),
        _ => ("", key.to_owned()),
    };
    // The app sets a key in its regular monospace weight where it is the subject of a row.
    let weight = if family == MONO { "400" } else { "500" };
    if host.is_empty() {
        return fframes::svgr!(<text x={x} y={y} font-family={family} font-size={size} font-weight={weight} fill={ink}>{path}</text>);
    }
    fframes::svgr!(
        <g>
            <path transform={format!("translate({x} {}) scale(0.5)", y - 10.5)} d={host} fill={ink} opacity="0.7" />
            <text x={x + 18.0} y={y} font-family={family} font-size={size} font-weight={weight} fill={ink}>{path}</text>
        </g>
    )
}

const HEAT_X: f32 = WHERE_WIDTH + 20.0;
const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// The hour heatmap (`hour-heatmap.tsx`), `height` tall: the week as a calendar, days across
/// and hours down, darker for less and lighter for more. `shown` seconds into appearing.
pub(crate) fn heatmap<'a>(hours: &[[f32; 24]; 7], height: f32, shown: f32) -> Svgr<'a> {
    let inner = HEAT_X + 20.0;
    let column = (1216.0 - 20.0 - inner - 28.0 - 14.0) / 7.0;
    let max = hours.iter().flatten().copied().fold(0.0_f32, f32::max);
    let days: Vec<Svgr> = WEEKDAYS
        .iter()
        .enumerate()
        .map(|(day, name)| {
            let x = inner + 30.0 + day as f32 * (column + 2.0);
            let cells: Vec<Svgr> = (0..24)
                .map(|hour| {
                    let cost = hours[day][hour];
                    let y = BOTTOM_Y + 112.0 + hour as f32 * 14.0;
                    let step = ((cost / max.max(0.001) * 7.0).floor() as usize).min(6);
                    let lit = if cost > 0.0 {
                        ease_out(progress(shown, 0.2 + day as f32 * 0.05 + hour as f32 * 0.025, 0.4))
                    } else {
                        0.0
                    };
                    fframes::svgr!(<g>
                        <rect x={x} y={y} width={column} height="12" rx="3" fill={MUTED_SURFACE} opacity="0.5" />
                        <rect x={x} y={y} width={column} height="12" rx="3" fill={SEQUENTIAL[step]} opacity={lit} />
                    </g>)
                })
                .collect();
            fframes::svgr!(<g>
                <text x={x + column / 2.0} y={BOTTOM_Y + 103.0} text-anchor="middle" font-size="10" font-weight="500" fill={MUTED}>{*name}</text>
                {cells}
            </g>)
        })
        .collect();
    let hours_named: Vec<Svgr> = (0..8)
        .map(|i| {
            fframes::svgr!(<text x={inner + 22.0} y={BOTTOM_Y + 121.0 + i as f32 * 42.0} text-anchor="end" font-size="10" font-weight="500" fill={MUTED}>{format!("{:02}", i * 3)}</text>)
        })
        .collect();
    let legend_y = BOTTOM_Y + 112.0 + 24.0 * 14.0 + 14.0;
    let ramp: Vec<Svgr> = SEQUENTIAL
        .iter()
        .enumerate()
        .map(|(i, color)| fframes::svgr!(<rect x={inner + 32.0 + i as f32 * 14.0} y={legend_y} width="12" height="10" rx="2" fill={*color} />))
        .collect();
    fframes::svgr!(
        <g font-family={SANS}>
            {card(HEAT_X, BOTTOM_Y, 1216.0 - HEAT_X, height, "When you use AI", "Per hour of the week, in")}
            <text x={inner} y={BOTTOM_Y + 71.0} font-size="12" font-weight="500" fill={MUTED}>"Europe/Stockholm."</text>
            {segments(1196.0, BOTTOM_Y + 20.0, &[("Spend", 58.0), ("Tokens", 60.0)], 0.0)}
            {days}
            {hours_named}
            <g font-size="12" font-weight="500" fill={MUTED}>
                <text x={inner} y={legend_y + 9.5}>"Less"</text>
                {ramp}
                <text x={inner + 134.0} y={legend_y + 9.5}>{format!("More, up to {}", money::shown(Currency::Sek, max))}</text>
            </g>
        </g>
    )
}

/// A pointer in page pixels that rests at each of `stops` and moves to the next one during
/// the matching span of `moves`, given as its start and how long it takes.
pub(crate) fn travel(t: f32, stops: &[(f32, f32)], moves: &[(f32, f32)]) -> (f32, f32) {
    let mut position = stops[0];
    for (i, (start, duration)) in moves.iter().enumerate() {
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
