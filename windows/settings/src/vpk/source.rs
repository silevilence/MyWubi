use semver::Version;
use serde::Deserialize;
use std::path::Path;
use std::sync::{mpsc::Sender, Arc, OnceLock};
use velopack::bundle::Manifest;
use velopack::sources::{HttpSource, UpdateSource};
use velopack::{Error, VelopackAsset, VelopackAssetFeed};

use super::RELEASES_PAGE_URL;

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
}

impl Release {
    fn includes(&self, name: &str) -> bool {
        self.assets.iter().any(|asset| asset.name == name)
    }

    fn has_feed(&self, channel: &str) -> Result<bool, Error> {
        let scopes: Vec<_> = self
            .assets
            .iter()
            .map(|asset| asset.name.as_str())
            .filter(|name| name.starts_with("release-scope-"))
            .collect();
        let has_feed = self.includes(&format!("releases.{channel}.json"));
        match scopes.as_slice() {
            [] => Ok(has_feed),
            ["release-scope-android.json"] => Ok(false),
            ["release-scope-windows.json" | "release-scope-both.json"] => {
                if channel == "win" && !has_feed {
                    return Err(Error::Other(format!(
                        "{} 缺少 Windows 更新清单",
                        self.tag_name
                    )));
                }
                Ok(has_feed)
            }
            _ => Err(Error::Other("发布范围记录无效".into())),
        }
    }

    fn validate(&self, feed: &VelopackAssetFeed, id: &str) -> Result<(), Error> {
        let version = &self.tag_name[1..];
        let mut has_full = false;
        for asset in &feed.Assets {
            let kind = asset.Type.to_ascii_lowercase();
            let filename = format!("{id}-{version}-{kind}.nupkg");
            if asset.PackageId != id
                || asset.Version != version
                || !matches!(kind.as_str(), "full" | "delta")
                || asset.FileName != filename
                || !self.includes(&filename)
                || asset.Size == 0
                || asset.SHA256.len() != 64
                || !asset.SHA256.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(Error::Other(format!(
                    "{} 的更新包与发布清单不符",
                    self.tag_name
                )));
            }
            has_full |= kind == "full";
        }
        if !has_full {
            return Err(Error::Other("Windows 更新清单缺少完整包".into()));
        }
        Ok(())
    }
}

const PAGE_SIZE: usize = 10;

fn discover(
    channel: &str,
    page: impl Fn(usize) -> Result<Vec<Release>, Error>,
) -> Result<Option<Release>, Error> {
    let mut releases = Vec::new();
    let mut index = 1;
    loop {
        let batch = page(index)?;
        let last_page = batch.len() < PAGE_SIZE;
        for release in batch {
            if release.draft || release.prerelease {
                continue;
            }
            let Some(version) = release
                .tag_name
                .strip_prefix(['v', 'V'])
                .and_then(|v| Version::parse(v).ok())
            else {
                continue;
            };
            if version.pre.is_empty() && version.build.is_empty() {
                releases.push((version, release));
            }
        }
        if last_page {
            break;
        }
        index += 1;
    }
    releases.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, release) in releases {
        if release.has_feed(channel)? {
            return Ok(Some(release));
        }
    }
    Ok(None)
}

#[derive(Clone)]
pub(super) struct WindowsSource {
    pub tag: Arc<OnceLock<String>>,
    api: String,
    downloads: String,
}

impl Default for WindowsSource {
    fn default() -> Self {
        Self {
            tag: Arc::default(),
            api: "https://api.github.com/repos/silevilence/MyWubi/releases".into(),
            downloads: format!("{RELEASES_PAGE_URL}/download"),
        }
    }
}

impl WindowsSource {
    pub(super) fn package_source(&self, tag: &str) -> HttpSource {
        HttpSource::new(format!("{}/{tag}/", self.downloads))
    }
}

impl UpdateSource for WindowsSource {
    fn get_release_feed(
        &self,
        channel: &str,
        app: &Manifest,
        _: &str,
    ) -> Result<VelopackAssetFeed, Error> {
        if std::env::consts::ARCH != "x86_64" {
            return Ok(VelopackAssetFeed { Assets: vec![] });
        }
        let release = discover(channel, |index| {
            let url = format!("{}?per_page={PAGE_SIZE}&page={index}", self.api);
            let text = velopack::download::download_url_as_string_with_headers(
                &url,
                &[("User-Agent", "MyWubi-Updater")],
            )?;
            Ok(serde_json::from_str(&text)?)
        })?;
        let Some(release) = release else {
            return Ok(VelopackAssetFeed { Assets: vec![] });
        };
        let url = format!(
            "{}/{}/releases.{channel}.json",
            self.downloads, release.tag_name
        );
        let feed = serde_json::from_str(&velopack::download::download_url_as_string(&url)?)?;
        release.validate(&feed, &app.id)?;
        self.tag
            .set(release.tag_name)
            .map_err(|_| Error::Other("重复检查更新".into()))?;
        Ok(feed)
    }

    fn download_release_entry(
        &self,
        asset: &VelopackAsset,
        path: &Path,
        progress: Option<Sender<i16>>,
    ) -> Result<(), Error> {
        let tag = self
            .tag
            .get()
            .ok_or_else(|| Error::Other("尚未选定 Windows 发布".into()))?;
        self.package_source(tag)
            .download_release_entry(asset, path, progress)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(names: &[&str]) -> Release {
        Release {
            tag_name: "V1.2.3".into(),
            draft: false,
            prerelease: false,
            assets: names
                .iter()
                .map(|n| Asset {
                    name: n.to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn windows_release_after_eleven_android_releases_is_found() {
        let found = discover("win", |page| {
            Ok(if page == 1 {
                (0..PAGE_SIZE)
                    .map(|_| release(&["release-scope-android.json"]))
                    .collect()
            } else {
                vec![
                    release(&["release-scope-android.json"]),
                    release(&["releases.win.json"]),
                ]
            })
        })
        .unwrap()
        .unwrap();
        assert_eq!(found.tag_name, "V1.2.3");
    }

    #[test]
    fn higher_version_wins_even_if_its_draft_was_created_first() {
        let found = discover("win", |_| {
            Ok(vec![
                release(&["releases.win.json"]),
                Release {
                    tag_name: "v10.2.0".into(),
                    ..release(&["releases.win.json"])
                },
            ])
        })
        .unwrap()
        .unwrap();
        assert_eq!(found.tag_name, "v10.2.0");
    }

    #[test]
    fn broken_windows_feed_or_offline_page_is_an_error() {
        assert!(release(&["release-scope-windows.json"])
            .has_feed("win")
            .is_err());
        assert!(discover("win", |_| Err(Error::Other("offline".into()))).is_err());
        assert!(release(&[])
            .validate(&VelopackAssetFeed { Assets: vec![] }, "MyWubi")
            .is_err());
    }
}

#[cfg(all(test, windows))]
mod upgrade_fixture;
