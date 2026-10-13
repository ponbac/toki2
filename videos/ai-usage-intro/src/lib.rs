//! The Toki "AI usage" intro video, on mocked data: connect a machine with the onboarding
//! dialog, paste its commands, let the daily job sync, declare a subscription, read the page
//! tab by tab, map repositories to projects.
//!
//! The UI is drawn in the app's own CSS pixels (the page is 1216 px wide inside `max-w-7xl`)
//! and scaled 1.5x, so 1 UI px is 1.5 video px and the page fills a 1920 px frame. The page
//! is taller than the frame, so `page::view` shows a part of it at a time.
use fframes::{
    AnimateRuntimeInput, AudioMap,
    AudioTimestamp::*,
    AudioTrack, Color, Duration, FFramesContext, Frame, Scene, Scenes, Svgr, Transform, Video,
    animation::{AnimationRuntime, Easing},
    include_media_dir,
};
use money::Currency;
use std::sync::LazyLock;

mod connect;
mod daily;
mod data;
mod intro;
mod money;
mod outro;
mod overview;
mod page;
mod purpose;
mod repositories;
mod schedule;
mod subscription;
mod synced;
mod terminal;
mod whereabouts;

include_media_dir!(pub struct TokiAiUsageMedia, "media");

pub const WIDTH: usize = 1920;
pub const HEIGHT: usize = 1080;

// The app's dark theme (`app/src/index.css`), resolved from its HSL tokens.
pub(crate) const BG: &str = "#0f121a";
pub(crate) const CARD: &str = "#161a22";
pub(crate) const POPOVER: &str = "#1a1f29";
pub(crate) const FG: &str = "#efece7";
pub(crate) const MUTED: &str = "#989081";
pub(crate) const BORDER: &str = "#272b35";
pub(crate) const HOVER: &str = "#222733";
/// `--muted`, for skeletons and unfilled progress.
pub(crate) const MUTED_SURFACE: &str = "#1d212a";
pub(crate) const AMBER: &str = "#f9a91f";
pub(crate) const GRID: &str = "#262b36";
pub(crate) const OTHER: &str = "#5b606b";
pub(crate) const OK: &str = "#199e70";
/// The green of a fresh sync and of history read in full.
pub(crate) const FRESH: &str = "#0ca30c";
/// `--viz-1..5`: Codex, Claude Code, Copilot, Pi.
pub(crate) const PROVIDER_COLORS: [&str; 4] = ["#3987e5", "#d95926", "#c98500", "#d55181"];
pub(crate) const PROVIDER_NAMES: [&str; 4] = ["Codex", "Claude Code", "Copilot", "Pi"];
/// `--viz-seq-1..7` in the dark theme, from little to much.
pub(crate) const SEQUENTIAL: [&str; 7] = [
    "#0d366b", "#184f95", "#256abf", "#3987e5", "#6da7ec", "#9ec5f4", "#cde2fb",
];

pub(crate) const SANS: &str = "Manrope";
pub(crate) const MONO: &str = "IBM Plex Mono";

/// Where the 1216 px wide page sits in the frame, and its zoom.
pub(crate) const PAGE_X: f32 = 48.0;
pub(crate) const PAGE_Y: f32 = 226.0;
pub(crate) const ZOOM: f32 = 1.5;

static EASE: LazyLock<AnimationRuntime> =
    LazyLock::new(|| AnimationRuntime::new(0.8, &Easing::CubicBezier(0.16, 1.0, 0.3, 1.0)));
static SPRING: LazyLock<AnimationRuntime> = LazyLock::new(|| {
    AnimationRuntime::new(
        3.0,
        &Easing::Spring {
            mass: 1.0,
            stiffness: 170.0,
            damping: 22.0,
        },
    )
});

pub struct TokiAiUsageVideo<'a> {
    pub media: &'a TokiAiUsageMedia,
    intro: intro::Intro,
    purpose: purpose::Purpose,
    connect: connect::Connect,
    terminal: terminal::Terminal,
    synced: synced::Synced,
    schedule: schedule::Schedule,
    overview: overview::Overview,
    daily: daily::Daily,
    whereabouts: whereabouts::Whereabouts,
    repositories: repositories::Repositories,
    subscription: subscription::Subscription,
    outro: outro::Outro,
}

