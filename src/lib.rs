//! YouTube for Popamp: a source plug-in that plays videos, playlists and
//! channels. yt-dlp finds the audio stream, ffmpeg decodes it, and the
//! player plays what ffmpeg writes. The calls are described in
//! [popamp-plugin-api](https://github.com/popamp/popamp-plugin-api).

pub mod youtube;

#[cfg(target_arch = "wasm32")]
mod plugin {
    use crate::youtube::{self, Cache, Resolved, Settings};
    use extism_pdk::*;
    use popamp_plugin_api::{ExecOutput, ExecRequest, Play, Tags, Track, Version, VERSION};
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    #[host_fn]
    extern "ExtismHost" {
        fn popamp_exec(req: Json<ExecRequest>) -> Json<ExecOutput>;
        fn popamp_now() -> Json<u64>;
    }

    thread_local! {
        static CACHE: RefCell<Cache> = RefCell::default();
    }

    fn settings() -> Result<Settings, Error> {
        Ok(Settings { format: config::get("format")?.unwrap_or_default() })
    }

    fn ytdlp(command: &str, vars: BTreeMap<String, String>) -> Result<serde_json::Value, Error> {
        let out = unsafe { popamp_exec(Json(ExecRequest { command: command.into(), vars }))? }.0;
        youtube::ytdlp_json(out).map_err(Error::msg)
    }

    fn resolve(url: &str, s: &Settings) -> Result<Resolved, Error> {
        let now = unsafe { popamp_now()? }.0;
        if let Some(r) = CACHE.with_borrow(|c| c.get(url, now)) {
            return Ok(r);
        }
        let r = Resolved::from_json(&ytdlp("resolve", s.resolve_vars(url))?).map_err(Error::msg)?;
        CACHE.with_borrow_mut(|c| c.put(url, now, r.clone()));
        Ok(r)
    }

    #[plugin_fn]
    pub fn api_version(_: String) -> FnResult<Json<Version>> {
        Ok(Json(VERSION))
    }

    #[plugin_fn]
    pub fn expand(Json(location): Json<String>) -> FnResult<Json<Option<Vec<Track>>>> {
        if youtube::is_single_video(&location) {
            return Ok(Json(None));
        }
        Ok(Json(Some(youtube::parse_listing(&ytdlp("list", youtube::list_vars(&location))?))))
    }

    #[plugin_fn]
    pub fn describe(Json(location): Json<String>) -> FnResult<Json<Option<Tags>>> {
        Ok(Json(Some(resolve(&location, &settings()?)?.tags)))
    }

    #[plugin_fn]
    pub fn open(Json(location): Json<String>) -> FnResult<Json<Play>> {
        Ok(Json(resolve(&location, &settings()?)?.play()))
    }
}
