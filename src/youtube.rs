//! What the plug-in does, without Extism: the yt-dlp arguments, reading its
//! JSON, the ffmpeg pipe and the cache. Pure, so it is tested natively.
//!
//! Stream URLs expire after a few hours, so playlists keep the YouTube page
//! URL and it is resolved again (with a short cache) whenever it is played.

use popamp_plugin_api::{ExecOutput, Play, Tags, Track, START};
use serde_json::Value;
use std::collections::HashMap;

/// ffmpeg's output rate.
pub const RATE: u32 = 48_000;
/// How long a resolved stream URL is used again.
pub const CACHE_TTL_MS: u64 = 3_600_000;
/// Upper bound for expanding playlists and channels.
pub const MAX_PLAYLIST: usize = 500;

/// The plug-in's configuration, as the player gives it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Settings {
    /// yt-dlp format selector.
    pub format: String,
    /// HTTP proxy (`[user:pass@]host:port`), empty for none.
    pub proxy: String,
}

impl Settings {
    fn proxy_url(&self) -> Option<String> {
        let p = self.proxy.trim();
        (!p.is_empty()).then(|| format!("http://{p}"))
    }

    fn ytdlp_args(&self, args: &[&str]) -> Vec<String> {
        let mut all: Vec<String> = ["--no-warnings", "--no-progress"].map(String::from).into();
        if let Some(p) = self.proxy_url() {
            all.extend(["--proxy".into(), p]);
        }
        all.extend(args.iter().map(|a| a.to_string()));
        all
    }

    /// yt-dlp arguments that print one video's JSON with its audio stream.
    pub fn resolve_args(&self, url: &str) -> Vec<String> {
        self.ytdlp_args(&["-j", "--no-playlist", "-f", &self.format, url])
    }

    /// yt-dlp arguments that list a playlist or channel without resolving it.
    pub fn list_args(&self, url: &str) -> Vec<String> {
        self.ytdlp_args(&["-J", "--flat-playlist", "--playlist-end", &MAX_PLAYLIST.to_string(), url])
    }
}

/// True when the URL names one video (as opposed to a playlist or channel).
pub fn is_single_video(url: &str) -> bool {
    let Some((_, rest)) = url.split_once("://") else { return false };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("").to_ascii_lowercase();
    let path = rest.find('/').map(|i| &rest[i..]).unwrap_or("");
    if host.ends_with("youtu.be") {
        return path.len() > 1;
    }
    let query = url.split_once('?').map(|(_, q)| q).unwrap_or("");
    path.starts_with("/shorts/")
        || path.starts_with("/live/")
        || path.starts_with("/embed/")
        || query.split('&').any(|kv| kv.starts_with("v="))
}

/// yt-dlp's JSON, or the error to show.
pub fn ytdlp_json(out: ExecOutput) -> Result<Value, String> {
    if let Some(e) = out.error {
        return Err(match e.ends_with(" not found") {
            true => format!("{e}: install yt-dlp to play YouTube links"),
            false => e,
        });
    }
    if out.status != Some(0) {
        let err = &out.stderr;
        let line = err.lines().rev().find(|l| l.contains("ERROR")).or_else(|| err.lines().last()).unwrap_or("yt-dlp failed");
        return Err(line.trim().trim_start_matches("ERROR: ").to_string());
    }
    serde_json::from_str(&out.stdout).map_err(|e| format!("yt-dlp returned bad JSON: {e}"))
}

fn str_field(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| v.get(*k).and_then(|x| x.as_str()).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string))
}

fn length_ms(v: &Value) -> Option<u64> {
    v.get("duration").and_then(|d| d.as_f64()).filter(|d| *d > 0.0).map(|d| (d * 1000.0) as u64)
}

pub fn tags_from_json(v: &Value) -> Tags {
    Tags {
        title: str_field(v, &["track", "title"]),
        artist: str_field(v, &["artist", "creator", "uploader", "channel"]),
        album: str_field(v, &["album"]),
        year: str_field(v, &["release_year"]).or_else(|| v.get("release_year").and_then(|y| y.as_u64()).map(|y| y.to_string())),
        genre: str_field(v, &["genre"]),
        comment: str_field(v, &["webpage_url"]),
        length_ms: length_ms(v),
        bitrate_kbps: v.get("abr").and_then(|b| b.as_f64()).map(|b| b.round() as u32),
        sample_rate: v.get("asr").and_then(|r| r.as_u64()).map(|r| r as u32),
        channels: v.get("audio_channels").and_then(|c| c.as_u64()).map(|c| c as u16),
        codec: str_field(v, &["acodec"]).map(|c| c.split('.').next().unwrap_or(&c).to_uppercase()),
    }
}

/// A video's audio stream, as yt-dlp found it.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub stream_url: String,
    pub headers: Vec<(String, String)>,
    pub tags: Tags,
    pub is_live: bool,
}