impl<'a> TokiAiUsageVideo<'a> {
    pub fn new(media: &'a TokiAiUsageMedia) -> Self {
        Self {
            media,
            intro: intro::Intro,
            purpose: purpose::Purpose,
            connect: connect::Connect,
            terminal: terminal::Terminal,
            synced: synced::Synced,
            schedule: schedule::Schedule,
            overview: overview::Overview,
            daily: daily::Daily,
            whereabouts: whereabouts::Whereabouts::new(),
            repositories: repositories::Repositories,
            subscription: subscription::Subscription,
            outro: outro::Outro,
        }
    }
}

impl std::fmt::Debug for TokiAiUsageVideo<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokiAiUsageVideo").finish()
    }
}

impl Video for TokiAiUsageVideo<'_> {
    const FPS: usize = 30;
    const WIDTH: usize = WIDTH;
    const HEIGHT: usize = HEIGHT;
    const BACKGROUND_COLOR: Color = Color::BLACK;

    fn duration(&self) -> Duration<'_> {
        Duration::Auto
    }

    fn audio(&self) -> AudioMap<'_> {
        // The pad runs under everything; each scene adds its own effects in `Scene::audio`.
        AudioMap::from([AudioTrack::new("bed.wav", Second(0.0)..Eof).gain_db(5.0)])
    }

    fn define_scenes(&self) -> Scenes<'_> {
        Scenes::from(vec![
            &self.intro as &dyn Scene,
            &self.purpose,
            &self.connect,
            &self.terminal,
            &self.synced,
            &self.schedule,
            &self.subscription,
            &self.overview,
            &self.daily,
            &self.whereabouts,
            &self.repositories,
            &self.outro,
        ])
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        fframes::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width={WIDTH} height={HEIGHT} viewBox="0 0 1920 1080">
                <defs>
                    <radialGradient id="glow" cx="0.5" cy="0.5" r="0.5">
                        <stop offset="0" stop-color={AMBER} stop-opacity="0.10" />
                        <stop offset="1" stop-color={AMBER} stop-opacity="0" />
                    </radialGradient>
                </defs>
                <rect width="1920" height="1080" fill={BG} />
                <ellipse cx="960" cy="-120" rx="1300" ry="560" fill="url(#glow)" />
                {ctx.render_scenes(&frame)}
            </svg>
        )
    }
}

// --- timing ---------------------------------------------------------------------------------

/// 0 to 1 between `start` and `start + duration` seconds, clamped.
pub(crate) fn progress(t: f32, start: f32, duration: f32) -> f32 {
    ((t - start) / duration).clamp(0.0, 1.0)
}

pub(crate) fn ease_out(p: f32) -> f32 {
    1.0 - (1.0 - p).powi(3)
}

pub(crate) fn ease_in_out(p: f32) -> f32 {
    if p < 0.5 {
        4.0 * p * p * p
    } else {
        1.0 - (-2.0 * p + 2.0).powi(3) / 2.0
    }
}

pub(crate) fn lerp(from: f32, to: f32, p: f32) -> f32 {
    from + (to - from) * p
}

/// 0 to 1 over 0.8 s from `start`, with a long settle.
pub(crate) fn ramp(frame: &Frame, start: f32) -> f32 {
    frame.animate_runtime(AnimateRuntimeInput {
        on_second: start,
        from: 0.0,
        to: 1.0,
        animation_runtime: &EASE,
    })
}

/// A vertical offset that springs from `distance` px to 0 at `start`.
pub(crate) fn rise(frame: &Frame, start: f32, distance: f32) -> f32 {
    frame.animate_runtime(AnimateRuntimeInput {
        on_second: start,
        from: distance,
        to: 0.0,
        animation_runtime: &SPRING,
    })
}

