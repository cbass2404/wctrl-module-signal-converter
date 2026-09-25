//! Whether a different release is out, for the banner at the foot of the window.
//!
//! Asked of GitHub once per start. Any failure (offline, a proxy in the way,
//! GitHub down, the rate limit) means no banner: the editor works the same
//! without a network, and a check that cannot answer has nothing to say.
//!
//! The window never opens a URL itself. It asks for "the update" and the
//! backend opens the release page it found, built from a tag it has checked,
//! so nothing on the page can steer it to any other address.

use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};

const REPO: &str = "cbass2404/wctrl-module-signal-converter";

/// The fields of GitHub's release listing this reads.
#[derive(Debug, Deserialize)]
pub struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    /// ISO 8601, so it orders as text. Absent on a draft.
    #[serde(default)]
    published_at: Option<String>,
}

/// A release other than the one running.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Update {
    /// `VERSION.md` as this build carries it.
    pub current: String,
    /// The newest release's tag, less its `v`.
    pub latest: String,
    #[serde(skip)]
    url: String,
}

/// What the last check found, for `open_update`.
#[derive(Default)]
pub struct Found(Mutex<Option<Update>>);

/// The newest published release a user of `current` should be offered.
///
/// While `current` is a pre-release, pre-releases count, or an alpha user would
/// never hear of the next alpha. A stable user is offered only stable ones.
/// Only `v*` tags are releases of the program: the repository also holds the
/// pinned DCS-BIOS nightly as a release of its own.
fn newest<'a>(current: &str, releases: &'a [Release]) -> Option<&'a Release> {
    let stable = !current.contains('-');
    releases
        .iter()
        .filter(|r| !r.draft && r.tag_name.starts_with('v') && !(stable && r.prerelease))
        .filter(|r| r.published_at.is_some())
        .max_by(|a, b| a.published_at.cmp(&b.published_at))
}

/// A tag safe to put in a URL: what `tools/release.cmd` makes and no more.
fn plain_tag(tag: &str) -> bool {
    tag.len() <= 64 && tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

/// The release to point at, or none when the newest one is the one running.
pub fn check(current: &str, releases: &[Release]) -> Option<Update> {
    let release = newest(current, releases)?;
    let latest = release.tag_name.strip_prefix('v')?;
    if latest == current || !plain_tag(&release.tag_name) {
        return None;
    }
    Some(Update {
        current: current.to_string(),
        latest: latest.to_string(),
        url: format!("https://github.com/{REPO}/releases/tag/{}", release.tag_name),
    })
}

async fn fetch() -> reqwest::Result<Vec<Release>> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        // GitHub refuses API calls without one.
        .user_agent(concat!("dcs-signal-converter/", env!("CARGO_PKG_VERSION")))
        .build()?
        .get(format!("https://api.github.com/repos/{REPO}/releases?per_page=30"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
}

/// A release other than this one, or none. Never an error: see the module note.
#[tauri::command]
pub async fn update_check(found: tauri::State<'_, Found>) -> Result<Option<Update>, String> {
    let update = match fetch().await {
        Ok(releases) => check(dsc_config::version(), &releases),
        Err(_) => None,
    };
    if let Ok(mut slot) = found.0.lock() {
        *slot = update.clone();
    }
    Ok(update)
}

/// Opens the release the last check found in the default browser.
#[tauri::command]
pub fn open_update(found: tauri::State<'_, Found>) -> Result<(), String> {
    let url = found
        .0
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().map(|u| u.url.clone()))
        .ok_or("no update has been found")?;
    std::process::Command::new("explorer")
        .arg(&url)
        .spawn()
        .map_err(|e| format!("opening {url}: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, prerelease: bool, published: Option<&str>) -> Release {
        Release {
            tag_name: tag.into(),
            draft: published.is_none(),
            prerelease,
            published_at: published.map(Into::into),
        }
    }

    #[test]
    fn the_running_release_is_no_update() {
        let list = [release("v1.0.0-alpha.001", true, Some("2026-09-20T00:00:00Z"))];
        assert_eq!(check("1.0.0-alpha.001", &list), None);
    }

    #[test]
    fn a_different_release_is_offered_with_its_page() {
        let list = [
            release("v1.0.0-alpha.001", true, Some("2026-09-20T00:00:00Z")),
            release("v1.0.0-alpha.002", true, Some("2026-10-01T00:00:00Z")),
        ];
        let update = check("1.0.0-alpha.001", &list).expect("an update");
        assert_eq!(update.latest, "1.0.0-alpha.002");
        assert_eq!(update.url, format!("https://github.com/{REPO}/releases/tag/v1.0.0-alpha.002"));
    }

    #[test]
    fn alpha_to_beta_is_offered_though_the_number_drops() {
        // The newest is chosen by date, not by comparing versions.
        let list = [
            release("v1.0.0-alpha.010", true, Some("2026-09-24T00:00:00Z")),
            release("v1.0.0-beta.001", true, Some("2026-10-01T00:00:00Z")),
        ];
        assert_eq!(check("1.0.0-alpha.010", &list).unwrap().latest, "1.0.0-beta.001");
        assert_eq!(check("1.0.0-beta.001", &list), None);
    }

    #[test]
    fn drafts_and_the_dcs_bios_pin_are_not_releases() {
        let list = [
            release("v1.0.0-alpha.001", true, Some("2026-09-20T00:00:00Z")),
            release("v1.0.0-alpha.002", true, None),
            release("dcs-bios-2026.10.01-nightly", true, Some("2026-10-02T00:00:00Z")),
        ];
        assert_eq!(check("1.0.0-alpha.001", &list), None);
    }

    #[test]
    fn a_stable_user_is_not_offered_a_pre_release() {
        let list = [
            release("v1.0.0", false, Some("2026-11-01T00:00:00Z")),
            release("v1.1.0-alpha.001", true, Some("2026-12-01T00:00:00Z")),
        ];
        assert_eq!(check("1.0.0", &list), None);
        assert_eq!(check("1.0.0-alpha.009", &list).unwrap().latest, "1.1.0-alpha.001");
    }

    #[test]
    fn a_tag_that_would_bend_the_url_is_ignored() {
        let list = [release("v2/../../evil", false, Some("2026-11-01T00:00:00Z"))];
        assert_eq!(check("1.0.0", &list), None);
    }

    #[test]
    fn nothing_published_is_no_update() {
        assert_eq!(check("1.0.0-alpha.001", &[]), None);
    }
}