impl Resolved {
    pub fn from_json(v: &Value) -> Result<Resolved, String> {
        let stream_url = str_field(v, &["url"]).ok_or("yt-dlp found no audio stream")?;
        let headers = v
            .get("http_headers")
            .and_then(|h| h.as_object())
            .map(|h| h.iter().filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string()))).collect())
            .unwrap_or_default();
        let is_live = v.get("is_live").and_then(|l| l.as_bool()).unwrap_or(false);
        Ok(Resolved { stream_url, headers, tags: tags_from_json(v), is_live })
    }

    /// How to play it: ffmpeg decodes the stream to 48 kHz stereo `f32`.
    pub fn play(self, settings: &Settings) -> Play {
        let mut args: Vec<String> = ["-nostdin", "-hide_banner", "-loglevel", "error"].map(String::from).into();
        args.extend(["-reconnect", "1", "-reconnect_streamed", "1", "-reconnect_delay_max", "5"].map(String::from));
        let mut extra = String::new();
        for (k, v) in &self.headers {
            if k.eq_ignore_ascii_case("user-agent") {
                args.extend(["-user_agent".into(), v.clone()]);
            } else {
                extra.push_str(&format!("{k}: {v}\r\n"));
            }
        }
        if !extra.is_empty() {
            args.extend(["-headers".into(), extra]);
        }
        if let Some(p) = settings.proxy_url() {
            args.extend(["-http_proxy".into(), p]);
        }
        if !self.is_live {
            args.extend(["-ss".into(), START.into()]);
        }
        args.extend(["-i".into(), self.stream_url]);
        args.extend(["-vn", "-f", "f32le", "-ac", "2", "-ar"].map(String::from));
        args.extend([RATE.to_string(), "pipe:1".into()]);
        let mut tags = self.tags;
        tags.codec.get_or_insert_with(|| "YOUTUBE".into());
        if self.is_live {
            tags.length_ms = None;
        }
        Play::Pipe { program: "ffmpeg".into(), args, rate: RATE, seekable: !self.is_live, tags }
    }
}

/// The videos of a playlist or channel listing.
pub fn parse_listing(v: &Value) -> Vec<Track> {
    let Some(entries) = v.get("entries").and_then(|e| e.as_array()) else {
        // Not a playlist after all: the page itself is the video.
        return str_field(v, &["webpage_url", "original_url"])
            .map(|location| vec![Track { location, title: str_field(v, &["title"]), length_ms: length_ms(v) }])
            .unwrap_or_default();
    };
    entries
        .iter()
        .filter_map(|e| {
            let location = match str_field(e, &["url"]) {
                Some(u) if u.starts_with("http") => u,
                _ => format!("https://www.youtube.com/watch?v={}", str_field(e, &["id"])?),
            };
            let title = match (str_field(e, &["uploader", "channel"]), str_field(e, &["title"])) {
                (Some(a), Some(t)) => Some(format!("{a} - {t}")),
                (_, t) => t,
            };
            Some(Track { location, title, length_ms: e.get("duration").and_then(|d| d.as_f64()).map(|d| (d * 1000.0) as u64) })
        })
        .collect()
}

/// Resolved videos by page URL, each kept for [`CACHE_TTL_MS`].
#[derive(Debug, Default)]
pub struct Cache(HashMap<String, (u64, Resolved)>);

impl Cache {
    pub fn get(&self, url: &str, now_ms: u64) -> Option<Resolved> {
        self.0.get(url).filter(|(at, _)| fresh(*at, now_ms)).map(|(_, r)| r.clone())
    }

    /// Keeps `r`, and forgets what has expired.
    pub fn put(&mut self, url: &str, now_ms: u64, r: Resolved) {
        self.0.retain(|_, (at, _)| fresh(*at, now_ms));
        self.0.insert(url.to_string(), (now_ms, r));
    }
}

