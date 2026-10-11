# AI usage intro video

The source of the film that introduces the AI usage page: connect a machine with the
onboarding dialog, paste its commands, let the daily job sync, read the page, map
repositories to projects. It is 1920x1080 at 30 fps, 112 seconds, with Swedish captions over
the app's English UI.

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
cargo run --release -- frame "Mapping@6.8s"      # one frame to frames/
cargo run --release -- strip "Synced@3s..Synced@6s" -n 8   # contact sheet of a movement
cargo run --release -- inspect                   # clipped text and invalid shapes in any frame
cargo run --release -- audio analyze             # loudness per scene
```

## Layout

| File | What |
| --- | --- |
| `src/lib.rs` | The video, the theme, and shared pieces: captions, cards, buttons, pointer |
| `src/data.rs` | The mocked usage, and the marks of tools and systems |
| `src/intro.rs`, `src/outro.rs` | Opening and close. The film opens on its poster, which dissolves |
| `src/connect.rs` | Empty state and the onboarding dialog, steps 1 to 3 |
| `src/terminal.rs` | `login`, `sync` and `schedule install` with what they print |
| `src/synced.rs` | The dialog's last step, and the page filling in |
| `src/schedule.rs` | A week of the daily job, and the Machines card |
| `src/usage.rs`, `src/breakdown.rs`, `src/heatmap.rs` | The page's charts and the sessions table |
| `src/mapping.rs` | Mapping repositories, and marking one Personal / internal |

## When the UI changes

The scenes follow the app at commit `679cd6d6` (the onboarding dialog and Personal /
internal). Nothing ties them to the components, so a change to the page's layout or text
needs the matching scene redrawn. Texts are copied verbatim from the components under
`app/src/routes/_layout/ai-usage/`, which makes them easy to find again.

## Data

All customers, projects, repositories, the machine and the API token are invented. The
totals are close to a heavy user's but spread evenly over days, tools and projects. The
lines in the terminal are what token-ledger 0.3.0 prints; the row of tool marks under `sync`
is an addition.

## Notes

- The `h264` feature of fframes links the system x264, and its prebuilt FFmpeg expects an
  older x264 than current distributions ship. The feature is off, and `render.sh` encodes
  the H.264 with the system `ffmpeg` instead.
- Third-party material, with its license texts in `licenses/`: the fonts Manrope and IBM
  Plex Mono (SIL Open Font License), the tool marks from `@lobehub/icons-static-svg` (MIT),
  and the line icons from Lucide (ISC). The Apple and Tux marks are from Simple Icons, which
  is CC0 and needs no notice.
