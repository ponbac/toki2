#!/usr/bin/env bash
# Renders the film to out/ai-usage-intro.mp4 with its poster beside it.
set -euo pipefail
cd "$(dirname "$0")"

[ -f media/bed.wav ] || ./make-audio.sh
mkdir -p out

# fframes writes a near-lossless MPEG-4 file; the system ffmpeg makes the H.264 from it.
cargo run --release -- render -o out/raw.mp4
ffmpeg -y -v error -i out/raw.mp4 -c:v libx264 -crf 18 -preset slow -pix_fmt yuv420p \
  -c:a aac -b:a 192k -movflags +faststart out/ai-usage-intro.mp4
ffmpeg -y -v error -i out/ai-usage-intro.mp4 -frames:v 1 -q:v 2 out/ai-usage-intro.jpg
rm out/raw.mp4

echo "out/ai-usage-intro.mp4"
