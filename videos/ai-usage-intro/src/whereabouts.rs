//! The bottom row of the Overview: "Where it went" per project, then per model and per tool,
//! beside the hours of the week.
use crate::page;
use crate::*;

const SECONDS: f32 = 11.0;
const MODEL_AT: f32 = 3.6;
const TOOL_AT: f32 = 6.0;
const PROJECT_AT: f32 = 8.2;
/// When each way to break the usage down is chosen: project, model, tool, project again.
const CHOSEN: [(f32, usize); 4] = [(0.0, 0), (MODEL_AT, 1), (TOOL_AT, 2), (PROJECT_AT, 0)];
const HEADINGS: [&str; 3] = ["Project", "Model", "Tool"];

#[derive(Debug)]
pub struct Whereabouts {
    rows: [Vec<page::Row>; 3],
    hours: [[f32; 24]; 7],
}

impl Whereabouts {
    pub fn new() -> Self {
        Self {
            rows: [page::rows(0), page::rows(1), page::rows(2)],
            hours: data::hours(),
        }
    }
}

impl Scene for Whereabouts {
    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(SECONDS)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap(Some(
            [MODEL_AT, TOOL_AT, PROJECT_AT]
                .iter()
                .map(|at| AudioTrack::new("click.wav", Second(*at)..Eof).gain_db(-7.0))
                .collect(),
        ))
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = frame.seconds();
        // The last choice made, the one before it, and how far the list is into changing.
        let step = CHOSEN.iter().rposition(|(at, _)| t >= *at).unwrap_or(0);
        let (since, shown) = CHOSEN[step];
        let before = CHOSEN[step.saturating_sub(1)].1;
        let changed = if step == 0 {
            1.0
        } else {
            progress(t, since, 0.3)
        };
        let dimension = lerp(before as f32, shown as f32, ease_in_out(changed));
        let height = lerp(
            page::where_height(self.rows[before].len()),
            page::where_height(self.rows[shown].len()),
            ease_in_out(changed),
        );
        let grow = ease_out(progress(t, since + 0.25, 0.55));

        let (pointer_x, pointer_y) = page::travel(
            t,
            &[
                (700.0, page::AT_BOTTOM + 470.0),
                page::dimension_at(1),
                page::dimension_at(2),
                page::dimension_at(0),
                (640.0, page::AT_BOTTOM + 420.0),
            ],
            &[(2.6, 0.8), (5.1, 0.7), (7.3, 0.7), (PROJECT_AT + 0.8, 1.0)],
        );

        fframes::svgr!(
            <g opacity={scene_fade(t, SECONDS)}>
                {caption_sequence(&frame, "5", "Vart kostnaden tog vägen", &[
                    ("Störst först, med stapeln på samma rad. Till höger: när i veckan du använder AI.", 0.0),
                    ("Samma lista per modell eller verktyg.", MODEL_AT + 0.2),
                ])}
                <g opacity={ramp(&frame, 0.2)} transform={Transform::translate(0, rise(&frame, 0.2, 18.0))}>
                    {page::view(page::AT_BOTTOM, fframes::svgr!(<g>
                        {page::heatmap(&self.hours, height, t)}
                        {page::where_surface(height, dimension)}
                        // The list that is leaving fades under the one that is arriving.
                        <g opacity={1.0 - changed}>{page::where_rows(HEADINGS[before], &self.rows[before], data::TOTAL_COST, 1.0)}</g>
                        <g opacity={changed}>{page::where_rows(HEADINGS[shown], &self.rows[shown], data::TOTAL_COST, grow)}</g>
                        {pointer(t, pointer_x, pointer_y, &[MODEL_AT, TOOL_AT, PROJECT_AT], progress(t, 2.3, 0.3).min(1.0 - progress(t, SECONDS - 1.4, 0.4)))}
                    </g>))}
                </g>
            </g>
        )
    }
}
