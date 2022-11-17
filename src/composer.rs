use crate::{packagist, Url};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{prelude::*, BufReader};

#[derive(Debug, PartialEq, Deserialize)]
pub struct ComposerFile {
    pub path: String,
    pub dependencies: Vec<ComposerDependency>,
    pub dev_dependencies: Vec<ComposerDependency>,
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct ComposerLock {
    pub versions: HashMap<String, InstalledPackage>,
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
pub struct ComposerDependency {
    pub name: String,
    pub version: String,
    pub line: u32,
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
pub struct InstalledPackage {
    pub name: String,
    pub version: String,
}

pub fn parse_json_file(filepath: Url) -> Option<ComposerFile> {
    let file = Url::parse(&filepath.to_string()).unwrap();
    if file.path().ends_with("composer.json") == false {
        return None;
    }

    let mut composer_file = ComposerFile {
        path: filepath.to_string(),
        dependencies: Vec::new(),
        dev_dependencies: Vec::new(),
    };

    let contents = fs::read_to_string(file.path()).expect("Error while reading the composer file");

    let parsed_contents: Value = match serde_json::from_str(&contents) {
        Ok(v) => v,
        _ => Value::Null,
    };

    if parsed_contents.is_null() {
        return None;
    }

    let parsed_contents_object = parsed_contents.as_object().unwrap();
    // Parse line numbers.
    let require_buffer = parse_by_require_line(file.path());

    // Get dependencies.
    if parsed_contents_object.contains_key("require") {
        let dependencies = &parsed_contents_object["require"];
        let dep_obj = dependencies.as_object().unwrap();
        for (name, version) in dep_obj {
            let line = require_buffer
                .get(&name.to_string())
                .expect("Can't unwrap a line num")
                - 1;

            let composer_dependency = ComposerDependency {
                name: name.to_string(),
                version: version.to_string(),
                line,
            };

            composer_file.dependencies.push(composer_dependency);
        }
    }

    // Get dev dependencies.
    let require_dev_buffer = parse_by_require_dev_line(file.path());
    if parsed_contents_object.contains_key("require-dev") {
        let dependencies = &parsed_contents_object["require-dev"];
        let dep_obj = dependencies.as_object().unwrap();
        for (name, version) in dep_obj {
            let line = require_dev_buffer
                .get(&name.to_string())
                .expect("Can't unwrap a line num")
                - 1;

            let composer_dependency = ComposerDependency {
                name: name.to_string(),
                version: version.to_string(),
                line,
            };

            composer_file.dev_dependencies.push(composer_dependency);
        }
    }

    Some(composer_file)
}

pub fn parse_lock_file(composer_file: &ComposerFile) -> Option<ComposerLock> {
    let composer_lock_path = composer_file.path.replace("composer.json", "composer.lock");
    let file = Url::parse(&composer_lock_path);

    match file {
        Ok(file_url) => {
            let mut composer_lock = ComposerLock {
                versions: HashMap::new(),
            };

            let contents =
                fs::read_to_string(file_url.path()).expect("Error while reading the lock file");

            let parsed_contents: Value = match serde_json::from_str(&contents) {
                Ok(v) => v,
                _ => Value::Null,
            };

            if parsed_contents.is_null() {
                return None;
            }

            let parsed_contents_object = parsed_contents.as_object().unwrap();
            if parsed_contents_object.contains_key("packages") {
                let packages = parsed_contents_object.get("packages");
                for item in packages.unwrap().as_array().unwrap() {
                    let package = item.as_object();
                    match package {
                        Some(item) => {
                            // @todo handle unwrap.
                            let name = item
                                .get("name")
                                .unwrap()
                                .to_string()
                                .replace("\"", "")
                                .replace("\'", "");

                            let version = item
                                .get("version")
                                .unwrap()
                                .to_string()
                                .replace("\"", "")
                                .replace("v", "")
                                .replace("\'", "");

                            let installed_package = InstalledPackage {
                                name: name.clone(),
                                version,
                            };

                            composer_lock.versions.insert(name, installed_package);
                        }
                        None => {}
                    }
                }
            }

            Some(composer_lock)
        }
        // @todo add logging.
        Err(_error) => None,
    }
}

fn parse_by_require_line(filepath: &str) -> HashMap<String, u32> {
    let file = File::open(filepath);
    let reader = BufReader::new(file.expect("Can't retrieve a file"));
    let mut buffer: HashMap<String, u32> = HashMap::new();

    let mut line_num = 1;
    let mut require_block_start = 0;
    let require_block_end = 0;
    for line in reader.lines() {
        if require_block_end > 0 {
            break;
        }

        let line_text = line.as_ref().expect("Can't unwrap a line text.");
        if line_text.contains("\"require\": {") {
            require_block_start = line_num;
        }

        if require_block_start > 0 && line_num > require_block_start {
            if line_text.contains("},") {
                break;
            }

            let dependency_name_delimiter = line_text
                .find(":")
                .expect("Can't find the dependency name delimiter");
            let dependency_name = &line_text[..dependency_name_delimiter].replace(" ", "");
            buffer.insert(dependency_name.to_string().replace("\"", ""), line_num);
        }

        line_num += 1;
    }

    buffer
}

fn parse_by_require_dev_line(filepath: &str) -> HashMap<String, u32> {
    let file = File::open(filepath);
    let reader = BufReader::new(file.expect("Can't retrieve a file"));
    let mut buffer: HashMap<String, u32> = HashMap::new();

    let mut line_num = 1;
    let mut require_block_start = 0;
    let require_block_end = 0;
    for line in reader.lines() {
        if require_block_end > 0 {
            break;
        }

        let line_text = line.as_ref().expect("Can't unwrap a line text.");
        if line_text.contains("\"require-dev\": {") {
            require_block_start = line_num;
        }

        if require_block_start > 0 && line_num > require_block_start {
            if line_text.contains("},") {
                break;
            }

            let dependency_name_delimiter = line_text
                .find(":")
                .expect("Can't find the dependency name delimiter");
            let dependency_name = &line_text[..dependency_name_delimiter].replace(" ", "");
            buffer.insert(dependency_name.to_string().replace("\"", ""), line_num);
        }

        line_num += 1;
    }

    buffer
}

#[cfg(test)]
mod tests {
    use reqwest::Url;

    use crate::composer::{parse_json_file, parse_lock_file};

    #[test]
    fn it_can_parse_a_valid_composer_json_file() {
        let root_path = env!("CARGO_MANIFEST_DIR");
        let test_file = Url::from_file_path(format!("{}/tests/composer.json", root_path));
        let parsed_contents = parse_json_file(test_file.unwrap());

        assert_ne!(None, parsed_contents);
    }

    #[test]
    fn it_can_parse_required_dependencies() {
        let root_path = env!("CARGO_MANIFEST_DIR");
        let test_file = Url::from_file_path(format!("{}/tests/composer.json", root_path));
        let parsed_contents = parse_json_file(test_file.unwrap()).unwrap();

        assert_eq!(2, parsed_contents.dependencies.len());
    }

    #[test]
    fn it_can_parse_required_dev_dependencies() {
        let root_path = env!("CARGO_MANIFEST_DIR");
        let test_file = Url::from_file_path(format!("{}/tests/composer.json", root_path));
        let parsed_contents = parse_json_file(test_file.unwrap()).unwrap();

        assert_eq!(3, parsed_contents.dev_dependencies.len());
    }

    #[test]
    fn it_can_parse_a_valid_composer_lock_file() {
        let root_path = env!("CARGO_MANIFEST_DIR");
        let test_file = Url::from_file_path(format!("{}/tests/composer.json", root_path));
        let composer_file = parse_json_file(test_file.unwrap()).unwrap();
        let composer_lock = parse_lock_file(&composer_file);

        assert_eq!(83, composer_lock.unwrap().versions.len());
    }
}
