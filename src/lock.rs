use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use anyhow::Result;

#[derive(Deserialize, Clone)]
pub struct ComposerPackageSource {
    #[serde(alias = "type")]
    pub source_type: String,
    pub url: String,
    pub reference: String,
}

#[derive(Deserialize, Clone)]
pub struct ComposerAutoload {
    pub files: Option<Vec<String>>,
    #[serde(alias = "psr-0")]
    pub psr0: Option<HashMap<String, String>>,
    #[serde(alias = "psr-4")]
    pub psr4: Option<HashMap<String, String>>,
}

#[derive(Deserialize, Clone)]
pub struct ComposerPackage {
    pub name: String,
    pub version: String,
    pub source: Option<ComposerPackageSource>,
    pub dist: Option<ComposerPackageSource>,
    #[serde(alias = "type")]
    pub package_type: Option<String>,
    pub autoload: Option<ComposerAutoload>
}

#[derive(Deserialize, Clone)]
pub struct ComposerLock {
    pub packages: Vec<ComposerPackage>,
    #[serde(alias = "content-hash")]
    pub content_hash: String,
}

pub async fn load_composer_lock(file_path: PathBuf) -> Result<ComposerLock> {
    let mut composer_lock = File::open(file_path)
        .await
        .expect("failed to open composer.lock");

    let mut buffer = Vec::new();
    composer_lock
        .read_to_end(&mut buffer)
        .await
        .expect("failed to read composer.lock");

    let parsed = serde_json::from_slice(&buffer).expect("failed to parse composer.lock");

    Ok(parsed)
}