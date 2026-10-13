//! Why the page exists, right after the opening: AI costs are billed on to the customers,
//! from real usage, which is why usage has to find its way from a tool to a customer project.
use crate::*;

const SECONDS: f32 = 7.5;
/// The three stops of a cost on its way to a bill, in video pixels.
const STOP_WIDTH: f32 = 480.0;
const STOP_GAP: f32 = 90.0;
const STOPS_Y: f32 = 680.0;

#[derive(Debug)]
pub struct Purpose;

fn stop_x(stop: usize) -> f32 {
    150.0 + stop as f32 * (STOP_WIDTH + STOP_GAP)
}

/// One stop: a card with its label, appearing at `start`.
fn stop<'a>(
    frame: &Frame,
    index: usize,
    start: f32,
    label: &'a str,
    content: Svgr<'a>,
) -> Svgr<'a> {
    let x = stop_x(index);
    fframes::svgr!(
        <g opacity={ramp(frame, start)} transform={Transform::translate(0, rise(frame, start, 24.0))}>
            <rect x={x} y={STOPS_Y} width={STOP_WIDTH} height="170" rx="24" fill={CARD} stroke={BORDER} stroke-width="1.5" />
            <text x={x + 36.0} y={STOPS_Y + 54.0} font-size="22" font-weight="500" fill={MUTED}>{label}</text>
            {content}
        </g>
    )
}

/// The arrow from one stop to the next, drawn from `start`.
fn arrow<'a>(t: f32, from: usize, start: f32) -> Svgr<'a> {
    let x = stop_x(from) + STOP_WIDTH + 14.0;
    let y = STOPS_Y + 85.0;
    let drawn = ease_out(progress(t, start, 0.5));
    let end = x + (STOP_GAP - 28.0) * drawn;
    fframes::svgr!(
        <g opacity={progress(t, start, 0.1)}>
            <path d={format!("M{x} {y} H{end}")} stroke={AMBER} stroke-width="4" stroke-linecap="round" />
            <path d={format!("M{} {} L{end} {y} L{} {}", end - 12.0, y - 11.0, end - 12.0, y + 11.0)} fill="none" stroke={AMBER} stroke-width="4" stroke-linecap="round" stroke-linejoin="round" />
        </g>
    )
}

impl Scene for Purpose {
    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(SECONDS)
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = frame.seconds();
        let tools: Vec<Svgr> = (0..4)
            .map(|i| provider_icon(i, stop_x(0) + 36.0 + i as f32 * 62.0, STOPS_Y + 82.0, 42.0))
            .collect();

        fframes::svgr!(
            <g opacity={scene_fade(t, SECONDS)} font-family={SANS}>
                <g opacity={ramp(&frame, 0.15)} transform={Transform::translate(0, rise(&frame, 0.15, 18.0))}>
                    <rect x="150" y="212" width="150" height="56" rx="28" fill={AMBER} />
                    <text x="225" y="250" text-anchor="middle" font-size="27" font-weight="800" fill={BG}>"SYFTET"</text>
                </g>
                <g opacity={ramp(&frame, 0.35)} transform={Transform::translate(0, rise(&frame, 0.35, 24.0))}>
                    <text x="146" y="380" font-size="76" font-weight="800" letter-spacing="-2.4" fill={FG}>"Vi fakturerar vidare våra AI-kostnader"</text>
                </g>
                <g opacity={ramp(&frame, 0.6)} transform={Transform::translate(0, rise(&frame, 0.6, 24.0))}>
                    <text x="146" y="472" font-size="76" font-weight="800" letter-spacing="-2.4" fill={FG}>"till rätt kund, utifrån verklig användning."</text>
                </g>
                <g opacity={ramp(&frame, 1.2)} transform={Transform::translate(0, rise(&frame, 1.2, 18.0))}>
                    <text x="150" y="556" font-size="30" font-weight="500" fill={MUTED}>"Därför samlar Toki in vad varje verktyg har använts till, per repo och projekt."</text>
                </g>
                {stop(&frame, 0, 2.0, "AI-verktyg", fframes::svgr!(<g>{tools}</g>))}
                {arrow(t, 0, 2.5)}
                {stop(&frame, 1, 2.7, "Repo", fframes::svgr!(
                    <text x={stop_x(1) + 36.0} y={STOPS_Y + 116.0} font-family={MONO} font-size="25" font-weight="500" fill={FG}>"spinit-ab/nordvik-portal-api"</text>
                ))}
                {arrow(t, 1, 3.2)}
                {stop(&frame, 2, 3.4, "Kundprojekt", fframes::svgr!(
                    <text x={stop_x(2) + 36.0} y={STOPS_Y + 118.0} font-size="30" font-weight="600" letter-spacing="-0.4" fill={FG}>{data::PROJECTS[0].0}</text>
                ))}
            </g>
        )
    }
}
