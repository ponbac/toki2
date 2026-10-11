//! The last step of the dialog, back from the terminal: the machine's first sync has landed,
//! so the dialog says it is connected, and Show my usage closes it over a page that fills in.
use crate::connect::{CONTENT_TOP, DIALOG_CENTER_Y, DIALOG_WIDTH, DIALOG_ZOOM, SHORT};
use crate::*;

const SECONDS: f32 = 9.5;
const CONNECTED_AT: f32 = 0.7;
const SHOW_AT: f32 = 3.8;

#[derive(Debug)]
pub struct Synced;

impl Scene for Synced {
    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(SECONDS)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::from([
            AudioTrack::new("chime.wav", Second(CONNECTED_AT)..Eof).gain_db(-6.0),
            AudioTrack::new("click.wav", Second(SHOW_AT)..Eof).gain_db(-7.0),
        ])
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = frame.seconds();
        let closed = progress(t, SHOW_AT, 0.25);
        let filling = t - SHOW_AT - 0.1;

        let read: Vec<Svgr> = [(0, 0.0), (1, 62.0), (3, 162.0)]
            .iter()
            .map(|(provider, offset)| {
                let x = 158.0 + offset;
                fframes::svgr!(<g>
                    {provider_icon(*provider, x, CONTENT_TOP + 43.0, 12.0)}
                    <text x={x + 17.0} y={CONTENT_TOP + 53.0} font-size="12" font-weight="500" fill={MUTED}>{PROVIDER_NAMES[*provider]}</text>
                </g>)
            })
            .collect();

        // The pointer goes to Show my usage, in video pixels.
        let target = (
            960.0 + (DIALOG_WIDTH - 24.0 - 68.0 - DIALOG_WIDTH / 2.0) * DIALOG_ZOOM,
            DIALOG_CENTER_Y + (SHORT - 44.0 - SHORT / 2.0) * DIALOG_ZOOM,
        );
        let reach = ease_in_out(progress(t, 2.5, 0.9));
        let (pointer_x, pointer_y) = (lerp(1340.0, target.0, reach), lerp(930.0, target.1, reach));

        let dialog = format!(
            "translate(960 {DIALOG_CENTER_Y}) scale({}) translate({} {})",
            DIALOG_ZOOM * lerp(1.0, 0.95, closed),
            -DIALOG_WIDTH / 2.0,
            -SHORT / 2.0
        );
        let connected = ramp(&frame, CONNECTED_AT);

        fframes::svgr!(
            <g opacity={scene_fade(t, SECONDS)}>
                <g transform={page_transform()}>
                    <g opacity={1.0 - progress(t, SHOW_AT + 0.1, 0.4)}>{connect::preview()}</g>
                    <g opacity={progress(t, SHOW_AT + 0.1, 0.4)}>{usage::dashboard(filling, 0.0)}</g>
                </g>
                <rect width="1920" height="1080" fill="#000000" opacity={(1.0 - closed) * 0.8} />
                {caption_sequence(&frame, "2", "Klart: datorn är kopplad", &[
                    ("Guiden märker den första synken av sig själv.", 0.0),
                    ("Och sidan fylls med din användning.", SHOW_AT + 0.3),
                ])}
                <g opacity={1.0 - closed} transform={dialog} font-family={SANS}>
                    {connect::dialog_shell(SHORT, 3, "First sync", icons::REFRESH)}
                    <g opacity={connected} transform={Transform::translate(0, (1.0 - connected) * 8.0)}>
                        <rect x="24" y={CONTENT_TOP} width="528" height="98" rx="12" fill={AMBER} opacity="0.05" />
                        <rect x="24" y={CONTENT_TOP} width="528" height="98" rx="12" fill="none" stroke={AMBER} stroke-width="1" opacity="0.3" />
                        {icon(icons::PARTY, 40.0, CONTENT_TOP + 16.0, 16.0, AMBER)}
                        <text x="64" y={CONTENT_TOP + 29.0} font-size="14" font-weight="500" fill={FG}>{format!("{} is connected", data::MACHINE)}</text>
                        <text x="40" y={CONTENT_TOP + 53.0} font-size="12" font-weight="500" fill={MUTED}>"History read in full:"</text>
                        {read}
                        <text x="40" y={CONTENT_TOP + 77.0} font-size="12" font-weight="500" fill={MUTED}>"Do the same on every machine where you use a coding agent."</text>
                    </g>
                    {button(24.0, SHORT - 64.0, 170.0, 40.0, "Add another machine", ButtonStyle::Ghost, None, None)}
                    {button(DIALOG_WIDTH - 24.0 - 136.0, SHORT - 64.0, 136.0, 40.0, "Show my usage", ButtonStyle::Primary, None, None)}
                </g>
                <g transform={format!("translate({pointer_x} {pointer_y}) scale({ZOOM})")}>
                    {pointer(t, 0.0, 0.0, &[SHOW_AT], progress(t, 2.2, 0.3).min(1.0 - progress(t, SHOW_AT + 0.5, 0.4)))}
                </g>
            </g>
        )
    }
}