fn fresh(at_ms: u64, now_ms: u64) -> bool {
    now_ms.saturating_sub(at_ms) < CACHE_TTL_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_vs_list() {
        assert!(is_single_video("https://www.youtube.com/watch?v=abc&list=PL1"));
        assert!(is_single_video("https://youtu.be/abc"));
        assert!(is_single_video("https://www.youtube.com/shorts/abc"));
        assert!(is_single_video("https://www.youtube.com/live/abc"));
        assert!(!is_single_video("https://youtu.be/"));
        assert!(!is_single_video("https://www.youtube.com/playlist?list=PL1"));
        assert!(!is_single_video("https://www.youtube.com/@someone/videos"));
    }

    #[test]
    fn asks_ytdlp_through_the_proxy_for_the_format() {
        let s = Settings { format: "bestaudio".into(), proxy: " h:1 ".into() };
        assert_eq!(
            s.resolve_args("https://youtu.be/a"),
            ["--no-warnings", "--no-progress", "--proxy", "http://h:1", "-j", "--no-playlist", "-f", "bestaudio", "https://youtu.be/a"]
        );
        let none = Settings { format: "f".into(), proxy: String::new() };
        assert_eq!(none.list_args("u"), ["--no-warnings", "--no-progress", "-J", "--flat-playlist", "--playlist-end", "500", "u"]);
    }

    #[test]
    fn says_why_ytdlp_gave_nothing() {
        let out = |error: Option<&str>, status, stdout: &str, stderr: &str| ExecOutput {
            error: error.map(String::from),
            status,
            stdout: stdout.into(),
            stderr: stderr.into(),
        };
        assert_eq!(
            ytdlp_json(out(Some("/usr/bin/yt-dlp not found"), None, "", "")),
            Err("/usr/bin/yt-dlp not found: install yt-dlp to play YouTube links".into())
        );
        assert_eq!(
            ytdlp_json(out(Some("yt-dlp took too long and was stopped"), None, "", "")),
            Err("yt-dlp took too long and was stopped".into())
        );
        assert_eq!(
            ytdlp_json(out(None, Some(1), "", "WARNING: x\nERROR: [youtube] a: Video unavailable\nmore\n")),
            Err("[youtube] a: Video unavailable".into())
        );
        assert_eq!(ytdlp_json(out(None, Some(1), "", "")), Err("yt-dlp failed".into()));
        assert_eq!(ytdlp_json(out(None, None, "", "killed\n")), Err("killed".into()));
        assert!(ytdlp_json(out(None, Some(0), "not json", "")).unwrap_err().starts_with("yt-dlp returned bad JSON"));
        assert_eq!(ytdlp_json(out(None, Some(0), r#"{"a":1}"#, "")), Ok(serde_json::json!({"a":1})));
    }

    #[test]
    fn parses_listings_and_tags() {
        let v: Value = serde_json::from_str(
            r#"{"entries":[
                {"id":"aaa","title":"One","uploader":"Band","duration":61.5},
                {"url":"https://www.youtube.com/watch?v=bbb","title":"Two"},
                {"title":"no id"}
            ]}"#,
        )
        .unwrap();
        let items = parse_listing(&v);
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0],
            Track { location: "https://www.youtube.com/watch?v=aaa".into(), title: Some("Band - One".into()), length_ms: Some(61_500) }
        );
        assert_eq!(items[1].title.as_deref(), Some("Two"));
        let page = serde_json::json!({"webpage_url":"https://youtu.be/x","title":"Solo","duration":2});
        assert_eq!(
            parse_listing(&page),
            [Track { location: "https://youtu.be/x".into(), title: Some("Solo".into()), length_ms: Some(2000) }]
        );

        let t = tags_from_json(
            &serde_json::json!({"title":"Vid","uploader":"Chan","duration":19,"acodec":"mp4a.40.2","abr":129.8,"release_year":1999}),
        );
        assert_eq!((t.title.as_deref(), t.artist.as_deref(), t.year.as_deref()), (Some("Vid"), Some("Chan"), Some("1999")));
        assert_eq!((t.length_ms, t.codec.as_deref(), t.bitrate_kbps), (Some(19_000), Some("MP4A"), Some(130)));
    }

    fn resolved() -> Resolved {
        let v = serde_json::json!({
            "url": "https://media/a",
            "http_headers": {"User-Agent": "UA", "Referer": "r"},
            "duration": 5,
        });
        Resolved::from_json(&v).unwrap()
    }

    #[test]
    fn plays_a_video_through_ffmpeg_from_the_start_asked() {
        let Play::Pipe { program, args, rate, seekable, tags } = resolved().play(&Settings { proxy: "h:1".into(), ..Default::default() })
        else {
            panic!()
        };
        let at = |a: &str| args.iter().position(|x| x == a).unwrap();
        assert_eq!((program.as_str(), rate), ("ffmpeg", RATE));
        assert_eq!(args[at("-ss") + 1], START);
        assert!(at("-ss") < at("-i"));
        assert_eq!(args[at("-i") + 1], "https://media/a");
        assert_eq!(args[at("-user_agent") + 1], "UA");
        assert_eq!(args[at("-headers") + 1], "Referer: r\r\n");
        assert_eq!(args[at("-http_proxy") + 1], "http://h:1");
        assert_eq!((seekable, tags.length_ms, tags.codec.as_deref()), (true, Some(5000), Some("YOUTUBE")));
        assert_eq!(Resolved::from_json(&serde_json::json!({"title":"x"})), Err("yt-dlp found no audio stream".into()));
    }

    #[test]
    fn a_live_stream_plays_from_now_and_cannot_seek() {
        let Play::Pipe { args, seekable, tags, .. } = Resolved { is_live: true, ..resolved() }.play(&Settings::default()) else { panic!() };
        assert!(!args.iter().any(|a| a == "-ss" || a == "-http_proxy"));
        assert_eq!((seekable, tags.length_ms), (false, None));
    }

    #[test]
    fn a_resolved_video_is_used_again_for_an_hour() {
        let mut c = Cache::default();
        c.put("a", 1_000, resolved());
        assert_eq!(c.get("a", 1_000 + CACHE_TTL_MS - 1), Some(resolved()));
        assert_eq!(c.get("a", 1_000 + CACHE_TTL_MS), None);
        assert_eq!(c.get("b", 1_000), None);
        c.put("b", 1_000 + CACHE_TTL_MS, resolved());
        assert_eq!(c.0.len(), 1, "the expired one is forgotten");
    }
}
