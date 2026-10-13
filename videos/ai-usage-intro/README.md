# AI usage intro video

The source of the film that introduces the AI usage page: why usage is collected, then
connect a machine with the onboarding dialog, paste its commands, let the daily job sync,
declare subscriptions, read the page tab by tab, and map repositories to projects. It is
1920x1080 at 30 fps, 151 seconds, with Swedish captions over the app's English UI.

It is made with [fframes](https://github.com/dmtrKovalenko/fframes): every frame is an SVG
tree returned from Rust. The UI is not the app's React components but a drawing of them, in
the app's own CSS pixels at 1.5x, with its fonts, colours and icons.

## Render

Needs Rust and `ffmpeg` with libx264.

```sh
./render.sh        # writes out/ai-usage-intro.mp4 and its poster
```

The first build downloads a prebuilt FFmpeg for fframes and takes a few minutes; a render
takes about ten seconds. The soundtrack is synthesized by `make-audio.sh` on the first run.

While working on a scene:

```sh
cargo run --release -- timeline                  # scenes and audio tracks
cargo run --release -- frame "Repositories@6.8s" # one frame to frames/
cargo run --release -- strip "Synced@3s..Synced@6s" -n 8   # contact sheet of a movement
cargo run --release -- inspect                   # clipped text and invalid shapes in any frame
cargo run --release -- audio analyze             # loudness per scene
```

## Layout

| File | What |
| --- | --- |
| `src/lib.rs` | The video, the theme, and shared pieces: captions, cards, buttons, pointer |
| `src/data.rs` | The mocked usage, and the marks of tools and systems |
| `src/money.rs` | Kronor and dollars as the page writes them, and the rate between them |
| `src/intro.rs`, `src/outro.rs` | Opening and close. The film opens on its poster, which dissolves |
| `src/purpose.rs` | Why: AI costs are billed on to customers from real usage |
| `src/connect.rs` | Empty state and the onboarding dialog, steps 1 to 3 |
| `src/terminal.rs` | `login`, `sync` and `schedule install` with what they print |
| `src/synced.rs` | The dialog's last step, and the Overview filling in |
| `src/schedule.rs` | A week of the daily job, and the machine's card on the Machines tab |
| `src/subscription.rs` | Declaring a subscription under Account, and how its fee is split |
| `src/page.rs` | The page itself: header, tabs, filters, summary strip, daily chart, "Where it went", hour heatmap |
| `src/overview.rs`, `src/daily.rs`, `src/whereabouts.rs` | The Overview tab from the top down: the currency switch, the chart by tool and model, and where the cost went |
| `src/repositories.rs` | The Repositories tab: mapping repositories, and marking one Personal / internal, beside a drawing of where the cost lies |

## When the UI changes

The scenes follow the app at commit `c0d32587` (the page split into four tabs, the Copilot
step of the onboarding dialog, Personal / internal, and costs shown in SEK with the currency
switch). Nothing ties them to the components, so a change to the page's layout or text needs
the matching scene redrawn. The page is taller than the frame; `page::view` shows a part of
it, and a scene scrolls by changing that part. Texts are copied verbatim from the components under
`app/src/routes/_layout/ai-usage/`, which makes them easy to find again.

## Data

All customers, projects, repositories, the machine and the API token are invented. The
totals are close to a heavy user's but spread evenly over days, tools and projects.
Estimates are priced in US dollars and shown in kronor at 10.0174 kr, the rate the page
named for October 2026; only the terminal shows dollars, as the CLI prints them. The
lines in the terminal are what token-ledger 0.5.1 prints; the row of tool marks under `sync`
is an addition. Ticking Copilot adds a step to the dialog that turns on Copilot's log; the
film names it and does not show it. The week of daily runs, the table of a fee's split and
the bars of where the cost lies beside the Repositories tab are explanatory drawings, not
screens of the app; the split follows the app's rule and is computed, not typed.

## Notes

- The renderer's layer cache is off (`cache_capacity: 0` in `src/main.rs`). It keys a
  rendered layer by its content without the opacity of the groups inside, so a layer whose
  only change is such an opacity, like a tooltip fading in over a still page, was reused as
  it had been. `frame` and `strip` render each frame on its own and do not show this, so
  check motion in the rendered file.
- The `h264` feature of fframes links the system x264, and its prebuilt FFmpeg expects an
  older x264 than current distributions ship. The feature is off, and `render.sh` encodes
  the H.264 with the system `ffmpeg` instead.
- Third-party material, with its license texts in `licenses/`: the fonts Manrope and IBM
  Plex Mono (SIL Open Font License), the tool marks from `@lobehub/icons-static-svg` (MIT),
  the line icons from Lucide (ISC), and the VS Code and Visual Studio logos from Devicon
  (MIT). The Apple, Tux, GitHub and Azure DevOps marks are from Simple Icons, which is CC0
  and needs no notice.