/// Opacity of a whole scene: in over 0.4 s, out over the last 0.35 s, so scenes dip through
/// the background instead of crossfading two layouts.
pub(crate) fn scene_fade(t: f32, duration: f32) -> f32 {
    progress(t, 0.0, 0.4).min(1.0 - progress(t, duration - 0.35, 0.35))
}

// --- formatting -----------------------------------------------------------------------------

/// `13842.6` as `13,842.60`.
pub(crate) fn decimals(value: f32) -> String {
    let cents = (value as f64 * 100.0).round() as i64;
    format!("{}.{:02}", thousands(cents / 100), cents % 100)
}

pub(crate) fn usd(value: f32) -> String {
    format!("${}", decimals(value))
}

pub(crate) fn thousands(value: i64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// A count as the page shortens it: `24.8B`, `946.7M`, `5M`.
pub(crate) fn compact(count: f32) -> String {
    let (value, unit) = match count {
        c if c >= 1e9 => (c / 1e9, "B"),
        c if c >= 1e6 => (c / 1e6, "M"),
        c if c >= 1e3 => (c / 1e3, "K"),
        c => (c, ""),
    };
    let text = format!("{value:.1}");
    format!("{}{unit}", text.strip_suffix(".0").unwrap_or(&text))
}

/// A share as the page writes it: `38.6%`, `10%`.
pub(crate) fn percent(share: f32) -> String {
    let text = format!("{:.1}", share * 100.0);
    format!("{}%", text.strip_suffix(".0").unwrap_or(&text))
}

// --- shared pieces --------------------------------------------------------------------------

/// The Swedish caption above the UI: a step chip, a headline and one line of explanation.
pub(crate) fn caption<'a>(frame: &Frame, step: &'a str, title: &'a str, sub: &'a str) -> Svgr<'a> {
    caption_sequence(frame, step, title, &[(sub, 0.0)])
}

/// A caption whose explanation changes: each of `subs` shows from its second until the next
/// one's, dipping through nothing in between.
pub(crate) fn caption_sequence<'a>(
    frame: &Frame,
    step: &'a str,
    title: &'a str,
    subs: &[(&'a str, f32)],
) -> Svgr<'a> {
    let t = frame.seconds();
    let lines: Vec<Svgr> = subs
        .iter()
        .enumerate()
        .map(|(i, (sub, start))| {
            let enter = if i == 0 { 1.0 } else { progress(t, *start, 0.3) };
            let leave = subs.get(i + 1).map_or(0.0, |(_, next)| progress(t, next - 0.3, 0.3));
            fframes::svgr!(<text x="50" y="176" font-size="28" font-weight="500" fill={MUTED} opacity={enter * (1.0 - leave)}>{*sub}</text>)
        })
        .collect();
    fframes::svgr!(
        <g font-family={SANS} opacity={ramp(frame, 0.15)} transform={Transform::translate(0, rise(frame, 0.15, 20.0))}>
            <circle cx="78" cy="92" r="30" fill={AMBER} />
            <text x="78" y="104" text-anchor="middle" font-size="32" font-weight="800" fill={BG}>{step}</text>
            <text x="128" y="110" font-size="52" font-weight="700" letter-spacing="-1" fill={FG}>{title}</text>
            {lines}
        </g>
    )
}

/// The page group: UI pixels in, video pixels out.
pub(crate) fn page_transform() -> String {
    format!("translate({PAGE_X} {PAGE_Y}) scale({ZOOM})")
}

/// A card of the usage page (`ChartCard`): surface, hairline border, title and description.
pub(crate) fn card<'a>(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    title: &'a str,
    description: &str,
) -> Svgr<'a> {
    fframes::svgr!(
        <g font-family={SANS}>
            <rect x={x} y={y} width={width} height={height} rx="16" fill={CARD} stroke={BORDER} stroke-width="1" />
            <text x={x + 20.0} y={y + 34.0} font-size="14" font-weight="600" fill={FG}>{title}</text>
            <text x={x + 20.0} y={y + 55.0} font-size="12" font-weight="500" fill={MUTED}>{description.to_owned()}</text>
        </g>
    )
}

