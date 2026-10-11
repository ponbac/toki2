//! Mocked usage: invented customers and repositories, with totals close to a real heavy user
//! of the page but spread evenly over days, tools and projects.

/// The provider marks of the app, one 24x24 path each: Codex, Claude Code, Copilot.
/// From `@lobehub/icons-static-svg` (MIT). Pi's mark is drawn from three parts.
pub const PROVIDER_PATHS: [&str; 3] = [
    "M8.086.457a6.105 6.105 0 013.046-.415c1.333.153 2.521.72 3.564 1.7a.117.117 0 00.107.029c1.408-.346 2.762-.224 4.061.366l.063.03.154.076c1.357.703 2.33 1.77 2.918 3.198.278.679.418 1.388.421 2.126a5.655 5.655 0 01-.18 1.631.167.167 0 00.04.155 5.982 5.982 0 011.578 2.891c.385 1.901-.01 3.615-1.183 5.14l-.182.22a6.063 6.063 0 01-2.934 1.851.162.162 0 00-.108.102c-.255.736-.511 1.364-.987 1.992-1.199 1.582-2.962 2.462-4.948 2.451-1.583-.008-2.986-.587-4.21-1.736a.145.145 0 00-.14-.032c-.518.167-1.04.191-1.604.185a5.924 5.924 0 01-2.595-.622 6.058 6.058 0 01-2.146-1.781c-.203-.269-.404-.522-.551-.821a7.74 7.74 0 01-.495-1.283 6.11 6.11 0 01-.017-3.064.166.166 0 00.008-.074.115.115 0 00-.037-.064 5.958 5.958 0 01-1.38-2.202 5.196 5.196 0 01-.333-1.589 6.915 6.915 0 01.188-2.132c.45-1.484 1.309-2.648 2.577-3.493.282-.188.55-.334.802-.438.286-.12.573-.22.861-.304a.129.129 0 00.087-.087A6.016 6.016 0 015.635 2.31C6.315 1.464 7.132.846 8.086.457zm-.804 7.85a.848.848 0 00-1.473.842l1.694 2.965-1.688 2.848a.849.849 0 001.46.864l1.94-3.272a.849.849 0 00.007-.854l-1.94-3.393zm5.446 6.24a.849.849 0 000 1.695h4.848a.849.849 0 000-1.696h-4.848z",
    "M4.709 15.955l4.72-2.647.08-.23-.08-.128H9.2l-.79-.048-2.698-.073-2.339-.097-2.266-.122-.571-.121L0 11.784l.055-.352.48-.321.686.06 1.52.103 2.278.158 1.652.097 2.449.255h.389l.055-.157-.134-.098-.103-.097-2.358-1.596-2.552-1.688-1.336-.972-.724-.491-.364-.462-.158-1.008.656-.722.881.06.225.061.893.686 1.908 1.476 2.491 1.833.365.304.145-.103.019-.073-.164-.274-1.355-2.446-1.446-2.49-.644-1.032-.17-.619a2.97 2.97 0 01-.104-.729L6.283.134 6.696 0l.996.134.42.364.62 1.414 1.002 2.229 1.555 3.03.456.898.243.832.091.255h.158V9.01l.128-1.706.237-2.095.23-2.695.08-.76.376-.91.747-.492.584.28.48.685-.067.444-.286 1.851-.559 2.903-.364 1.942h.212l.243-.242.985-1.306 1.652-2.064.73-.82.85-.904.547-.431h1.033l.76 1.129-.34 1.166-1.064 1.347-.881 1.142-1.264 1.7-.79 1.36.073.11.188-.02 2.856-.606 1.543-.28 1.841-.315.833.388.091.395-.328.807-1.969.486-2.309.462-3.439.813-.042.03.049.061 1.549.146.662.036h1.622l3.02.225.79.522.474.638-.079.485-1.215.62-1.64-.389-3.829-.91-1.312-.329h-.182v.11l1.093 1.068 2.006 1.81 2.509 2.33.127.578-.322.455-.34-.049-2.205-1.657-.851-.747-1.926-1.62h-.128v.17l.444.649 2.345 3.521.122 1.08-.17.353-.608.213-.668-.122-1.374-1.925-1.415-2.167-1.143-1.943-.14.08-.674 7.254-.316.37-.729.28-.607-.461-.322-.747.322-1.476.389-1.924.315-1.53.286-1.9.17-.632-.012-.042-.14.018-1.434 1.967-2.18 2.945-1.726 1.845-.414.164-.717-.37.067-.662.401-.589 2.388-3.036 1.44-1.882.93-1.086-.006-.158h-.055L4.132 18.56l-1.13.146-.487-.456.061-.746.231-.243 1.908-1.312-.006.006z",
    "M19.245 5.364c1.322 1.36 1.877 3.216 2.11 5.817.622 0 1.2.135 1.592.654l.73.964c.21.278.323.61.323.955v2.62c0 .339-.173.669-.453.868C20.239 19.602 16.157 21.5 12 21.5c-4.6 0-9.205-2.583-11.547-4.258-.28-.2-.452-.53-.453-.868v-2.62c0-.345.113-.679.321-.956l.73-.963c.392-.517.974-.654 1.593-.654l.029-.297c.25-2.446.81-4.213 2.082-5.52 2.461-2.54 5.71-2.851 7.146-2.864h.198c1.436.013 4.685.323 7.146 2.864zm-7.244 4.328c-.284 0-.613.016-.962.05-.123.447-.305.85-.57 1.108-1.05 1.023-2.316 1.18-2.994 1.18-.638 0-1.306-.13-1.851-.464-.516.165-1.012.403-1.044.996a65.882 65.882 0 00-.063 2.884l-.002.48c-.002.563-.005 1.126-.013 1.69.002.326.204.63.51.765 2.482 1.102 4.83 1.657 6.99 1.657 2.156 0 4.504-.555 6.985-1.657a.854.854 0 00.51-.766c.03-1.682.006-3.372-.076-5.053-.031-.596-.528-.83-1.046-.996-.546.333-1.212.464-1.85.464-.677 0-1.942-.157-2.993-1.18-.266-.258-.447-.661-.57-1.108-.32-.032-.64-.049-.96-.05zm-2.525 4.013c.539 0 .976.426.976.95v1.753c0 .525-.437.95-.976.95a.964.964 0 01-.976-.95v-1.752c0-.525.437-.951.976-.951zm5 0c.539 0 .976.426.976.95v1.753c0 .525-.437.95-.976.95a.964.964 0 01-.976-.95v-1.752c0-.525.437-.951.976-.951zM7.635 5.087c-1.05.102-1.935.438-2.385.906-.975 1.037-.765 3.668-.21 4.224.405.394 1.17.657 1.995.657h.09c.649-.013 1.785-.176 2.73-1.11.435-.41.705-1.433.675-2.47-.03-.834-.27-1.52-.63-1.813-.39-.336-1.275-.482-2.265-.394zm6.465.394c-.36.292-.6.98-.63 1.813-.03 1.037.24 2.06.675 2.47.968.957 2.136 1.104 2.776 1.11h.044c.825 0 1.59-.263 1.995-.657.555-.556.765-3.187-.21-4.224-.45-.468-1.335-.804-2.385-.906-.99-.088-1.875.058-2.265.394zM12 7.615c-.24 0-.525.015-.84.044.03.16.045.336.06.526l-.001.159a2.94 2.94 0 01-.014.25c.225-.022.425-.027.612-.028h.366c.187 0 .387.006.612.028-.015-.146-.015-.277-.015-.409.015-.19.03-.365.06-.526a9.29 9.29 0 00-.84-.044z",
];

