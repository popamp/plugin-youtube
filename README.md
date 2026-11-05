# YouTube for Popamp

A [Popamp](https://github.com/popamp/popamp) source plug-in: plays YouTube
videos, playlists and channels. [yt-dlp](https://github.com/yt-dlp/yt-dlp)
finds the audio stream and ffmpeg decodes it; the plug-in itself only
runs those two programs and never touches the network or your files.

## Install

You need Popamp built with plug-ins (`cargo build --release --features
plugins`), and `yt-dlp` and `ffmpeg` installed.

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