/// A segmented control of the app (`Segmented` in `chart-card.tsx`), right-aligned at
/// `right`: each option with its width. `selected` is the chosen option, and may be between
/// two options while the choice slides from one to the other.
pub(crate) fn segments<'a>(
    right: f32,
    y: f32,
    options: &[(&'a str, f32)],
    selected: f32,
) -> Svgr<'a> {
    let total: f32 = options.iter().map(|(_, width)| width).sum();
    let x = right - total - 4.0;
    let starts: Vec<f32> = options
        .iter()
        .scan(x + 2.0, |at, (_, width)| {
            let start = *at;
            *at += width;
            Some(start)
        })
        .collect();
    let last = options.len() - 1;
    let (from, to) = (
        (selected.floor() as usize).min(last),
        (selected.ceil() as usize).min(last),
    );
    let slide = selected.fract();
    let labels: Vec<Svgr> = options
        .iter()
        .enumerate()
        .map(|(i, (label, width))| {
            let ink = if selected.round() as usize == i { FG } else { MUTED };
            fframes::svgr!(<text x={starts[i] + width / 2.0} y={y + 18.5} text-anchor="middle" fill={ink}>{*label}</text>)
        })
        .collect();
    fframes::svgr!(
        <g font-family={SANS} font-size="12" font-weight="500">
            <rect x={x} y={y} width={total + 4.0} height="28" rx="8" fill="#1b1f28" stroke={BORDER} stroke-width="1" />
            <rect x={lerp(starts[from], starts[to], slide)} y={y + 2.0} width={lerp(options[from].1, options[to].1, slide)} height="24" rx="6" fill={BG} />
            {labels}
        </g>
    )
}

/// A provider's mark from the app (`ai-provider-icon.tsx`), `size` px wide at `x`, `y`.
pub(crate) fn provider_icon<'a>(provider: usize, x: f32, y: f32, size: f32) -> Svgr<'a> {
    let transform = format!("translate({x} {y}) scale({})", size / 24.0);
    match provider {
        3 => fframes::svgr!(
            <g transform={transform}>
                <path d="M2 2h15v10h-5V7H2z" fill="#e48a7a" />
                <path d="M2 7h5v5h5v5H7v5H2z" fill="#4f8eb3" />
                <path d="M17 12h5v10h-5z" fill="#eab65d" />
            </g>
        ),
        _ => fframes::svgr!(
            <path transform={transform} d={data::PROVIDER_PATHS[provider]} fill={PROVIDER_COLORS[provider]} fill-rule="evenodd" clip-rule="evenodd" />
        ),
    }
}

