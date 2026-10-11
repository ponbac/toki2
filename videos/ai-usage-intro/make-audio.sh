#!/usr/bin/env bash
# Synthesizes the soundtrack into media/: a pad in A minor and three effects in the same key.
# The files are generated, not committed. Pass the length of the film in seconds when it
# changes (see `cargo run --release -- timeline`).
set -euo pipefail
cd "$(dirname "$0")"

seconds="${1:-112}"
fade_at=$((seconds - 4))

ffmpeg -y -v error -f lavfi -i "aevalsrc='0.10*sin(2*PI*110*t)*(0.75+0.25*sin(2*PI*0.11*t))+0.07*sin(2*PI*164.81*t)*(0.7+0.3*sin(2*PI*0.07*t+1))+0.05*sin(2*PI*220.4*t)+0.04*sin(2*PI*261.63*t)*(0.6+0.4*sin(2*PI*0.05*t+2))+0.025*sin(2*PI*329.63*t)*(0.5+0.5*sin(2*PI*0.09*t+3))':s=48000:d=${seconds}" \
  -af "lowpass=f=1800,aecho=0.8:0.7:120|250:0.3|0.2,afade=in:d=1.5,afade=out:st=${fade_at}:d=4" -ac 2 media/bed.wav

ffmpeg -y -v error -f lavfi -i "aevalsrc='(random(0)*2-1)*exp(-mod(t,1/15)*260)*0.5':s=48000:d=1.25" \
  -af "highpass=f=2500,lowpass=f=9000,afade=out:st=1.15:d=0.1" -ac 2 media/typing.wav

ffmpeg -y -v error -f lavfi -i "aevalsrc='(0.5*sin(2*PI*880*t)+0.2*sin(2*PI*1760*t))*exp(-t*26)':s=48000:d=0.45" \
  -af "afade=in:d=0.004,aecho=0.8:0.5:60:0.2" -ac 2 media/click.wav

ffmpeg -y -v error -f lavfi -i "aevalsrc='0.4*sin(2*PI*440*t)*exp(-t*7)+0.35*gt(t,0.11)*sin(2*PI*523.25*(t-0.11))*exp(-(t-0.11)*7)+0.35*gt(t,0.22)*sin(2*PI*659.25*(t-0.22))*exp(-(t-0.22)*6)':s=48000:d=1.6" \
  -af "afade=in:d=0.004,aecho=0.8:0.6:90|180:0.25|0.15,afade=out:st=1.3:d=0.3" -ac 2 media/chime.wav

# The media folder is embedded at compile time, so the next build must see the new files.
touch src/lib.rs
