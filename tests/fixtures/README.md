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