/// Lucide icons of the app, as the `d` of one stroked 24x24 path each.
pub(crate) mod icons {
    pub const LAPTOP: &str = "M20 16V7a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v9m16 0H4m16 0 1.28 2.55a1 1 0 0 1-.9 1.45H3.62a1 1 0 0 1-.9-1.45L4 16";
    pub const PLUG: &str = "M12 22v-5M9 8V2M15 8V2M18 8v5a4 4 0 0 1-4 4h-4a4 4 0 0 1-4-4V8Z";
    pub const SHIELD_CHECK: &str = "M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1zM9 12l2 2 4-4";
    pub const KEY: &str = "M2.586 17.414A2 2 0 0 0 2 18.828V21a1 1 0 0 0 1 1h3a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h1a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h.172a2 2 0 0 0 1.414-.586l.814-.814a6.5 6.5 0 1 0-4-4zM16.4 7.5h.2";
    pub const TERMINAL: &str = "M4 17l6-6-6-6M12 19h8";
    pub const ARROW_RIGHT: &str = "M5 12h14M12 5l7 7-7 7";
    pub const ARROW_LEFT: &str = "M12 19l-7-7 7-7M19 12H5";
    pub const CHECK: &str = "M20 6 9 17l-5-5";
    pub const COPY: &str = "M10 8h10a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H10a2 2 0 0 1-2-2V10a2 2 0 0 1 2-2zM4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2";
    pub const PLUS: &str = "M5 12h14M12 5v14";
    pub const CLOSE: &str = "M18 6 6 18M6 6l12 12";
    pub const PARTY: &str = "M5.8 11.3 2 22l10.7-3.79M4 3h.01M22 8h.01M15 2h.01M22 20h.01M22 2l-2.24.75a2.9 2.9 0 0 0-1.96 3.12c.1.86-.57 1.63-1.45 1.63h-.38c-.86 0-1.6.6-1.76 1.44L14 10M22 13l-.82-.33c-.86-.34-1.82.2-1.98 1.11c-.11.7-.72 1.22-1.43 1.22H17M11 2l.33.82c.34.86-.2 1.82-1.11 1.98C9.52 4.9 9 5.52 9 6.23V7M11 13c1.93 1.93 2.83 4.17 2 5-.83.83-3.07-.07-5-2-1.93-1.93-2.83-4.17-2-5 .83-.83 3.07.07 5 2Z";
    pub const REFRESH: &str = "M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8M21 3v5h-5M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16M8 16H3v5";
    pub const CREDIT_CARD: &str =
        "M4 5h16a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2zM2 10h20";
    pub const WARNING: &str = "M21.73 18l-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3M12 9v4M12 17h.01";
    pub const CHEVRON_DOWN: &str = "M6 9l6 6 6-6";
    pub const PENCIL: &str = "M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497zM15 5l4 4";
    pub const TRASH: &str = "M3 6h18M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2M10 11v6M14 11v6";
    pub const POWER: &str = "M12 2v10M18.4 6.6a9 9 0 1 1-12.77.04";
    pub const CHART_COLUMN: &str = "M3 3v16a2 2 0 0 0 2 2h16M18 17V9M13 17V5M8 17v-3";
    pub const LIST: &str = "M3 12h.01M3 18h.01M3 6h.01M8 12h13M8 18h13M8 6h13";
    pub const FOLDER_GIT: &str = "M9 20H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H20a2 2 0 0 1 2 2v5M11 12a2 2 0 1 0 4 0a2 2 0 1 0-4 0M18 19c-2.8 0-5-2.2-5-5v8M18 19a2 2 0 1 0 4 0a2 2 0 1 0-4 0";
    pub const CALENDAR: &str = "M8 2v4M16 2v4M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2zM3 10h18";
    pub const GLOBE: &str = "M2 12a10 10 0 1 0 20 0a10 10 0 1 0-20 0M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20M2 12h20";
    pub const COINS: &str = "M2 8a6 6 0 1 0 12 0a6 6 0 1 0-12 0M18.09 10.37A6 6 0 1 1 10.34 18M7 6h1v4M16.71 13.88l.7.71-2.82 2.82";
    pub const LAYERS: &str = "M12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83zM2 12a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 12M2 17a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 17";
    pub const ACTIVITY: &str = "M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2";
    pub const CIRCLE_DASHED: &str = "M10.1 2.182a10 10 0 0 1 3.8 0M13.9 21.818a10 10 0 0 1-3.8 0M17.609 3.721a10 10 0 0 1 2.69 2.7M2.182 13.9a10 10 0 0 1 0-3.8M20.279 17.609a10 10 0 0 1-2.7 2.69M21.818 10.1a10 10 0 0 1 0 3.8M3.721 6.391a10 10 0 0 1 2.7-2.69M6.391 20.279a10 10 0 0 1-2.69-2.7";
    pub const CHEVRON_RIGHT: &str = "M9 18l6-6-6-6";
    pub const BRIEFCASE: &str = "M16 20V4a2 2 0 0 0-2-2h-4a2 2 0 0 0-2 2v16M4 6h16a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2z";
    pub const INFO: &str = "M2 12a10 10 0 1 0 20 0a10 10 0 1 0-20 0M12 16v-4M12 8h.01";
    pub const CHEVRONS_UP_DOWN: &str = "M7 15l5 5 5-5M7 9l5-5 5 5";
    pub const SEARCH: &str = "M3 11a8 8 0 1 0 16 0a8 8 0 1 0-16 0M21 21l-4.3-4.3";
}

