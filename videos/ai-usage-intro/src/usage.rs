//! The top of the usage page: summary tiles and the daily spend chart. `dashboard` draws it
//! for any moment of its entrance, so the scene after the first sync can fill it in; this
//! scene starts from the filled page and re-stacks the days by model on a click.
use crate::*;

const SECONDS: f32 = 10.0;
const TILE_WIDTH: f32 = 295.0;
const TILE_HEIGHT: f32 = 120.0;
const TILE_GAP: f32 = 12.0;

const CHART_Y: f32 = 140.0;
const CHART_HEIGHT: f32 = 385.0;
const PLOT_LEFT: f32 = 72.0;
const PLOT_RIGHT: f32 = 1192.0;
const PLOT_TOP: f32 = CHART_Y + 88.0;
const PLOT_BOTTOM: f32 = CHART_Y + 308.0;
const AXIS_MAX: f32 = 600.0;
const BAR_WIDTH: f32 = 13.0;

const MODEL_AT: f32 = 4.4;
/// The Model option of the chart's "Stack by" control.
const MODEL_OPTION: (f32, f32) = (1056.0, CHART_Y + 34.0);
/// How long the page takes to fill in; past it, `dashboard` is settled.
pub(crate) const FILLED: f32 = 4.0;

#[derive(Debug)]
pub struct Usage;

/// A summary tile. `detail` wraps where it has a line break, as the app wraps it at this width.
fn tile<'a>(index: usize, label: &'a str, value: String, detail: String) -> Svgr<'a> {
    let x = index as f32 * (TILE_WIDTH + TILE_GAP);
    let (detail, second_line) = match detail.split_once('\n') {
        Some((first, second)) => (first.to_owned(), second.to_owned()),
        None => (detail, String::new()),
    };
    fframes::svgr!(
        <g font-family={SANS}>
            <rect x={x} y="0" width={TILE_WIDTH} height={TILE_HEIGHT} rx="16" fill={CARD} stroke={BORDER} stroke-width="1" />
            <text x={x + 16.0} y="29" font-size="12" font-weight="500" fill={MUTED}>{label}</text>
            <text x={x + 16.0} y="61" font-size="24" font-weight="600" letter-spacing="-0.6" fill={FG}>{value}</text>
            <text x={x + 16.0} y="84" font-size="12" font-weight="500" fill={MUTED}>{detail}</text>
            <text x={x + 16.0} y="100" font-size="12" font-weight="500" fill={MUTED}>{second_line}</text>
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

/// One day's stacked bar: `parts` from the bottom up, grown to `grow` of its height.
fn bar<'a>(x: f32, parts: [f32; 4], colors: &[&'a str; 4], scale: f32, grow: f32) -> Svgr<'a> {
    let mut bottom = PLOT_BOTTOM;
    let last = parts.iter().rposition(|cost| *cost > 0.0).unwrap_or(0);
    parts
        .iter()
        .enumerate()
        .map(|(i, cost)| {
            let height = cost * scale * grow;
            let top = bottom - height;
            // A 1.5 px gap of card surface separates the segments, as in the app.
            let gap = if i == last {
                0.0
            } else {
                1.5_f32.min(height / 2.0)
            };
            let radius = if i == last { 3.0_f32.min(height) } else { 0.0 };
            let path = segment(x, top + gap, bottom, radius);
            bottom = top;
            fframes::svgr!(<path d={path} fill={colors[i]} />)
        })
        .collect()
}