/// Grok's mark, which only the onboarding card lists.
pub const GROK_PATH: &str = "M9.27 15.29l7.978-5.897c.391-.29.95-.177 1.137.272.98 2.369.542 5.215-1.41 7.169-1.951 1.954-4.667 2.382-7.149 1.406l-2.711 1.257c3.889 2.661 8.611 2.003 11.562-.953 2.341-2.344 3.066-5.539 2.388-8.42l.006.007c-.983-4.232.242-5.924 2.75-9.383.06-.082.12-.164.179-.248l-3.301 3.305v-.01L9.267 15.292M7.623 16.723c-2.792-2.67-2.31-6.801.071-9.184 1.761-1.763 4.647-2.483 7.166-1.425l2.705-1.25a7.808 7.808 0 00-1.829-1A8.975 8.975 0 005.984 5.83c-2.533 2.536-3.33 6.436-1.962 9.764 1.022 2.487-.653 4.246-2.34 6.022-.599.63-1.199 1.259-1.682 1.925l7.62-6.815";

/// The system marks of the onboarding dialog (`os-icon.tsx`): macOS, Linux, Windows.
pub const APPLE_PATH: &str = "M12.152 6.896c-.948 0-2.415-1.078-3.96-1.04-2.04.027-3.91 1.183-4.961 3.014-2.117 3.675-.546 9.103 1.519 12.09 1.013 1.454 2.208 3.09 3.792 3.039 1.52-.065 2.09-.987 3.935-.987 1.831 0 2.35.987 3.96.948 1.637-.026 2.676-1.48 3.676-2.948 1.156-1.688 1.636-3.325 1.662-3.415-.039-.013-3.182-1.221-3.22-4.857-.026-3.04 2.48-4.494 2.597-4.559-1.429-2.09-3.623-2.324-4.39-2.376-2-.156-3.675 1.09-4.61 1.09zM15.53 3.83c.843-1.012 1.4-2.427 1.245-3.83-1.207.052-2.662.805-3.532 1.818-.78.896-1.454 2.338-1.273 3.714 1.338.104 2.715-.688 3.559-1.701";
pub const WINDOWS_PATH: &str =
    "M0 0h11.4v11.4H0zM12.6 0H24v11.4H12.6zM0 12.6h11.4V24H0zM12.6 12.6H24V24H12.6z";
