//! The images a block names for Vulnerabilities (`Block::images`, `[vulnerabilities] images`):
//! what the scan reads of them.

use std::path::{Path, PathBuf};

use anyhow::Result;

/// An image to scan, as its provider names it: its label (`cart-api · prod`, a base image's
/// folder), its tags newest first (the first published one is scanned), the rules of the
/// `.trivyignore` that applies, or why it could not be named. `repo`: the repository it is built
/// from when not the provider's (its CVEs are fixed there); `base`: a base image (Base group).
pub struct ImageTarget {
    pub label: String,
    pub tags: Vec<ImageTag>,
    pub ignore: Option<String>,
    pub error: Option<String>,
    pub repo: Option<String>,
    pub base: bool,
}

/// A tag: what is shown of it (`prod/cart-api:0.2.1`), and where its image may be, in order.
pub struct ImageTag {
    pub tag: String,
    pub shown: String,
    pub at: Vec<ImageRef>,
}

/// An image with its tag; `gcp`: read with a gcloud token.
pub struct ImageRef {
    pub image: String,
    pub gcp: bool,
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
