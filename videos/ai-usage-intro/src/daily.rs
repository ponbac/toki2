//! The Overview's daily chart, with the summary strip above it: the days stacked by tool,
//! then by model, where each model is a tint of its tool's colour, and what a day cost as
//! the pointer passes over it.
use crate::page::{self, Figures};
use crate::*;

const SECONDS: f32 = 12.5;
const MODEL_AT: f32 = 3.6;
/// When the pointer enters the plot, and the days it passes over before it rests.
const ENTER_AT: f32 = 6.2;
const FROM_DAY: usize = 41;
const TO_DAY: usize = 26;
const TOOLTIP_WIDTH: f32 = 196.0;
const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

#[derive(Debug)]
pub struct Daily;

/// The chart's tooltip (`ChartTooltipFrame`) for day `index`, stacked by model: each series
/// from the top of the bar down, then the day's total.
fn tooltip<'a>(index: usize, x: f32, y: f32) -> Svgr<'a> {
    let day = &data::DAYS_OF_USAGE[index];
    let models = day.by_model();
    let rows: Vec<Svgr> = (0..models.len())
        .rev()
        .filter(|model| models[*model] > 0.0)
        .enumerate()
        .map(|(row, model)| {
            let at = y + 42.0 + row as f32 * 20.0;
            let value = money::shown(Currency::Sek, models[model]);
            let digits = value.chars().filter(|c| *c != ' ').count() as f32;
            let label_x = x + 34.0 + digits * 6.9 + (value.chars().count() as f32 - digits) * 3.2 + 8.0;
            fframes::svgr!(<g>
                <rect x={x + 12.0} y={at - 5.0} width="12" height="2" rx="1" fill={data::MODEL_COLORS[model]} />
                <text x={x + 34.0} y={at} font-weight="600" fill={FG}>{value}</text>
                <text x={label_x} y={at} font-weight="500" fill={MUTED}>{data::MODELS[model]}</text>
            </g>)
        })
        .collect();
    let rows_end = y + 30.0 + rows.len() as f32 * 20.0;
    // The range starts on a Friday.
    let weekday = WEEKDAYS[(4 + index) % 7];
    fframes::svgr!(
        <g font-family={SANS} font-size="12">
            <rect x={x + 2.0} y={y + 6.0} width={TOOLTIP_WIDTH} height={rows_end - y + 38.0} rx="10" fill="#000000" opacity="0.35" />
            <rect x={x} y={y} width={TOOLTIP_WIDTH} height={rows_end - y + 38.0} rx="8" fill={POPOVER} stroke={BORDER} stroke-width="1" />
            <text x={x + 12.0} y={y + 21.0} font-weight="500" fill={FG}>{format!("{weekday}, {} 2026", day.label)}</text>
            {rows}
            <path d={format!("M{} {} h{}", x + 12.0, rows_end + 4.0, TOOLTIP_WIDTH - 24.0)} stroke={BORDER} stroke-width="1" opacity="0.6" />
            <text x={x + 12.0} y={rows_end + 24.0} font-weight="500" fill={MUTED}>"Total"</text>
            <text x={x + 46.0} y={rows_end + 24.0} font-weight="600" fill={FG}>{money::shown(Currency::Sek, day.total())}</text>
        </g>
    )
}

impl Scene for Daily {
    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(SECONDS)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::from([AudioTrack::new("click.wav", Second(MODEL_AT)..Eof).gain_db(-7.0)])
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = frame.seconds();
        // The bars grow to their height from the left as the scene opens.
        let days = data::DAYS_OF_USAGE
            .iter()
            .enumerate()
            .map(|(i, day)| {
                let grow = ease_out(progress(t, 0.5 + i as f32 * 0.02, 0.7));
                day.cost.map(|cost| cost * grow)
            })
            .collect();
        let chart = page::Daily {
            days,
            by_model: ease_in_out(progress(t, MODEL_AT, 0.45)),
            ..page::Daily::all()
        };

        // The pointer presses Model, comes down into the plot, and passes slowly over two
        // weeks of days before it rests.
        let hover_y = page::CHART_Y + 236.0;
        let (pointer_x, pointer_y) = page::travel(
            t,
            &[
                (1160.0, 560.0),
                page::MODEL_OPTION,
                (page::day_x(FROM_DAY) + 7.5, hover_y),
                (page::day_x(TO_DAY) + 7.5, hover_y),
            ],
            &[(2.5, 0.9), (ENTER_AT - 0.5, 1.2), (ENTER_AT + 1.1, 3.2)],
        );
        // The tooltip belongs to the day under the pointer, from the moment it is over one.
        let over = (t >= ENTER_AT && pointer_y > page::PLOT_TOP_Y)
            .then(|| page::day_under(pointer_x))
            .flatten();
        let hover = over.map(|day| {
            let bar = page::day_x(day) + 7.5;
            // It opens to the right of the day, or to the left where the card ends.
            let x = if bar + 18.0 + TOOLTIP_WIDTH > 1190.0 {
                bar - 18.0 - TOOLTIP_WIDTH
            } else {
                bar + 18.0
            };
            fframes::svgr!(<g>
                <rect x={bar - 9.5} y={page::CHART_Y + 86.0} width="19" height="224" rx="3" fill={FG} opacity="0.06" />
                {tooltip(day, x, page::CHART_Y + 96.0)}
            </g>)
        });

        fframes::svgr!(
            <g opacity={scene_fade(t, SECONDS)}>
                {caption_sequence(&frame, "5", "Dag för dag", &[
                    ("En stapel per dag, färgad efter verktyg.", 0.0),
                    ("Eller per modell. Varje modell är en nyans av sitt verktygs färg.", MODEL_AT + 0.3),
                    ("Peka på en dag för att se vad den kostade.", ENTER_AT - 0.2),
                ])}
                <g opacity={ramp(&frame, 0.2)} transform={Transform::translate(0, rise(&frame, 0.2, 18.0))}>
                    {page::view(page::AT_CHART, fframes::svgr!(<g>
                        {page::filters()}
                        {page::summary(&Figures::all(), Currency::Sek)}
                        {page::daily(&chart)}
                        {hover.unwrap_or_default()}
                        {pointer(t, pointer_x, pointer_y, &[MODEL_AT], progress(t, 2.2, 0.3).min(1.0 - progress(t, SECONDS - 0.9, 0.4)))}
                    </g>))}
                </g>
            </g>
        )
    }
}