pub const TUX_PATH: &str = include_str!("tux.path");

/// The machine of the story, its API token as the dialog shows it, and the server.
pub const MACHINE: &str = "jobbdator";
pub const TOKEN: &str = "toki_3f9a1c7e5b2d4086a1f3c9e7b5d20486";
pub const SERVER: &str = "https://toki-api.spinit.se";

/// The headline total of the mocked range, in USD.
pub const TOTAL_COST: f32 = 13_842.6;
pub const TOTAL_TOKENS_B: f32 = 24.8;
pub const CACHE_READ_TOKENS_B: f32 = 23.9;
pub const REQUESTS: f32 = 176_204.0;
/// 14 Aug to 11 Oct 2026.
pub const DAYS: usize = 59;

/// Invented customers and their time-tracking projects, with their cost before the mapping
/// flow of the third probe. Together with `UNASSIGNED_COST` they sum to `TOTAL_COST`.
pub const PROJECTS: [(&str, f32); 5] = [
    ("Nordvik Energi – Kundportal", 3_412.80),
    ("Fjällbo Logistik – Ruttplanering", 3_105.40),
    ("Hamnkontoret – Bokningssystem", 2_480.10),
    ("Lindholm & Söner – Intranät", 1_720.30),
    ("Skogsbruket – Fältapp", 973.30),
];
pub const UNASSIGNED_COST: f32 = 2_150.70;

/// Repositories without a project yet: key, when last used, what their usage costs, and the
/// project of `PROJECTS` they belong to. The one without a project is the developer's own.
/// Together they are all of `UNASSIGNED_COST`.
pub const UNMAPPED: [(&str, &str, f32, Option<usize>); 5] = [
    (
        "github.com/spinit-ab/nordvik-portal-api",
        "Last used Sun, 11 Oct 2026",
        640.20,
        Some(0),
    ),
    (
        "github.com/ponbac/dotfiles",
        "Last used Sat, 10 Oct 2026",
        250.00,
        None,
    ),
    (
        "github.com/spinit-ab/fjallbo-ruttmotor",
        "Last used Fri, 9 Oct 2026",
        520.50,
        Some(1),
    ),
    (
        "dev.azure.com/hamnkontoret/Bokning/_git/web",
        "Last used Thu, 8 Oct 2026",
        410.00,
        Some(2),
    ),
    (
        "github.com/spinit-ab/lindholm-intranet",
        "Last used Mon, 5 Oct 2026",
        330.00,
        Some(3),
    ),
];
/// The app's name for usage that is not client work (`PERSONAL_LABEL`).
pub const PERSONAL: &str = "Personal / internal";

