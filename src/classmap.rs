use anyhow::Result;
use async_walkdir::{Filtering, WalkDir};
use futures_lite::stream::StreamExt;
use php_parser_rs::parser::ast::{Statement, namespaces::NamespaceStatement};
use std::{path::{Path, PathBuf}, collections::HashMap};

pub async fn generate_classmap(
    package_directory: PathBuf,
    class_map_directory: String,
    exclude_directories: Vec<String>,
) -> Result<HashMap<String, String>> {
    let mut class_to_files = HashMap::new();

    let scoped_excludes = exclude_directories
        .iter()
        .map(|d| package_directory.join(d.as_str()))
        .collect::<Vec<PathBuf>>();

    let scan_directory = package_directory.join(Path::new(&class_map_directory));

    if !scan_directory.exists() {
        return Ok(class_to_files);
    }

    if scan_directory.is_file() {
        let read_file = scan_directory.clone();
        let relative_path = {
            let path = scan_directory.clone();
            path.strip_prefix(&package_directory).unwrap().to_str().unwrap().to_owned()
        };
        let content = tokio::fs::read(read_file).await.expect("foo");

        let parsed = php_parser_rs::parse(content.as_slice());

        match parsed {
            Ok(parsed) => {
                for name in get_classes_of_statements(parsed, "".to_string()) {
                    class_to_files.insert(name, relative_path.clone());
                }
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }

        return Ok(class_to_files);
    }

    let mut entries = WalkDir::new(scan_directory).filter(
        |entry| async move {
            let dir_path = entry.path();
            let path = dir_path.to_str().unwrap();
            if let Some(true) = dir_path
                .file_name()
                .map(|f| f.to_string_lossy().starts_with('.'))
            {
                return Filtering::IgnoreDir;
            }

            // if scoped_excludes.iter().any(|d| dir_path.starts_with(d)) {
            //     return Filtering::IgnoreDir;
            // }

            // PHP files are not inside a node_modules
            if entry.path().ends_with("node_modules") {
                return Filtering::IgnoreDir;
            }

            if !path.ends_with(".php") && !path.ends_with(".inc") {
                return Filtering::Ignore;
            }

            Filtering::Continue
        },
    );

    loop {
        match entries.next().await {
            Some(Ok(entry)) => {
                let relative_path = {
                    let path = entry.path();
                    path.strip_prefix(&package_directory).unwrap().to_str().unwrap().to_owned()
                };
                let content = tokio::fs::read(entry.path()).await.expect("foo");

                let parsed = php_parser_rs::parse(content.as_slice());

                match parsed {
                    Ok(parsed) => {
                        for name in get_classes_of_statements(parsed, "".to_string()) {
                            class_to_files.insert(name, relative_path.clone());
                        }
                    }
                    Err(e) => {
                        println!("Error: {}", e);
                    }
                }
            }
            Some(Err(e)) => {
                return Err(anyhow::anyhow!("Error: {}", e));
            }
            None => break,
        }
    }

    Ok(class_to_files)
}

fn get_classes_of_statements(statements: Vec<Statement>, prefix: String) -> Vec<String> {
    let mut classes = vec![];
    for stmt in statements {
        match stmt {
            Statement::Class(c) => {
                classes.push(format!("{}{}", prefix, c.name));
            }
            Statement::Namespace(n) => {
                match n {
                    NamespaceStatement::Braced(b) => {
                        let mut new_prefix = prefix.clone();
                        if let Some(name) = b.name {
                            new_prefix.push_str(std::str::from_utf8(&name.value.bytes).unwrap());
                            new_prefix.push_str("\\");
                        }
                        classes.append(&mut get_classes_of_statements(b.body.statements, new_prefix));
                    },
                    NamespaceStatement::Unbraced(u) => {
                        let mut new_prefix = prefix.clone();
                        new_prefix.push_str(std::str::from_utf8(&u.name.value.bytes).unwrap());
                        new_prefix.push_str("\\");
                        classes.append(&mut get_classes_of_statements(u.statements, new_prefix));
                    },
                }
            }
            _ => {}
        }
    }
    classes
}