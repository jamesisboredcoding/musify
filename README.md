# Musify

A music player for Linux, written in Rust with [iced](https://iced.rs). It plays your YouTube
Music library and music from a folder on your computer.

## Build

Playback uses rodio, which needs the ALSA headers:

```sh
sudo dnf install alsa-lib-devel      # Fedora
cargo run --release
```

To add Musify to your app launcher, run `scripts/install-desktop.sh "$PWD/target/release/musify"`.

## YouTube Music

Press **Connect YouTube Music** in the sidebar. Musify looks for a Firefox profile that's signed
in to YouTube and copies its youtube.com cookies to `~/.local/share/musify/youtube-cookies.txt`
(readable only by you). If none is signed in, it opens a Firefox window; sign in there and
close it.

Musify then loads your playlists, Liked Music and the mixes from your home page. Liking a
song in Musify likes it on your YouTube account. Songs are downloaded with **yt-dlp** into
`~/.cache/musify/audio` when you play them.

yt-dlp needs a JavaScript runtime and YouTube's challenge solver:

```sh
sudo dnf install yt-dlp nodejs
pip install --user yt-dlp-ejs
```

This isn't an official YouTube integration and goes against YouTube's Terms of Service. It can
break when YouTube changes things; updating yt-dlp usually fixes it.

## Use

```sh
musify                 # reads your music folder (usually ~/Music)
musify ~/some/folder   # reads another folder
```

- Click a song to play it; the rest of that list becomes the queue.
- The strip under the controls is the queue, each song as wide as it is long. Click the
  current song to seek, or a later one to jump to it.
- Search looks through your library and all of YouTube Music: songs, albums and playlists.
- Space plays and pauses. Escape leaves Now playing.
- Lyrics come from an `.lrc` file next to a local song, otherwise from
  [LRCLIB](https://lrclib.net) or YouTube Music (this sends the song's title, artist, album
  and length). They're cached in `~/.cache/musify/lyrics`.

## Fonts

Onest and Bricolage Grotesque, bundled under the SIL Open Font License (see `assets/fonts`).
