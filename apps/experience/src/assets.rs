use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::{Mutex, OnceLock, RwLock},
};

use gpui::{AssetSource, SharedString};
use runtime_luau::RevisionAsset;

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

static REVISION_ASSETS: OnceLock<RwLock<HashMap<String, Vec<u8>>>> = OnceLock::new();
type RevisionFont = (String, Vec<u8>);
static REVISION_FONTS: OnceLock<RwLock<Vec<RevisionFont>>> = OnceLock::new();
static LOADED_FONTS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

pub fn install(assets: &[RevisionAsset]) {
    let registry = REVISION_ASSETS.get_or_init(|| RwLock::new(HashMap::new()));
    let mut registry = registry.write().expect("revision asset registry");
    registry.clear();
    let mut fonts = REVISION_FONTS
        .get_or_init(|| RwLock::new(Vec::new()))
        .write()
        .expect("revision font registry");
    fonts.clear();
    for asset in assets {
        registry.insert(asset.path.clone(), asset.bytes.clone());
        if asset.kind == "font" {
            fonts.push((asset.sha256.clone(), asset.bytes.clone()));
        }
    }
}

pub fn install_fonts(window: &mut gpui::Window) {
    let Some(fonts) = REVISION_FONTS.get().and_then(|fonts| fonts.read().ok()) else {
        return;
    };
    let mut loaded = LOADED_FONTS
        .get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        .expect("loaded font registry");
    let pending = fonts
        .iter()
        .filter(|(sha256, _)| !loaded.contains(sha256))
        .cloned()
        .collect::<Vec<_>>();
    if pending.is_empty() {
        return;
    }
    let bytes = pending
        .iter()
        .map(|(_, bytes)| Cow::Owned(bytes.clone()))
        .collect::<Vec<_>>();
    match window.text_system().add_fonts(bytes) {
        Ok(()) => loaded.extend(pending.into_iter().map(|(sha256, _)| sha256)),
        Err(error) => log::warn!("revision_font_load_failed error={error}"),
    }
}

impl AssetSource for SosAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        if path == ALBUM_ASSET {
            return Ok(Some(Cow::Borrowed(ALBUM_ORBIT.as_bytes())));
        }
        Ok(REVISION_ASSETS
            .get()
            .and_then(|assets| assets.read().ok()?.get(path).cloned())
            .map(Cow::Owned))
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        let mut listed = if path == "sos" {
            vec![SharedString::from(ALBUM_ASSET)]
        } else {
            Vec::new()
        };
        if let Some(assets) = REVISION_ASSETS.get().and_then(|assets| assets.read().ok()) {
            listed.extend(
                assets
                    .keys()
                    .filter(|asset| asset.starts_with(path))
                    .cloned()
                    .map(SharedString::from),
            );
        }
        Ok(listed)
    }
}
