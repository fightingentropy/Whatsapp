# Synthetic media fixtures

`h264-bframes.mp4` contains four red, four green and four blue 96x64 frames,
at 10 fps, encoded with two B frames. It contains no real messages or media.
It exercises decoder reordering, final-frame flushing, colour channels and timing.

Generated with FFmpeg/libx264:

```sh
ffmpeg -f lavfi -i 'color=c=red:s=96x64:r=10:d=0.4' \
  -f lavfi -i 'color=c=lime:s=96x64:r=10:d=0.4' \
  -f lavfi -i 'color=c=blue:s=96x64:r=10:d=0.4' \
  -filter_complex '[0:v][1:v][2:v]concat=n=3:v=1:a=0[v]' -map '[v]' \
  -c:v libx264 -pix_fmt yuv420p -bf 2 -g 12 -movflags +faststart h264-bframes.mp4
```

`h264-preview.mp4` uses the same command with `s=640x360`. It exercises the
automatic hardware path and its preview-size output.

Tests read the committed fixtures; FFmpeg is not required to run them.

`inline-video.mp4` contains six seconds of 640x360 H.264 at 24 fps: two seconds
each of red, green and blue, with a white top-left marker and yellow bottom-right
marker. A quiet 440 Hz stereo AAC tone checks the enabled audio track and mute.
`inline-video-portrait.mp4` remuxes it with a 90-degree rotation tag, preserving
the encoded pixels so the native track transform is exercised.

```sh
ffmpeg -f lavfi -i 'color=c=red:s=640x360:r=24:d=2' \
  -f lavfi -i 'color=c=lime:s=640x360:r=24:d=2' \
  -f lavfi -i 'color=c=blue:s=640x360:r=24:d=2' \
  -f lavfi -i 'sine=frequency=440:sample_rate=48000:duration=6' \
  -filter_complex '[0:v][1:v][2:v]concat=n=3:v=1:a=0,drawbox=x=0:y=0:w=80:h=80:color=white:t=fill,drawbox=x=560:y=280:w=80:h=80:color=yellow:t=fill[v];[3:a]volume=0.02[a]' \
  -map '[v]' -map '[a]' -c:v libx264 -pix_fmt yuv420p -bf 2 -g 24 \
  -c:a aac -b:a 32k -ac 2 -movflags +faststart inline-video.mp4
ffmpeg -display_rotation 90 -i inline-video.mp4 -c copy inline-video-portrait.mp4
```
