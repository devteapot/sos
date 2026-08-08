use std::borrow::Cow;

use gpui::{AssetSource, SharedString};

pub const ALBUM_ASSET: &str = "sos/album-orbit.svg";

const ALBUM_ORBIT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
  <defs>
    <linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#f5bb55"/>
      <stop offset="0.52" stop-color="#e76f51"/>
      <stop offset="1" stop-color="#7148d8"/>
    </linearGradient>
  </defs>
  <rect width="128" height="128" rx="28" fill="#16141d"/>
  <circle cx="64" cy="64" r="43" fill="none" stroke="url(#g)" stroke-width="10"/>
  <circle cx="64" cy="64" r="12" fill="#f5efe4"/>
  <path d="M64 21a43 43 0 0 1 36 20" fill="none" stroke="#f5efe4" stroke-width="4" stroke-linecap="round"/>
</svg>"##;

pub struct SosAssets;

impl AssetSource for SosAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        Ok(match path {
            ALBUM_ASSET => Some(Cow::Borrowed(ALBUM_ORBIT.as_bytes())),
            _ => None,
        })
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(if path == "sos" {
            vec![SharedString::from(ALBUM_ASSET)]
        } else {
            Vec::new()
        })
    }
}
