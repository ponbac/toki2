//! Declaring an AI subscription under Account (`ai-subscriptions-settings.tsx`): the warning
//! about a plan the tool reported, Declare, the form, and the declared plan. Then the dialog
//! makes room for an explanatory drawing of what the plan changes: the month's
//! API-equivalent cost per project becomes each project's share of the fee.
use crate::*;

const SECONDS: f32 = 19.0;

const DECLARE_AT: f32 = 2.7;
const PLAN_AT: f32 = 3.9;
const COST_AT: f32 = 4.9;
/// The currency select opens, and US dollars are picked instead of the default.
const CURRENCY_AT: f32 = 6.0;
const DOLLARS_AT: f32 = 6.8;
const ADD_AT: f32 = 7.8;
const ADDED_AT: f32 = 8.0;
const SPLIT_AT: f32 = 8.7;
/// When the three columns of the drawing arrive: cost, share, and part of the fee.
const COSTS_AT: f32 = 9.2;
const SHARES_AT: f32 = 10.9;
const FEES_AT: f32 = 12.3;

/// The Account dialog, `max-w-md`, in its own pixels at the page's zoom.
const WIDTH: f32 = 448.0;
const HEIGHT: f32 = 566.0;
const TOP: f32 = 208.0;
/// The scrolling part of the dialog, between its header and its footer.
const VIEW_TOP: f32 = 92.0;
const VIEW_HEIGHT: f32 = 394.0;
/// How far the content scrolls to bring the whole form into view.
const SCROLL: f32 = 194.0;
/// Where the form starts, and the list of declared plans, in content pixels.
const FORM_Y: f32 = 370.0;
const LIST_Y: f32 = 190.0;
const TYPING: f32 = 18.0;
/// Where the list of currencies opens, under its select.
const CURRENCIES_Y: f32 = FORM_Y + 140.0;

/// The drawing beside the dialog, in video pixels.
const SPLIT_X: f32 = 820.0;
const ROWS_Y: f32 = 440.0;
const ROW: f32 = 72.0;
const BAR_X: f32 = 1140.0;

#[derive(Debug)]
pub struct Subscription;

/// `text` up to what was typed by `t`.
fn typed(text: &str, t: f32, start: f32) -> &str {
    let count = ((t - start) * TYPING).max(0.0) as usize;
    text.char_indices()
        .nth(count)
        .map_or(text, |(end, _)| &text[..end])
}

/// Where the dialog's left edge is: centered, then aside to make room for the drawing.
fn dialog_x(t: f32) -> f32 {
    lerp(
        960.0 - WIDTH * ZOOM / 2.0,
        96.0,
        ease_in_out(progress(t, ADDED_AT + 0.3, 0.8)),
    )
}

/// Where the pointer is at `t`, in dialog pixels as they are on screen after scrolling.
fn pointer_at(t: f32) -> (f32, f32) {
    const STOPS: [(f32, f32); 7] = [
        (540.0, 520.0),
        (382.0, 236.0),
        (300.0, 420.0 - SCROLL),
        (330.0, FORM_Y + 116.0 - SCROLL),
        (300.0, CURRENCIES_Y + 4.0 + 32.0 + 17.0 - SCROLL),
        (386.0, 642.0 - SCROLL),
        (520.0, 420.0),
    ];
    const MOVES: [(f32, f32); 6] = [
        (1.7, 0.8),
        (3.2, 0.6),
        (5.4, 0.5),
        (6.2, 0.4),
        (7.0, 0.6),
        (8.2, 0.8),
    ];
    let mut position = STOPS[0];
    for (i, (start, duration)) in MOVES.iter().enumerate() {
        let p = ease_in_out(progress(t, *start, *duration));
        position = (
            lerp(STOPS[i].0, STOPS[i + 1].0, p),
            lerp(STOPS[i].1, STOPS[i + 1].1, p),
        );
        if p < 1.0 {
            break;
        }
    }
    position
}

