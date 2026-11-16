//! The images a block names for Vulnerabilities (`Block::images`, `[vulnerabilities] images`):
//! what the scan reads of them.

use std::path::{Path, PathBuf};

use anyhow::Result;

/// An image to scan, as its provider names it: its label (`cart-api · prod`, a base image's
/// folder), its tags newest first (the first published one is scanned), the rules of the
/// `.trivyignore` that applies, or why it could not be named. `repo`: the repository it is built
/// from when not the provider's (its CVEs are fixed there); `base`: a base image (Base group).
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
pub struct ImageTarget {
    pub label: String,
    pub tags: Vec<ImageTag>,
    pub ignore: Option<String>,
    pub error: Option<String>,
    pub repo: Option<String>,
    pub base: bool,
}

/// A tag: what is shown of it (`prod/cart-api:0.2.1`), and where its image may be, in order.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
pub struct ImageTag {
    pub tag: String,
    pub shown: String,
    pub at: Vec<ImageRef>,
}

/// An image with its tag; `gcp`: read with a gcloud token.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
pub struct ImageRef {
    pub image: String,
    pub gcp: bool,
}

impl ImageTarget {
    /// As a provider named it, what Prism goes by: a reference that reads as an option
    /// (`-…`) dropped, and a gcloud token only for an image on Google's registries, whatever the
    /// provider (a module) said: the token goes to the image's host.
    pub fn checked(mut self) -> Self {
        for tag in &mut self.tags {
            tag.at.retain(|at| !at.image.is_empty() && !at.image.starts_with('-'));
            for at in &mut tag.at {
                at.gcp = at.gcp && on_gcp(&at.image);
            }
        }
        self
    }
}

/// Whether `image` is on Google's registries (Artifact Registry, Container Registry): its host,
/// before the first `/`.
pub fn on_gcp(image: &str) -> bool {
    let Some((host, _)) = image.split_once('/') else { return false };
    let host = host.to_ascii_lowercase();
    host.ends_with("-docker.pkg.dev") || host == "gcr.io" || host.ends_with(".gcr.io")
}

/// A repository with a local clone (`[[repos]]` with a path), as a provider may read it.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalRepo {
    pub name: String,
    pub clone: PathBuf,
}

/// A source of the images to scan (`[vulnerabilities] images`): which images and where they are,
/// its own settings say. `repo` and `project`: the repository of the block providing them;
/// `local`: every repository with a local clone.
pub trait ImageProvider: Send + Sync {
    fn images<'a>(&'a self, repo: &'a Path, project: &'a str, local: &'a [LocalRepo]) -> futures::future::BoxFuture<'a, Result<Vec<ImageTarget>>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gcloud_token_only_for_google_registries() {
        for (image, gcp) in [
            ("europe-west1-docker.pkg.dev/p/r/app:1", true),
            ("gcr.io/p/app:1", true),
            ("eu.gcr.io/p/app:1", true),
            ("attacker.example/p/app:1", false),
            ("pkg.dev.attacker.example/app:1", false),
            ("evil-docker.pkg.dev.example/app:1", false),
            ("gcr.io", false),
            ("app:1", false),
        ] {
            assert_eq!(on_gcp(image), gcp, "{image}");
        }
        let at = |image: &str| ImageRef { image: image.into(), gcp: true };
        let target = ImageTarget { tags: vec![ImageTag { at: vec![at("attacker.example/x:1"), at("--config=/etc/x"), at("gcr.io/p/x:1")], ..Default::default() }], ..Default::default() }.checked();
        let at = &target.tags[0].at;
        assert_eq!(at.iter().map(|a| (a.image.as_str(), a.gcp)).collect::<Vec<_>>(), [("attacker.example/x:1", false), ("gcr.io/p/x:1", true)]);
    }
}
