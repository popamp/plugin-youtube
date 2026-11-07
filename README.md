# YouTube for Popamp

A [Popamp](https://github.com/popamp/popamp) source plug-in: plays YouTube
videos, playlists and channels. [yt-dlp](https://github.com/yt-dlp/yt-dlp)
finds the audio stream and ffmpeg decodes it; the plug-in itself only
runs those two programs and never touches the network or your files.

## Install

You need Popamp Plus, the build with plug-ins, 0.6.0 or newer. From
[apt.popamp.app](https://apt.popamp.app), with `yt-dlp` and `ffmpeg`:

```sh
sudo apt install popamp-plugin-youtube
```

It stays off until you allow it, as below. Or, with any build with
plug-ins (`cargo build --release --features plugins`) and `yt-dlp` and
`ffmpeg` installed, clone it into Popamp's plug-ins folder:

```sh
git clone https://github.com/popamp/plugin-youtube ~/.local/share/popamp/plugins/youtube
```

The folder must be called `youtube`, the plug-in's id. Then in Popamp:
Options > Plug-ins…, tick YouTube (it says it runs yt-dlp and ffmpeg with
your rights) and press OK. Paste a YouTube link with `Ctrl+L`.

To update: `git pull` in that folder.

## Settings

In Options > Plug-ins…:

- **yt-dlp program**, **ffmpeg program**: a path, or a name looked up in
  `PATH`.
- **yt-dlp format**: the format selector, `bestaudio[ext=m4a]/bestaudio`
  by default.

Popamp's HTTP proxy (Preferences) is passed to both programs.

Keep `yt-dlp` up to date (`pipx install yt-dlp`; distribution packages
are often too old), because YouTube changes often break old versions.
Recent versions also want a JavaScript runtime such as `deno` for full
format support.

## Tested with

yt-dlp 2026.10.26 and ffmpeg 6.1.1 on Ubuntu: a video and a `youtu.be`
link (title, uploader, length, seeking), a channel's videos, and an
unavailable video (its error shows in Popamp).

## Build

```sh
rustup target add wasm32-unknown-unknown   # once
./build.sh                                 # writes plugin.wasm
cargo test                                 # the logic, natively
```

`plugin.wasm` is committed, so cloning the repository installs a working
plug-in; rebuild it with `./build.sh` whenever `src/` changes.

## License

MIT or Apache-2.0, at your option.