/// A labelled field of the form: `value`, or `placeholder` in muted ink while it is empty.
#[allow(clippy::too_many_arguments)]
fn field<'a>(
    x: f32,
    y: f32,
    label: &'a str,
    value: &'a str,
    placeholder: &'a str,
    focused: f32,
    select: bool,
    lead: Svgr<'a>,
) -> Svgr<'a> {
    let (text, ink) = if value.is_empty() {
        (placeholder, MUTED)
    } else {
        (value, FG)
    };
    let indent = if select && !placeholder.is_empty() {
        22.0
    } else {
        0.0
    };
    fframes::svgr!(
        <g>
            <text x={x} y={y + 11.0} font-size="12" font-weight="500" fill={FG}>{label}</text>
            <rect x={x} y={y + 18.0} width="184" height="40" rx="8" fill={BG} stroke={BORDER} stroke-width="1" />
            <rect x={x} y={y + 18.0} width="184" height="40" rx="8" fill="none" stroke={AMBER} stroke-width="1.5" opacity={focused * 0.8} />
            {lead}
            <text x={x + 13.0 + indent} y={y + 43.0} font-size="14" font-weight="500" fill={ink}>{text}</text>
            {if select { icon(icons::CHEVRON_DOWN, x + 158.0, y + 30.0, 16.0, MUTED) } else { Svgr::empty() }}
        </g>
    )
}

/// A date of the form: typed as YYYY-MM-DD, with a button that opens a calendar.
fn date_field<'a>(x: f32, y: f32, label: &'a str, value: &'a str) -> Svgr<'a> {
    let (text, ink) = if value.is_empty() {
        ("YYYY-MM-DD", MUTED)
    } else {
        (value, FG)
    };
    fframes::svgr!(
        <g>
            <text x={x} y={y + 11.0} font-size="12" font-weight="500" fill={FG}>{label}</text>
            <rect x={x} y={y + 18.0} width="138" height="40" rx="8" fill={BG} stroke={BORDER} stroke-width="1" />
            <text x={x + 13.0} y={y + 43.0} font-size="14" font-weight="500" fill={ink}>{text}</text>
            <rect x={x + 144.0} y={y + 18.0} width="40" height="40" rx="8" fill={BG} stroke={BORDER} stroke-width="1" />
            {icon(icons::CALENDAR, x + 156.0, y + 30.0, 16.0, FG)}
        </g>
    )
}

impl Scene for Subscription {
    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(SECONDS)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::from([
            AudioTrack::new("click.wav", Second(DECLARE_AT)..Eof).gain_db(-7.0),
            AudioTrack::new("typing.wav", Second(PLAN_AT + 0.1)..Eof).gain_db(-13.0),
            AudioTrack::new("chime.wav", Second(ADD_AT)..Eof).gain_db(-5.0),
            AudioTrack::new("click.wav", Second(FEES_AT)..Eof).gain_db(-9.0),
        ])
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = frame.seconds();
        let form = progress(t, DECLARE_AT, 0.25) * (1.0 - progress(t, ADDED_AT, 0.2));
        let added = progress(t, ADDED_AT, 0.3);
        let scroll = SCROLL
            * ease_in_out(progress(t, DECLARE_AT, 0.5))
            * (1.0 - ease_in_out(progress(t, ADDED_AT, 0.5)));
        let x = dialog_x(t);
        let (pointer_x, pointer_y) = pointer_at(t);

        let plan = typed(data::PLAN, t, PLAN_AT + 0.1);
        let cost = typed("200", t, COST_AT + 0.1);
        let plan_focus = progress(t, PLAN_AT, 0.1) * (1.0 - progress(t, COST_AT, 0.1));
        let cost_focus = progress(t, COST_AT, 0.1) * (1.0 - progress(t, CURRENCY_AT, 0.1));
        let currencies = progress(t, CURRENCY_AT, 0.12).min(1.0 - progress(t, DOLLARS_AT, 0.1));
        let currency = if t >= DOLLARS_AT { "USD" } else { "SEK" };
        let saving = t >= ADD_AT;

