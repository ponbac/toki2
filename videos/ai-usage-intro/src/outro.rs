//! The close: the four things to do, and where.
use crate::*;

const SECONDS: f32 = 6.5;
const STEPS: [(&str, &str); 4] = [
    ("Koppla din dator", "Guiden på sidan AI usage"),
    ("Låt den synka", "Varje dag 12:30, av sig själv"),
    ("Ange abonnemang", "Under Account, per verktyg"),
    ("Koppla dina repon", "Kostnaden hamnar på rätt projekt"),
];

#[derive(Debug)]
pub struct Outro;

impl Scene for Outro {
    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(SECONDS)
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = frame.seconds();
        let steps: Vec<Svgr> = STEPS
            .iter()
            .enumerate()
            .map(|(i, (title, detail))| {
                let start = 0.7 + i as f32 * 0.3;
                let x = 150.0 + i as f32 * 410.0;
                fframes::svgr!(<g opacity={ramp(&frame, start)} transform={Transform::translate(0, rise(&frame, start, 24.0))}>
                    <rect x={x} y="430" width="390" height="210" rx="24" fill={CARD} stroke={BORDER} stroke-width="1.5" />
                    <circle cx={x + 56.0} cy="494" r="26" fill={AMBER} />
                    <text x={x + 56.0} y="505" text-anchor="middle" font-size="29" font-weight="800" fill={BG}>{format!("{}", i + 1)}</text>
                    <text x={x + 30.0} y="574" font-size="33" font-weight="700" letter-spacing="-0.6" fill={FG}>{*title}</text>
                    <text x={x + 30.0} y="610" font-size="21" font-weight="500" fill={MUTED}>{*detail}</text>
                </g>)
            })
            .collect();

        fframes::svgr!(
            <g opacity={progress(t, 0.0, 0.4).min(1.0 - progress(t, SECONDS - 0.5, 0.5))} font-family={SANS}>
                <g opacity={ramp(&frame, 0.15)} transform={Transform::translate(0, rise(&frame, 0.15, 24.0))}>
                    <text x="150" y="330" font-size="92" font-weight="800" letter-spacing="-3" fill={FG}>"Det tar två minuter."</text>
                </g>
                {steps}
                <g opacity={ramp(&frame, 2.0)} transform={Transform::translate(0, rise(&frame, 2.0, 18.0))}>
                    <rect x="150" y="730" width="590" height="76" rx="38" fill="#0b0d13" stroke={BORDER} stroke-width="1.5" />
                    <text x="190" y="779" font-family={MONO} font-size="32" font-weight="500" fill={FG}>"toki.spinit.se/ai-usage"</text>
                    <rect x="668" y="752" width="14" height="32" fill={AMBER} opacity={if (t * 1.6).fract() < 0.55 { 0.9 } else { 0.0 }} />
                </g>
            </g>
        )
    }
}
