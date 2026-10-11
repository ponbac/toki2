//! The opening: a question, the figure that answers it counting up, and what the page is.
use crate::*;

const SECONDS: f32 = 6.0;
/// The settled moment of the scene that stands in as frame 0.
const POSTER: f32 = 5.0;

#[derive(Debug)]
pub struct Intro;

impl Scene for Intro {
    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(SECONDS)
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        // Frame 0 is the poster: players show it as the thumbnail, so it is the settled
        // opening instead of the first instant of its fade-in.
        let poster = frame.global_index == 0;
        let frame = if poster {
            Frame::new(frame.second_to_frame(POSTER), 0, frame.fps)
        } else {
            frame
        };
        let t = frame.seconds();
        let count = ease_out(progress(t, 0.9, 2.0));
        let tools: Vec<Svgr> = (0..4)
            .map(|i| {
                let start = 2.5 + i as f32 * 0.12;
                fframes::svgr!(<g opacity={ramp(&frame, start)} transform={Transform::translate(0, rise(&frame, start, 14.0))}>
                    {provider_icon(i, 846.0 + i as f32 * 64.0, 760.0, 36.0)}
                </g>)
            })
            .collect();

        fframes::svgr!(
            <g opacity={progress(t, 0.0, 0.3).min(1.0 - progress(t, SECONDS - 0.35, 0.35))} font-family={SANS} text-anchor="middle">
                <g opacity={ramp(&frame, 0.15)} transform={Transform::translate(0, rise(&frame, 0.15, 24.0))}>
                    <text x="960" y="330" font-size="56" font-weight="600" letter-spacing="-1" fill={MUTED}>"Vad kostar din AI-användning?"</text>
                </g>
                <g opacity={ramp(&frame, 0.8)} transform={Transform::translate(0, rise(&frame, 0.8, 30.0))}>
                    <text x="960" y="540" font-size="210" font-weight="800" letter-spacing="-8" fill={FG}>{usd(data::TOTAL_COST * count)}</text>
                </g>
                <g opacity={ramp(&frame, 2.3)} transform={Transform::translate(0, rise(&frame, 2.3, 18.0))}>
                    <rect x="700" y="610" width="230" height="56" rx="28" fill={AMBER} />
                    <text x="815" y="648" font-size="27" font-weight="800" fill={BG}>"NYTT I TOKI"</text>
                    <text x="952" y="651" text-anchor="start" font-size="40" font-weight="700" letter-spacing="-0.6" fill={FG}>"AI usage"</text>
                    <text x="960" y="716" font-size="26" font-weight="500" fill={MUTED}>"Uppskattad API-kostnad per dag, verktyg och projekt. Exempeldata i filmen."</text>
                </g>
                {tools}
            </g>
        )
    }
}
