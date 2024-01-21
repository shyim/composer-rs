use anyhow::{Ok, Result};
use askama::Template;
use std::{path::PathBuf, collections::HashMap};

use crate::lock;

#[derive(Template)]
#[template(path = "autoload.html")]
struct ComposerAutoload {
    hash: String,
}


#[derive(Template)]
#[template(path = "autoload_real.html")]
struct ComposerRealTemplate {
    hash: String
}

#[derive(Template)]
#[template(path = "autoload_static.html")]
struct ComposerStaticTemplate {
    hash: String,
    files: HashMap<String, String>,
    psr0: HashMap<String, HashMap<String, HashMap<String, usize>>>,
    psr4: HashMap<String, Vec<String>>,
    psr4_prefix: HashMap<String, HashMap<String, usize>>,
}

#[derive(Template)]
#[template(path = "Classloader.html")]
struct ClassloaderTemplate {}

pub async fn generate_composer_autoload(
    lock: lock::ComposerLock,
    vendor_directory: PathBuf,
) -> Result<()> {
    let composer_directory = vendor_directory.join("composer");
    if !composer_directory.exists() {
        tokio::fs::create_dir_all(&composer_directory)
            .await
            .expect("Failed to create composer directory");
    }

    generate_main_autoload(lock.clone(), vendor_directory.clone())
        .await
        .expect("Failed to generate vendor/autoload.php file");

    generate_composer_real(lock.clone(), vendor_directory.clone())
        .await
        .expect("Failed to generate autoload_real.php file");

    generate_composer_static(lock.clone(), vendor_directory.clone())
        .await
        .expect("Failed to generate autoload_static.php file");

    generate_composer_classloader(vendor_directory.clone())
        .await
        .expect("Failed to generate ClassLoader.php file");

    Ok(())
}

async fn generate_main_autoload(lock: lock::ComposerLock, vendor_directory: PathBuf) -> Result<()> {
    let template = ComposerAutoload {
        hash: lock.content_hash,
    };

    tokio::fs::write(
        vendor_directory.join("autoload.php"),
        template.render().unwrap(),
    )
    .await
    .expect("Failed to write composer autoload file");

    Ok(())
}

async fn generate_composer_real(lock: lock::ComposerLock, vendor_directory: PathBuf) -> Result<()> {
    let template = ComposerRealTemplate {
        hash: lock.content_hash
    };
    let rendered = template.render().unwrap();

    let composer_directory = vendor_directory.join("composer");
    let classmap_file = composer_directory.join("autoload_real.php");
    tokio::fs::write(&classmap_file, rendered)
        .await
        .expect("Failed to write composer classmap");

    Ok(())
}

async fn generate_composer_static(lock: lock::ComposerLock, vendor_directory: PathBuf) -> Result<()> {
    let mut files = HashMap::new();
    let mut psr0: HashMap<String, HashMap<String, HashMap<String, usize>>> = HashMap::new();
    let mut psr4: HashMap<String, Vec<String>> = HashMap::new();
    let mut psr4_prefix: HashMap<String, HashMap<String, usize>> = HashMap::new();

    for package in lock.packages {
        if package.autoload.is_some() {
            let autoload = package.autoload.unwrap();

            if autoload.files.is_some() {
                for file in autoload.files.unwrap() {
                    files.insert(xxhash_rust::xxh3::xxh3_64(file.as_bytes()).to_string(), format!("{}/{}", package.name, file));
                }
            }

            if autoload.psr0.is_some() {
                for (namespace, path) in autoload.psr0.unwrap() {
                    let path_to_directory = format!("{}/{}", package.name, path);
                    let first_letter = namespace.chars().next().unwrap();

                    if psr0.contains_key(&first_letter.to_string()) {
                        let first_letter_map = psr0.get_mut(&first_letter.to_string()).unwrap();

                        if first_letter_map.contains_key(&namespace) {
                            first_letter_map.get_mut(&namespace).unwrap().insert(path_to_directory, 1);
                        } else {
                            let mut paths = HashMap::new();
                            paths.insert(path_to_directory, 1);
                            first_letter_map.insert(namespace.clone(), paths);
                        }
                    } else {
                        let mut prefix = HashMap::new();

                        let mut paths = HashMap::new();
                        paths.insert(path_to_directory, 1);

                        prefix.insert(namespace.clone(), paths);
                        psr0.insert(first_letter.to_string(), prefix);
                    }
                }
            }

            if autoload.psr4.is_some() {
                for (namespace, path) in autoload.psr4.unwrap() {
                    let path_to_directory = format!("{}/{}", package.name, path);
                    let first_letter = namespace.chars().next().unwrap();

                    if psr4.contains_key(&namespace) {
                        psr4.get_mut(&namespace).unwrap().push(path_to_directory);
                    } else {
                        psr4.insert(namespace.clone(), vec![path_to_directory]);
                    }

                    if psr4_prefix.contains_key(&first_letter.to_string()) {
                        psr4_prefix.get_mut(&first_letter.to_string()).unwrap().insert(namespace.clone(), namespace.len());
                    } else {
                        let mut prefix = HashMap::new();
                        prefix.insert(namespace.clone(), namespace.len());
                        psr4_prefix.insert(first_letter.to_string(), prefix);
                    }
                }
            }
        }
    }

    let template = ComposerStaticTemplate {
        hash: lock.content_hash,
        files: files,
        psr0: psr0,
        psr4: psr4,
        psr4_prefix: psr4_prefix,
    };
    let rendered = template.render().unwrap();

    let composer_directory = vendor_directory.join("composer");
    let classmap_file = composer_directory.join("autoload_static.php");
    tokio::fs::write(&classmap_file, rendered)
        .await
        .expect("Failed to write composer classmap");

    Ok(())
}

async fn generate_composer_classloader(vendor_directory: PathBuf) -> Result<()> {
    let template = ClassloaderTemplate {};
    let rendered = template.render().unwrap();

    let composer_directory = vendor_directory.join("composer");
    let classmap_file = composer_directory.join("ClassLoader.php");
    tokio::fs::write(&classmap_file, rendered)
        .await
        .expect("Failed to write composer classmap");

    Ok(())
}

mod filters {
    pub fn php_escape<T: std::fmt::Display>(s: T) -> ::askama::Result<String> {
        let s = s.to_string();
        Ok(s.replace("\\", "\\\\"))
    }
}
