use anyhow::anyhow;
use anyhow::Result;
use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::{
    io::Cursor,
    path::{Path, PathBuf},
};
use tokio::{fs::File, io::AsyncReadExt};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[clap(long, short)]
    working_directory: Option<String>,

    #[clap(long, short)]
    cache_directory: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    Install {},
    ClearCache {},
}

#[derive(Deserialize, Clone)]
struct ComposerPackageSource {
    #[serde(alias = "type")]
    source_type: String,
    url: String,
    reference: String,
}

#[derive(Deserialize)]
struct ComposerPackage {
    name: String,
    version: String,
    source: Option<ComposerPackageSource>,
    dist: Option<ComposerPackageSource>,
    #[serde(alias = "type")]
    package_type: Option<String>,
}

#[derive(Deserialize)]
struct ComposerLock {
    packages: Vec<ComposerPackage>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut cli = Cli::parse();

    if cli.working_directory.is_none() {
        cli.working_directory = Some(
            std::env::current_dir()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string(),
        );
    }

    if cli.cache_directory.is_none() {
        cli.cache_directory = Some(
            dirs::cache_dir()
                .unwrap()
                .join(Path::new("composer-rs"))
                .to_str()
                .unwrap()
                .to_string(),
        );
    }

    match &cli.command {
        Some(Commands::Install {}) => {
            let project_root = cli.working_directory.unwrap();
            let project_root_str = &project_root.as_str();
            let cache_directory = cli.cache_directory.unwrap();
            let cache_directory_str = &cache_directory.as_str();
            return install_from_composer_lock(
                Path::new(&project_root_str),
                Path::new(cache_directory_str),
            )
            .await;
        }
        Some(Commands::ClearCache {}) => {
            println!("Clearing cache");
        }
        None => {
            println!("No command passed");
        }
    }

    Ok(())
}

async fn install_from_composer_lock(
    working_directory: &Path,
    cache_directory: &Path,
) -> Result<()> {
    let mut composer_lock = File::open(working_directory.join(Path::new("composer.lock")))
        .await
        .expect("failed to open composer.lock");

    let mut buffer = Vec::new();
    composer_lock
        .read_to_end(&mut buffer)
        .await
        .expect("failed to read composer.lock");

    let composer_lock: ComposerLock =
        serde_json::from_slice(&buffer).expect("failed to parse composer.lock");

    let cache_archive_directory = cache_directory.join("archives");

        tokio::fs::create_dir_all(cache_archive_directory)
            .await
            .expect("failed to create cache directory");

    let mut handles = Vec::new();

    let client = reqwest::Client::builder()
        .user_agent("composer-rs")
        .build()
        .unwrap();

    for package in composer_lock.packages {
        // Skip meta-packages, these are only virtual and should not be installed
        if package
            .package_type
            .is_some_and(|package_type| package_type == "metapackage")
        {
            continue;
        }

        println!("Installing {} in version {}", package.name, package.version);

        let handle = tokio::spawn(install_package(
            client.clone(),
            package.dist.unwrap_or(package.source.unwrap()),
            cache_directory.join(Path::new("archives")),
            working_directory
                .join(Path::new("vendor"))
                .join(Path::new(package.name.as_str())),
        ));
        handles.push(handle);
    }

    let results = futures::future::join_all(handles).await;

    for result in results {
        match result {
            Ok(Ok(_)) => continue,
            Ok(Err(e)) => return Err(anyhow!("Future failed with error: {}", e)),
            Err(e) => return Err(anyhow!("Future panicked: {}", e)),
        }
    }

    Ok(())
}

async fn install_package(
    client: reqwest::Client,
    source: ComposerPackageSource,
    cache_directory: PathBuf,
    extract: PathBuf,
) -> Result<()> {
    match source.source_type.as_str() {
        "zip" => install_package_from_zip(client, source, cache_directory, extract).await,
        source_type => Err(anyhow!("Unsupported source type: {}", source_type)),
    }
}

async fn install_package_from_zip(
    client: reqwest::Client,
    source: ComposerPackageSource,
    cache_directory: PathBuf,
    extract: PathBuf,
) -> Result<()> {
    let cache_file = cache_directory.join(Path::new(format!("{}.zip", source.reference).as_str()));

    // Check if the file is already cached, using toktio
    if cache_file.exists() {
        let mut file = File::open(&cache_file)
            .await
            .expect("failed to open cached file");

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .await
            .expect("failed to read cached file");

        tokio::fs::create_dir_all(&extract)
            .await
            .expect("failed to create directory");

        zip_extract::extract(Cursor::new(buffer.clone()), &extract, true)
            .expect("failed to extract zip file");
    } else {
        let resp = client
            .get(source.url)
            .send()
            .await
            .expect("failed to download package")
            .error_for_status()
            .expect("invalid repsonse from server");

        let bytes = resp.bytes().await.expect("failed to read response body");

        tokio::fs::create_dir_all(&extract)
            .await
            .expect("failed to create directory");

        zip_extract::extract(Cursor::new(bytes.clone()), &extract, true)
            .expect("failed to extract zip file");

        tokio::fs::write(&cache_file, bytes)
            .await
            .expect("failed to write file");
    }

    Ok(())
}