/// A Lucide icon, `size` px wide with its corner at `x`, `y`.
pub(crate) fn icon<'a>(d: &'static str, x: f32, y: f32, size: f32, color: &'a str) -> Svgr<'a> {
    fframes::svgr!(
        <path transform={format!("translate({x} {y}) scale({})", size / 24.0)} d={d} fill="none" stroke={color} stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
    )
}

/// How a button of the app is drawn.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ButtonStyle {
    Primary,
    Outline,
    Ghost,
}

/// A button of the app, `width` by `height`, with an optional icon before or after its label.
#[allow(clippy::too_many_arguments)]
pub(crate) fn button<'a>(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    label: &'a str,
    style: ButtonStyle,
    lead: Option<&'static str>,
    trail: Option<&'static str>,
) -> Svgr<'a> {
    let ink = if style == ButtonStyle::Primary {
        BG
    } else {
        FG
    };
    let surface = match style {
        ButtonStyle::Primary => {
            fframes::svgr!(<rect x={x} y={y} width={width} height={height} rx="10" fill={AMBER} />)
        }
        ButtonStyle::Outline => {
            fframes::svgr!(<rect x={x} y={y} width={width} height={height} rx="10" fill={BG} stroke={BORDER} stroke-width="1" />)
        }
        ButtonStyle::Ghost => Svgr::empty(),
    };
    // The label sits beside its icon; both are centered together.
    let shift = match (lead.is_some(), trail.is_some()) {
        (true, false) => 12.0,
        (false, true) => -12.0,
        _ => 0.0,
    };
    let center = x + width / 2.0 + shift;
    let label_half = label.chars().count() as f32 * 3.7;
    fframes::svgr!(
        <g font-family={SANS}>
            {surface}
            {lead.map(|d| icon(d, center - label_half - 24.0, y + height / 2.0 - 8.0, 16.0, ink)).unwrap_or_default()}
            <text x={center} y={y + height / 2.0 + 5.0} text-anchor="middle" font-size="14" font-weight="500" fill={ink}>{label}</text>
            {trail.map(|d| icon(d, center + label_half + 8.0, y + height / 2.0 - 8.0, 16.0, ink)).unwrap_or_default()}
        </g>
    )
}

/// A filled 24x24 mark, such as a tool's or a system's, `size` px wide at `x`, `y`.
pub(crate) fn mark<'a>(d: &'static str, x: f32, y: f32, size: f32, color: &'a str) -> Svgr<'a> {
    fframes::svgr!(
        <path transform={format!("translate({x} {y}) scale({})", size / 24.0)} d={d} fill={color} fill-rule="evenodd" clip-rule="evenodd" />
    )
}

/// A mouse pointer with its tip at `x`, `y`, and a ring that expands from each click.
pub(crate) fn pointer<'a>(t: f32, x: f32, y: f32, clicks: &[f32], opacity: f32) -> Svgr<'a> {
    let rings: Vec<Svgr> = clicks
        .iter()
        .filter(|click| t >= **click && t < **click + 0.45)
        .map(|click| {
            let p = ease_out(progress(t, *click, 0.45));
            fframes::svgr!(<circle cx={x} cy={y} r={4.0 + 14.0 * p} fill="none" stroke={AMBER} stroke-width="1.5" opacity={1.0 - p} />)
        })
        .collect();
    // The pointer dips a little on a click.
    let press = clicks
        .iter()
        .map(|click| 1.0 - (progress(t, *click - 0.06, 0.12) * std::f32::consts::PI).sin() * 0.14)
        .fold(1.0_f32, f32::min);
    fframes::svgr!(
        <g opacity={opacity}>
            {rings}
            <path transform={format!("translate({x} {y}) scale({press})")}
                d="M0 0 L0 16.5 L4.4 12.6 L7.3 19.2 L9.9 18.1 L7 11.6 L12.6 11.6 Z"
                fill="#ffffff" stroke="#0b0d12" stroke-width="1.2" stroke-linejoin="round" />
        </g>
    )
}