/// The mocked days, made once.
pub static DAYS_OF_USAGE: std::sync::LazyLock<Vec<Day>> = std::sync::LazyLock::new(days);

/// A small deterministic generator, so every render shows the same mocked data.
pub struct Random(u64);

impl Random {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// The next value from 0 up to 1.
    pub fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        ((self.0 >> 40) as f32) / ((1u64 << 24) as f32)
    }
}

/// One local day of usage: its cost per provider, in the order of `PROVIDER_NAMES`.
#[derive(Debug)]
pub struct Day {
    pub label: String,
    /// 0 is Monday.
    pub weekday: usize,
    pub cost: [f32; 4],
}

impl Day {
    pub fn total(&self) -> f32 {
        self.cost.iter().sum()
    }

    /// The day's cost per model, in the order of `MODELS`: the tools' costs regrouped.
    pub fn by_model(&self) -> [f32; 4] {
        let [codex, claude, copilot, pi] = self.cost;
        [
            0.62 * codex + 0.6 * pi,
            claude,
            0.38 * codex + 0.4 * pi,
            copilot,
        ]
    }
}

/// The models of the mocked usage by cost, with the first four categorical colours.
pub const MODELS: [&str; 4] = [
    "gpt-5.6-sol",
    "claude-opus-5-5",
    "gpt-6-astra",
    "gpt-6.1-sol",
];
pub const MODEL_COLORS: [&str; 4] = ["#3987e5", "#d95926", "#199e70", "#c98500"];

/// 59 days from Friday 14 Aug 2026: steady weekdays, light weekends, every tool every week.
pub fn days() -> Vec<Day> {
    let mut random = Random::new(0x70C1_A1B2);
    let mut days: Vec<Day> = (0..DAYS)
        .map(|i| {
            let weekday = (4 + i) % 7;
            let weekend = weekday >= 5;
            // A slow swell over the weeks, so the chart has a shape without any spike.
            let swell = 1.0 + 0.16 * ((i as f32 / DAYS as f32) * std::f32::consts::TAU * 1.5).sin();
            let total = if weekend {
                28.0 + random.next() * 78.0
            } else {
                (232.0 + random.next() * 150.0) * swell
            };
            let codex = 0.40 + random.next() * 0.14;
            let claude = 0.28 + random.next() * 0.14;
            let copilot = 0.03 + random.next() * 0.04;
            let pi = 1.0 - codex - claude - copilot;
            Day {
                label: date_label(i),
                weekday,
                cost: [total * codex, total * claude, total * copilot, total * pi],
            }
        })
        .collect();
    // The days add up to the headline figure exactly.
    let scale = TOTAL_COST / days.iter().map(Day::total).sum::<f32>();
    for day in &mut days {
        for cost in &mut day.cost {
            *cost *= scale;
        }
    }
    days
}

/// Day `index` of the range as the app writes it, such as `17 Aug` or `7 Sept`.
fn date_label(index: usize) -> String {
    let day = 14 + index;
    match day {
        ..=31 => format!("{day} Aug"),
        32..=61 => format!("{} Sept", day - 31),
        _ => format!("{} Oct", day - 61),
    }
}

/// Cost per hour of the week, 7 rows of 24: office hours on weekdays, some evenings, quiet
/// nights and weekends.
pub fn hours() -> [[f32; 24]; 7] {
    let mut random = Random::new(0x5EED_0042);
    let mut hours = [[0.0; 24]; 7];
    for (weekday, row) in hours.iter_mut().enumerate() {
        for (hour, cell) in row.iter_mut().enumerate() {
            let weekend = weekday >= 5;
            let level = match hour {
                8..=11 => 0.78,
                12 => 0.5,
                13..=16 => 0.88,
                17 => 0.52,
                19..=21 => 0.3,
                7 | 18 | 22 => 0.16,
                _ => 0.0,
            };
            let level = if weekend { level * 0.3 } else { level };
            // Friday afternoons trail off.
            let level = if weekday == 4 && hour >= 15 {
                level * 0.6
            } else {
                level
            };
            let noise = 0.72 + random.next() * 0.5;
            *cell = if level > 0.0 && (!weekend || random.next() > 0.35) {
                level * noise * 240.0
            } else {
                0.0
            };
        }
    }
    hours
}
