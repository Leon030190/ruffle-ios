//! Web & URL SWF Downloader for Ruffle iOS
//!
//! Enables downloading and adding Flash movies (.swf and .ruf) directly
//! from web URLs, HTTP/HTTPS sources, and online archives without needing
//! file transfer or Xcode.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use objc2::rc::Retained;
use objc2_foundation::{
    ns_string, NSData, NSFileManager, NSSearchPathDirectory, NSSearchPathDomainMask, NSString, NSURL,
};
use url::Url;

use crate::storage;

/// Curated sample Flash demos and games for instant testing
pub const SAMPLE_MOVIES: &[(&str, &str, &str)] = &[
    (
        "Alien Hominid (Classic Demo)",
        "https://uploads.ungrounded.net/59000/59593_alien_hominid.swf",
        "Classic legendary Flash shooter by Tom Fulp & Dan Paladin",
    ),
    (
        "Ruffle Animated Logo",
        "https://ruffle.rs/demo/logo-anim.swf",
        "Official Ruffle vector animation and interactive demo",
    ),
    (
        "ActionScript 2 Timeline Demo",
        "https://ruffle.rs/demo/tests/swfs/as2/timeline.swf",
        "Interactive timeline control and shape morphing demo",
    ),
    (
        "ActionScript 3 Vector Graphics",
        "https://ruffle.rs/demo/tests/swfs/as3/simple_graphics.swf",
        "High-performance AS3 vector rendering test",
    ),
];

/// Get the local iOS Documents directory for downloaded Flash files
pub fn get_downloads_dir() -> PathBuf {
    let file_manager = unsafe { NSFileManager::defaultManager() };
    let urls = unsafe {
        file_manager.URLsForDirectory_inDomains(
            NSSearchPathDirectory::DocumentDirectory,
            NSSearchPathDomainMask::UserDomainMask,
        )
    };

    if let Some(first_url) = urls.first() {
        if let Some(path_str) = first_url.path() {
            let path = PathBuf::from(path_str.to_string());
            let downloads = path.join("Downloads");
            let _ = std::fs::create_dir_all(&downloads);
            return downloads;
        }
    }

    // Fallback: standard tmp directory
    std::env::temp_dir()
}

/// Download an SWF or Ruffle bundle from a web URL and save it to Documents
pub fn download_swf_from_url(url_str: &str) -> Result<Retained<NSURL>, String> {
    let parsed_url = Url::parse(url_str).map_err(|e| format!("Invalid URL: {e}"))?;

    // Determine filename
    let file_stem = Path::new(parsed_url.path())
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("downloaded_movie.swf");

    let filename = if file_stem.ends_with(".swf") || file_stem.ends_with(".ruf") {
        file_stem.to_string()
    } else {
        format!("{file_stem}.swf")
    };

    let target_path = get_downloads_dir().join(&filename);

    tracing::info!("Downloading Flash movie from {url_str} to {target_path:?}");

    // Download content using Foundation NSURL and NSData
    let ns_url_str = NSString::from_str(url_str);
    let ns_url = unsafe { NSURL::URLWithString(&ns_url_str) }
        .ok_or_else(|| "Failed to create NSURL".to_string())?;

    let data = unsafe { NSData::dataWithContentsOfURL(&ns_url) }
        .ok_or_else(|| "Failed to download data from URL (network error or timeout)".to_string())?;

    let bytes = data.to_vec();
    if bytes.is_empty() {
        return Err("Downloaded file is empty".to_string());
    }

    // Verify Flash SWF / Ruffle magic header (FWS, CWS, ZWS, or RUF)
    let is_swf = bytes.starts_with(b"FWS")
        || bytes.starts_with(b"CWS")
        || bytes.starts_with(b"ZWS");
    let is_ruf = bytes.starts_with(b"RUF");

    if !is_swf && !is_ruf {
        tracing::warn!("File header does not match standard SWF (FWS/CWS/ZWS), proceeding anyway");
    }

    // Save to disk
    let mut file = File::create(&target_path)
        .map_err(|e| format!("Failed to create local file: {e}"))?;
    file.write_all(&bytes)
        .map_err(|e| format!("Failed to write data to file: {e}"))?;

    // Register into Core Data storage
    let path_str = NSString::from_str(&target_path.to_string_lossy());
    let file_url = unsafe { NSURL::fileURLWithPath(&path_str) };

    if storage::movie_from_url(&file_url).is_none() {
        storage::add_movie(&file_url);
        tracing::info!("Successfully added downloaded movie to library: {filename}");
    }

    Ok(file_url)
}

/// Add an online SWF link directly as a streamable movie without waiting for download
pub fn add_stream_url(url_str: &str) -> Result<Retained<NSURL>, String> {
    let _parsed = Url::parse(url_str).map_err(|e| format!("Invalid URL: {e}"))?;

    let ns_url_str = NSString::from_str(url_str);
    let ns_url = unsafe { NSURL::URLWithString(&ns_url_str) }
        .ok_or_else(|| "Failed to create NSURL".to_string())?;

    if storage::movie_from_url(&ns_url).is_none() {
        storage::add_movie(&ns_url);
        tracing::info!("Added online streaming movie: {url_str}");
    }

    Ok(ns_url)
}

/// Quick-add one of the curated sample games/demos
pub fn add_sample_demo(index: usize) -> Result<Retained<NSURL>, String> {
    let demo = SAMPLE_MOVIES
        .get(index)
        .ok_or_else(|| "Sample index out of range".to_string())?;

    download_swf_from_url(demo.1)
}