        let toast = progress(t, ADDED_AT, 0.25).min(1.0 - progress(t, ADDED_AT + 2.4, 0.3));

        // The drawing: September's Codex cost per project, its share, and that share of the fee.
        let costs = data::codex_in_september();
        let total: f32 = costs.iter().sum();
        // The fee is converted to kronor once, and split in whole öre that sum to it.
        let fee = money::usd_to_ore(data::PLAN_FEE_CENTS as f32 / 100.0);
        let fees = data::split_fee(fee, &costs);
        let rate = format!("{:.4}", money::SEK_PER_USD).replace('.', ",");
        let rows: Vec<Svgr> = (0..5)
            .map(|i| {
                let y = ROWS_Y + i as f32 * ROW;
                let name = data::PROJECTS[i].0.split(" – ").next().unwrap_or_default();
                let grow = ease_out(progress(t, COSTS_AT + i as f32 * 0.1, 0.8));
                let share = ramp(&frame, SHARES_AT + i as f32 * 0.1);
                let counted = ease_out(progress(t, FEES_AT + i as f32 * 0.1, 0.9));
                let width = (costs[i] / costs[0] * 250.0 * grow).max(0.5);
                fframes::svgr!(<g>
                    <text x={SPLIT_X + 40.0} y={y + 30.0} font-size="24" font-weight="500" fill={FG} opacity={ramp(&frame, SPLIT_AT + 0.2 + i as f32 * 0.06)}>{name}</text>
                    <rect x={BAR_X} y={y + 6.0} width={width} height="30" rx="6" fill={PROVIDER_COLORS[0]} />
                    <text x={BAR_X + width + 12.0} y={y + 29.0} font-size="20" font-weight="500" fill={MUTED} opacity={grow}>{money::shown(Currency::Sek, costs[i] * grow)}</text>
                    <text x={SPLIT_X + 800.0} y={y + 31.0} text-anchor="end" font-size="26" font-weight="700" fill={FG} opacity={share}>{format!("{:.1} %", costs[i] / total * 100.0).replace('.', ",")}</text>
                    <text x={SPLIT_X + 1012.0} y={y + 31.0} text-anchor="end" font-size="26" font-weight="700" fill={AMBER} opacity={counted.min(1.0) * progress(t, FEES_AT + i as f32 * 0.1, 0.15)}>{money::kronor((fees[i] as f32 * counted).round() as i64)}</text>
                </g>)
            })
            .collect();
        let sum_y = ROWS_Y + 5.0 * ROW + 6.0;

