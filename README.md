# Musify

A music player for Linux, written in Rust with [iced](https://iced.rs). It draws its own
window chrome (no system title bar) and plays your YouTube Music library alongside music
from a folder on your computer.

Design: [Figma file](https://www.figma.com/design/PKb810t2RgkmpODRPmzmB0).

## Build

Playback goes through rodio, which needs the ALSA headers:

```sh
sudo dnf install alsa-lib-devel      # Fedora
cargo run --release
```

Without them, build with `--no-default-features`. Musify then runs with a silent clock in
place of the sound device, which is handy for working on the UI.

## YouTube Music

Press **Connect YouTube Music** in the rail. Musify looks for a Firefox profile that's signed
in to YouTube and copies its youtube.com cookies to `~/.local/share/musify/youtube-cookies.txt`
(readable only by you). If none is signed in, it opens a Firefox window with its own profile;
sign in there and close it.

It then reads your library playlists, Liked Music and the home page (mixes, Listen again…)
through the same private API music.youtube.com uses, read-only. Songs are downloaded with
**yt-dlp** into `~/.cache/musify/audio` when you play them, and the next song is fetched ahead.

yt-dlp needs a JavaScript runtime and YouTube's challenge solver to get audio:

```sh
sudo dnf install yt-dlp nodejs   # Musify turns on node for yt-dlp automatically
pip install --user yt-dlp-ejs     # the challenge solver yt-dlp uses
```

This isn't an official YouTube integration and goes against YouTube's Terms of Service; it
can break when YouTube changes things, usually fixed by updating yt-dlp.

## Use

```sh
musify                 # reads your XDG music folder (usually ~/Music)
musify ~/some/folder   # reads another folder
```

- Click a song to play it from there; the queue is the rest of that list.
- The strip under the controls is the queue. Each song's width is its length; click inside
  the current song to seek, or on a later song to jump to it.
- Space plays and pauses. Escape leaves Now playing.
- Lyrics come from an `.lrc` file next to a local song; otherwise Musify looks them up on
  [LRCLIB](https://lrclib.net) (synced) and YouTube Music (plain text), sending the song's
  title, artist, album and length. Results are cached in `~/.cache/musify/lyrics`.
- Liked songs are saved to `~/.local/share/musify/liked.txt`.

Window: drag the top of the main area (or the wordmark) to move, double-click it to maximize,
drag the outer edge to resize.

## Layout

| File | What it holds |
| --- | --- |
| `src/main.rs` | State, messages, playback queue logic |
| `src/view.rs` | Every screen: chrome, rail, album/list/home pages, player bar, Now playing |
| `src/canvas.rs` | Queue ribbon, generated album covers, startup animation |
| `src/anim.rs` | Tweens, easing, and the hover-fade wrapper widget |
| `src/library.rs` | Folder scan, tags (lofty), album colors from cover art |
| `src/audio.rs` | rodio playback, or the silent clock |
| `src/youtube/` | Firefox login import, YouTube Music API client, yt-dlp downloads |
| `src/theme.rs` | Colors, fonts, widget styles (Figma tokens) |

Landscape and portrait layouts switch automatically: under 1000px wide, or taller than wide,
the rail collapses to spine tiles and the player becomes a two-row dock.

### Debug helpers

Debug builds read these environment variables for checking layouts and motion:

- `MUSIFY_SIZE=760x1240` sets the starting window size.
- `MUSIFY_PREVIEW=album|playing|now` opens straight into that state; `sheet` and `next` open the
  Now playing sheet or skip a song shortly after the intro, to check those animations.
- `MUSIFY_ALBUM="Low Tide Choir"` picks which album the preview uses.
- `musify --yt-check` signs in from Firefox and prints how much of your library came back.

## Fonts

Onest and Bricolage Grotesque, bundled under the SIL Open Font License (see `assets/fonts`).
