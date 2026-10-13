//! The top of the Overview tab: the header with its currency switch, the tabs, the filters
//! and the summary strip. The pointer tries US dollars and returns to kronor, and the four
//! tabs are pointed out.
use crate::page::{self, Figures, Waiting};
use crate::*;

const SECONDS: f32 = 13.5;
const DOLLARS_AT: f32 = 4.4;
const KRONOR_AT: f32 = 6.6;
const TABS_AT: f32 = 8.6;

#[derive(Debug)]
pub struct Overview;

impl Scene for Overview {
    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(SECONDS)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::from([
            AudioTrack::new("click.wav", Second(DOLLARS_AT)..Eof).gain_db(-7.0),
            AudioTrack::new("click.wav", Second(KRONOR_AT)..Eof).gain_db(-7.0),
        ])
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = frame.seconds();
        // The figures roll from the currency they were in to the one just chosen.
        let (from, to, rolled) = if t < KRONOR_AT {
            (Currency::Sek, Currency::Usd, progress(t, DOLLARS_AT, 0.3))
        } else {
            (Currency::Usd, Currency::Sek, progress(t, KRONOR_AT, 0.3))
        };
        let currency = if rolled > 0.0 { to } else { from };
        let figures = Figures::all();
        let axis = match currency {
            Currency::Sek => page::ALL_AXIS,
            Currency::Usd => page::ALL_AXIS / 10.0,
        };
        let chart = page::Daily {
            currency,
            axis,
            ..page::Daily::all()
        };

        let (pointer_x, pointer_y) = page::travel(
            t,
            &[
                (1170.0, 300.0),
                page::DOLLARS_OPTION,
                page::KRONOR_OPTION,
                (1120.0, 150.0),
            ],
            &[(3.3, 0.9), (5.6, 0.7), (7.3, 0.9)],
        );
        // The tabs are pointed out by a ring that comes and goes.
        let ring = (progress(t, TABS_AT, 2.4) * std::f32::consts::PI).sin();

        fframes::svgr!(
            <g opacity={scene_fade(t, SECONDS)}>
                {caption_sequence(&frame, "5", "Se din användning", &[
                    ("Överst på sidan: uppskattad API-kostnad, tokens, anrop och det som saknar projekt.", 0.0),
                    ("Kostnaden visas i kronor till månadens kurs. Byt till USD eller EUR uppe till höger.", DOLLARS_AT - 0.9),
                    ("Sidan har fyra flikar: översikt, sessioner, repon och datorer. Bara du ser dina timmar och sessioner.", TABS_AT - 0.2),
                ])}
                <g opacity={ramp(&frame, 0.2)} transform={Transform::translate(0, rise(&frame, 0.2, 18.0))}>
                    {page::view(page::AT_TOP, fframes::svgr!(<g>
                        {page::header(currency)}
                        {page::tabs(0.0, &Waiting::all())}
                        {page::filters()}
                        {page::summary_surface()}
                        {page::summary_labels(true)}
                        {page::roll(page::summary_tile(0, &figures, from), page::summary_tile(0, &figures, to), rolled)}
                        {page::summary_tile(1, &figures, currency)}
                        {page::summary_tile(2, &figures, currency)}
                        {page::roll(page::summary_tile(3, &figures, from), page::summary_tile(3, &figures, to), rolled)}
                        {page::daily(&chart)}
                        <rect x="-10" y={page::TABS_Y - 4.0} width="562" height="50" rx="12" fill="none" stroke={AMBER} stroke-width="1.5" opacity={ring * 0.9} />
                        {pointer(t, pointer_x, pointer_y, &[DOLLARS_AT, KRONOR_AT], progress(t, 3.0, 0.3).min(1.0 - progress(t, 8.3, 0.4)))}
                    </g>))}
                </g>
            </g>
        )
    }
}