        fframes::svgr!(
            <g opacity={scene_fade(t, SECONDS)}>
                {caption_sequence(&frame, "4", "Ange dina abonnemang", &[
                    ("Toki märker när ett verktyg rapporterar ett abonnemang som inte är angivet.", 0.0),
                    ("Ange plan, månadskostnad och valuta under Account. Dagar utan abonnemang räknas som API-kostnad.", DECLARE_AT + 0.3),
                    ("API-kostnaden blir då en fördelningsnyckel: varje projekt får sin andel av avgiften.", SPLIT_AT),
                ])}
                <defs>
                    <clipPath id="account-view">
                        <rect x="0" y={VIEW_TOP} width={WIDTH} height={VIEW_HEIGHT} />
                    </clipPath>
                </defs>
                <g opacity={ramp(&frame, 0.2)} transform={Transform::translate(0, rise(&frame, 0.2, 24.0))}>
                    <g transform={format!("translate({x} {TOP}) scale({ZOOM})")} font-family={SANS}>
                        <rect x="0" y="8" width={WIDTH} height={HEIGHT} rx="14" fill="#000000" opacity="0.4" />
                        <rect x="0" y="0" width={WIDTH} height={HEIGHT} rx="12" fill={BG} stroke={BORDER} stroke-width="1" />
                        <text x="24" y="42" font-size="18" font-weight="600" letter-spacing="-0.4" fill={FG}>"Account"</text>
                        <text x="24" y="66" font-size="14" font-weight="500" fill={MUTED}>"Avatar, timer buddy, API tokens, and AI subscriptions."</text>
                        {icon(icons::CLOSE, 416.0, 16.0, 16.0, MUTED)}
                        <g clip-path="url(#account-view)">
                            <g transform={Transform::translate(0, -scroll)}>
                                <path d="M24 100 H424" stroke={BORDER} stroke-width="1" />
                                {icon(icons::CREDIT_CARD, 24.0, 115.0, 16.0, FG)}
                                <text x="48" y="128" font-size="14" font-weight="500" fill={FG}>"AI subscriptions"</text>
                                <g font-size="12" font-weight="500" fill={MUTED}>
                                    <text x="24" y="149">"Declare the AI plans you pay a monthly fee for. On days without one, a"</text>
                                    <text x="24" y="164">"provider's usage is billed at its estimated API cost in billing reports."</text>
                                    <text x="24" y="179">"Dates are calendar days in Europe/Stockholm."</text>
                                </g>
                                <g opacity={1.0 - added}>
                                    <rect x="24" y={LIST_Y} width="400" height="144" rx="10" fill="none" stroke={AMBER} stroke-width="1" />
                                    {icon(icons::WARNING, 36.0, LIST_Y + 12.0, 16.0, AMBER)}
                                    <text x="64" y={LIST_Y + 25.0} font-size="14" font-weight="500" fill={AMBER}>"Undeclared subscription?"</text>
                                    <g font-size="12" font-weight="500" fill={FG}>
                                        <text x="64" y={LIST_Y + 47.0}>"Codex usage indicates a subscription (plan pro)"</text>
                                        <text x="64" y={LIST_Y + 65.0}>"on 59 days from 2026-08-14 to 2026-10-11, but"</text>
                                        <text x="64" y={LIST_Y + 83.0}>"no subscription is declared for these days, so"</text>
                                        <text x="64" y={LIST_Y + 101.0}>"their usage is billed at its estimated API cost in"</text>
                                        <text x="64" y={LIST_Y + 119.0}>"billing reports, unless it is Personal / internal."</text>
                                    </g>
                                    <g opacity={if t >= DECLARE_AT { 0.5 } else { 1.0 }}>
                                        <rect x="352" y={LIST_Y + 32.0} width="60" height="28" rx="8" fill={BG} stroke={BORDER} stroke-width="1" />
                                        <text x="382" y={LIST_Y + 50.0} text-anchor="middle" font-size="12" font-weight="500" fill={FG}>"Declare"</text>
                                    </g>
                                    <text x="24" y={LIST_Y + 166.0} font-size="12" font-weight="500" fill={MUTED}>"No subscriptions declared yet."</text>
                                </g>
                                <g opacity={added}>
                                    <rect x="24" y={LIST_Y} width="400" height="56" rx="8" fill="#151922" stroke={BORDER} stroke-width="1" />
                                    <rect x="36" y={LIST_Y + 12.0} width="32" height="32" rx="8" fill={PROVIDER_COLORS[0]} opacity="0.14" />
                                    {provider_icon(0, 44.0, LIST_Y + 20.0, 16.0)}
                                    <text x="80" y={LIST_Y + 25.0} font-size="14" font-weight="500" fill={FG}>{data::PLAN}</text>
                                    <text x="164" y={LIST_Y + 25.0} font-size="14" font-weight="500" fill={MUTED}>"· Codex"</text>
                                    <text x="80" y={LIST_Y + 43.0} font-size="12" font-weight="500" fill={MUTED}>{format!("200.00 USD/month · from {} · ongoing", data::PLAN_FROM)}</text>
                                    {icon(icons::PENCIL, 362.0, LIST_Y + 20.0, 16.0, FG)}
                                    {icon(icons::TRASH, 394.0, LIST_Y + 20.0, 16.0, FG)}
                                </g>
                                <g opacity={1.0 - progress(t, DECLARE_AT, 0.15) * (1.0 - added)}>
                                    {button(24.0, lerp(FORM_Y, LIST_Y + 68.0, added), 156.0, 36.0, "Add subscription", ButtonStyle::Outline, Some(icons::PLUS), None)}
                                </g>
                                <g opacity={form}>
                                    <rect x="24" y={FORM_Y} width="400" height="302" rx="8" fill="#151922" stroke={BORDER} stroke-width="1" />
                                    {field(36.0, FORM_Y + 12.0, "Provider", "Codex", "Codex", 0.0, true, provider_icon(0, 49.0, FORM_Y + 43.0, 14.0))}
                                    {field(228.0, FORM_Y + 12.0, "Plan", plan, "ChatGPT Plus", plan_focus, false, Svgr::empty())}
                                    {field(36.0, FORM_Y + 78.0, "Monthly cost", cost, "0.00", cost_focus, false, Svgr::empty())}
                                    {field(228.0, FORM_Y + 78.0, "Currency", currency, "", currencies, true, Svgr::empty())}
                                    {date_field(36.0, FORM_Y + 144.0, "From", data::PLAN_FROM)}
                                    {date_field(228.0, FORM_Y + 144.0, "To (optional)", "")}
                                    <g font-size="12" font-weight="500" fill={MUTED}>
                                        <text x="36" y={FORM_Y + 225.0}>"Enter dates as YYYY-MM-DD. Both dates are included. Leave the"</text>
                                        <text x="36" y={FORM_Y + 240.0}>"end empty while the subscription is ongoing."</text>
                                    </g>
                                    {button(284.0, FORM_Y + 254.0, 68.0, 36.0, "Cancel", ButtonStyle::Ghost, None, None)}
                                    <g opacity={currencies}>
                                        <rect x="230" y={CURRENCIES_Y + 4.0} width="184" height="104" rx="10" fill="#000000" opacity="0.35" />
                                        <rect x="228" y={CURRENCIES_Y} width="184" height="104" rx="10" fill={POPOVER} stroke={BORDER} stroke-width="1" />
                                        <rect x="232" y={CURRENCIES_Y + 36.0} width="176" height="32" rx="6" fill={HOVER} opacity={progress(t, CURRENCY_AT + 0.4, 0.1)} />
                                        {icon(icons::CHECK, 240.0, CURRENCIES_Y + 13.0, 14.0, FG)}
                                        <text x="264" y={CURRENCIES_Y + 25.0} font-size="14" font-weight="500" fill={FG}>"SEK"</text>
                                        <text x="264" y={CURRENCIES_Y + 57.0} font-size="14" font-weight="500" fill={FG}>"USD"</text>
                                        <text x="264" y={CURRENCIES_Y + 89.0} font-size="14" font-weight="500" fill={FG}>"EUR"</text>
                                    </g>
                                    <g opacity={if saving { 0.5 } else { 1.0 }}>
                                        {button(if saving { 334.0 } else { 360.0 }, FORM_Y + 254.0, if saving { 78.0 } else { 52.0 }, 36.0, if saving { "Saving..." } else { "Add" }, ButtonStyle::Primary, None, None)}
                                    </g>
                                </g>
                            </g>
                        </g>
                        <path d={format!("M0 {} H{WIDTH}", HEIGHT - 80.0)} stroke={BORDER} stroke-width="1" opacity="0.6" />
                        {button(170.0, HEIGHT - 64.0, 124.0, 40.0, "Remove avatar", ButtonStyle::Outline, None, None)}
                        <g opacity="0.5">
                            {button(302.0, HEIGHT - 64.0, 122.0, 40.0, "Save avatar", ButtonStyle::Primary, None, None)}
                        </g>
                    </g>
                </g>
                <g opacity={ramp(&frame, SPLIT_AT)} transform={Transform::translate(rise(&frame, SPLIT_AT, 40.0), 0)} font-family={SANS}>
                    <rect x={SPLIT_X} y="236" width="1052" height="704" rx="24" fill={CARD} stroke={BORDER} stroke-width="1.5" />
                    <rect x={SPLIT_X + 40.0} y="270" width="60" height="60" rx="14" fill={PROVIDER_COLORS[0]} opacity="0.14" />
                    {provider_icon(0, SPLIT_X + 55.0, 285.0, 30.0)}
                    <text x={SPLIT_X + 118.0} y="298" font-size="34" font-weight="600" letter-spacing="-0.6" fill={FG}>{data::PLAN}</text>
                    <text x={SPLIT_X + 118.0} y="327" font-family={MONO} font-size="20" font-weight="500" fill={MUTED}>"Codex · september 2026"</text>
                    <text x={SPLIT_X + 1012.0} y="304" text-anchor="end" font-size="44" font-weight="800" letter-spacing="-1" fill={FG}>{money::kronor(fee)}</text>
                    <text x={SPLIT_X + 1012.0} y="332" text-anchor="end" font-size="20" font-weight="500" fill={MUTED}>{format!("{} USD per månad, à {rate} kr", data::PLAN_FEE_CENTS / 100)}</text>
                    <g font-size="20" font-weight="500" fill={MUTED}>
                        <text x={SPLIT_X + 40.0} y="404">"Projekt"</text>
                        <text x={BAR_X} y="404" opacity={ramp(&frame, COSTS_AT)}>"API-kostnad"</text>
                        <text x={SPLIT_X + 800.0} y="404" text-anchor="end" opacity={ramp(&frame, SHARES_AT)}>"Andel"</text>
                        <text x={SPLIT_X + 1012.0} y="404" text-anchor="end" opacity={ramp(&frame, FEES_AT)}>"Av avgiften"</text>
                    </g>
                    <path d={format!("M{} 420 H{}", SPLIT_X + 40.0, SPLIT_X + 1012.0)} stroke={BORDER} stroke-width="1.5" />
                    {rows}
                    <g opacity={ramp(&frame, FEES_AT + 1.2)}>
                        <path d={format!("M{} {sum_y} H{}", SPLIT_X + 40.0, SPLIT_X + 1012.0)} stroke={BORDER} stroke-width="1.5" />
                        <text x={SPLIT_X + 40.0} y={sum_y + 40.0} font-size="24" font-weight="700" fill={FG}>"Summa"</text>
                        <text x={BAR_X} y={sum_y + 39.0} font-size="20" font-weight="500" fill={MUTED}>{money::shown(Currency::Sek, total)}</text>
                        <text x={SPLIT_X + 800.0} y={sum_y + 41.0} text-anchor="end" font-size="26" font-weight="700" fill={FG}>"100 %"</text>
                        <text x={SPLIT_X + 1012.0} y={sum_y + 41.0} text-anchor="end" font-size="26" font-weight="700" fill={AMBER}>{money::kronor(fees.iter().sum())}</text>
                    </g>
                    <text x={SPLIT_X + 40.0} y="912" font-size="22" font-weight="500" fill={MUTED} opacity={ramp(&frame, FEES_AT + 2.2)}>"Egna repon, märkta Personal / internal, tar ingen andel av avgiften."</text>
                </g>
                <g opacity={toast} transform={Transform::translate(0, (1.0 - toast) * 16.0)} font-family={SANS}>
                    <rect x="1456" y="964" width="416" height="68" rx="14" fill={CARD} stroke={BORDER} stroke-width="1.5" />
                    <circle cx="1496" cy="998" r="13" fill={OK} />
                    {icon(icons::CHECK, 1487.0, 989.0, 18.0, CARD)}
                    <text x="1524" y="1006" font-size="22" font-weight="600" fill={FG}>"Subscription added"</text>
                </g>
                <g transform={format!("translate({} {}) scale({ZOOM})", x + pointer_x * ZOOM, TOP + pointer_y * ZOOM)}>
                    {pointer(t, 0.0, 0.0, &[DECLARE_AT, PLAN_AT, CURRENCY_AT, DOLLARS_AT, ADD_AT], progress(t, 1.4, 0.3).min(1.0 - progress(t, ADD_AT + 0.9, 0.4)))}
                </g>
            </g>
        )
    }
}