/// The tiles and the daily chart in page pixels, `t` seconds into filling in: the figures
/// count up and the bars grow from the left. `by_model` runs from 0, stacked by tool, to 1,
/// stacked by model.
pub(crate) fn dashboard<'a>(t: f32, by_model: f32) -> Svgr<'a> {
    let days = &*data::DAYS_OF_USAGE;
    let count = ease_out(progress(t, 0.3, 1.6));
    let unassigned_share = data::UNASSIGNED_COST / data::TOTAL_COST * 100.0;
    let scale = (PLOT_BOTTOM - PLOT_TOP) / AXIS_MAX;
    let band = (PLOT_RIGHT - PLOT_LEFT) / days.len() as f32;

    let grid: Vec<Svgr> = (0..=4)
        .map(|i| {
            let value = i as f32 * 150.0;
            let y = PLOT_BOTTOM - value * scale;
            fframes::svgr!(<g>
                <path d={format!("M{PLOT_LEFT} {y} H{PLOT_RIGHT}")} stroke={GRID} stroke-width="1" />
                <text x={PLOT_LEFT - 10.0} y={y + 4.0} text-anchor="end" font-size="11" font-weight="500" fill={MUTED}>{format!("${value:.0}")}</text>
            </g>)
        })
        .collect();

    let bars: Vec<Svgr> = days
        .iter()
        .enumerate()
        .map(|(i, day)| {
            let grow = ease_out(progress(t, 0.5 + i as f32 * 0.028, 0.75));
            let x = PLOT_LEFT + i as f32 * band + (band - BAR_WIDTH) / 2.0;
            let tick = if day.weekday == 0 {
                fframes::svgr!(<text x={x + BAR_WIDTH / 2.0} y={PLOT_BOTTOM + 20.0} text-anchor="middle" font-size="11" font-weight="500" fill={MUTED}>{day.label.as_str()}</text>)
            } else {
                Svgr::empty()
            };
            fframes::svgr!(<g>
                <g opacity={1.0 - by_model}>{bar(x, day.cost, &PROVIDER_COLORS, scale, grow)}</g>
                <g opacity={by_model}>{bar(x, day.by_model(), &data::MODEL_COLORS, scale, grow)}</g>
                {tick}
            </g>)
        })
        .collect();

    let tools: Vec<Svgr> = (0..4)
        .map(|i| {
            let x = 20.0 + [0.0, 72.0, 180.0, 258.0][i];
            fframes::svgr!(<g>
                {provider_icon(i, x, CHART_Y + 345.0, 14.0)}
                <text x={x + 20.0} y={CHART_Y + 356.5} font-size="12" font-weight="500" fill={MUTED}>{PROVIDER_NAMES[i]}</text>
            </g>)
        })
        .collect();
    let models: Vec<Svgr> = (0..4)
        .map(|i| {
            let x = 20.0 + [0.0, 100.0, 228.0, 328.0][i];
            fframes::svgr!(<g>
                <rect x={x} y={CHART_Y + 347.0} width="10" height="10" rx="3" fill={data::MODEL_COLORS[i]} />
                <text x={x + 16.0} y={CHART_Y + 356.5} font-size="12" font-weight="500" fill={MUTED}>{data::MODELS[i]}</text>
            </g>)
        })
        .collect();

    fframes::svgr!(
        <g font-family={SANS}>
            {tile(0, "Estimated cost", usd(data::TOTAL_COST * count), "API-equivalent, in USD. Not a subscription bill.".to_owned())}
            {tile(1, "Tokens", format!("{:.1}B", data::TOTAL_TOKENS_B * count), format!("{:.1}B of them cache reads", data::CACHE_READ_TOKENS_B * count))}
            {tile(2, "Requests", thousands((data::REQUESTS * count) as i64), format!("{} active days", (data::DAYS as f32 * count) as usize))}
            {tile(3, "Unassigned", usd(data::UNASSIGNED_COST * count), format!("{unassigned_share:.1}% of the known cost has no mapped\nproject"))}
            {card(0.0, CHART_Y, 1216.0, CHART_HEIGHT, "Estimated spend per day", "API-equivalent estimate in USD, by local day in Europe/Stockholm. Not a bill.")}
            {segmented(1196.0, CHART_Y + 20.0, "Chart", "Table", 50.0, 50.0, 0.0)}
            {segmented(1084.0, CHART_Y + 20.0, "Tool", "Model", 44.0, 52.0, by_model)}
            {grid}
            {bars}
            <g opacity={1.0 - by_model}>{tools}</g>
            <g opacity={by_model}>{models}</g>
        </g>
    )
}

impl Scene for Usage {
    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(SECONDS)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::from([AudioTrack::new("click.wav", Second(MODEL_AT)..Eof).gain_db(-7.0)])
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = frame.seconds();
        let by_model = ease_in_out(progress(t, MODEL_AT, 0.4));

        // The pointer comes in from the right and presses Model.
        let reach = ease_in_out(progress(t, 3.3, 0.9));
        let leave = ease_in_out(progress(t, MODEL_AT + 0.7, 1.0));
        let pointer_x = lerp(lerp(1150.0, MODEL_OPTION.0, reach), 1100.0, leave);
        let pointer_y = lerp(lerp(330.0, MODEL_OPTION.1, reach), 250.0, leave);

        fframes::svgr!(
            <g opacity={scene_fade(t, SECONDS)}>
                {caption_sequence(&frame, "4", "Se din användning", &[
                    ("Uppskattad API-kostnad per dag och verktyg. Inte en faktura, utan vad det hade kostat via API.", 0.0),
                    ("Samma dagar, staplade per modell i stället.", MODEL_AT + 0.3),
                ])}
                <g transform={page_transform()}>
                    <g opacity={ramp(&frame, 0.2)} transform={Transform::translate(0, rise(&frame, 0.2, 18.0))}>
                        {dashboard(FILLED, by_model)}
                    </g>
                    {pointer(t, pointer_x, pointer_y, &[MODEL_AT], progress(t, 3.0, 0.3).min(1.0 - progress(t, MODEL_AT + 1.4, 0.4)))}
                </g>
            </g>
        )
    }
}
