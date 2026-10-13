use fframes::cpu::CpuRenderingBackend;
use fframes::{EncoderOptions, RenderOptions, StaticMediaProvider, cli};
use std::process::ExitCode;
use toki_ai_usage::{TokiAiUsageMedia, TokiAiUsageVideo};

fn main() -> ExitCode {
    let media = TokiAiUsageMedia::prepare().expect("media");
    let video = TokiAiUsageVideo::new(&media);

    cli::new(
        &video,
        RenderOptions {
            media: Some(&media),
            // A near-lossless MPEG-4 mezzanine. The H.264 feature of fframes links the system
            // x264, whose ABI is newer than the bundled FFmpeg expects here, so the system
            // ffmpeg makes the final H.264 from this file instead.
            video_encoder_options: EncoderOptions {
                bitrate: Some(80_000_000),
                qmin: 1,
                qmax: 3,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    // The renderer keeps rendered layers between frames, keyed by their content without the
    // opacity of the groups inside. A layer whose only change is such an opacity, like a
    // tooltip fading in over a still page, would be reused as it was. Without the cache
    // every frame is drawn as it is described.
    .backend(CpuRenderingBackend {
        cache_capacity: 0,
        ..Default::default()
    })
    .run()
}
