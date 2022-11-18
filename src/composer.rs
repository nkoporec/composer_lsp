use crate::Url;
use serde::Deserialize;
use serde_json::Value;
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

#[derive(Deserialize, Debug)]
struct ComposerJsonFile {
    #[serde(default)]
    require: HashMap<String, String>,

    #[serde(rename(deserialize = "require-dev"), default)]
    require_dev: HashMap<String, String>,
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

    let file_open = File::open(file.path().to_string()).unwrap();
    let mut reader = BufReader::new(file_open);
    let composer_json_parsed: ComposerJsonFile =
        serde_json::from_reader(&mut reader).expect("Can't parse the composer json");

    // Get dependencies.
    for (name, version) in composer_json_parsed.require {
        let line_num = get_line_num(filepath.path(), "require", &name, version.clone());

        match line_num {
            Some(num) => {
                let composer_dependency = ComposerDependency {
                    name: name.to_string(),
                    version: version.to_string(),
                    line: num,
                };

                composer_file.dependencies.push(composer_dependency);
            }
            None => {}
        }
    }

    // Get dev dependencies.
    for (name, version) in composer_json_parsed.require_dev {
        let line_num = get_line_num(filepath.path(), "require-dev", &name, version.clone());

        match line_num {
            Some(num) => {
                let composer_dependency = ComposerDependency {
                    name: name.to_string(),
                    version: version.to_string(),
                    line: num,
                };

                composer_file.dev_dependencies.push(composer_dependency);
            }
            None => {}
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

fn get_line_num(
    filepath: &str,
    block_name: &str,
    dependency_name: &str,
    dependency_version: String,
) -> Option<u32> {
    let file = File::open(filepath);
    let reader = BufReader::new(file.expect("Can't retrieve a file"));

    let mut line_num = 1;
    let mut require_block_start = 0;
    let require_block_end = 0;
    for line in reader.lines() {
        if require_block_end > 0 {
            break;
        }

        let line_text = line.as_ref().expect("Can't unwrap a line text.");
        if line_text.contains(&format!("\"{}\":", block_name).to_string()) {
            require_block_start = line_num;
        }

        if require_block_start > 0 && line_num > require_block_start {
            if line_text.contains(dependency_name) && line_text.contains(&dependency_version) {
                return Some(line_num);
            }
        }

        line_num += 1;
    }

    None
}

#[cfg(test)]
mod tests {
    use reqwest::Url;

    use crate::composer::{get_line_num, parse_json_file, parse_lock_file};

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

        assert_eq!(3, parsed_contents.dependencies.len());
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

    #[test]
    fn it_can_get_the_correct_dependency_line_number() {
        let root_path = env!("CARGO_MANIFEST_DIR");
        let test_file = Url::from_file_path(format!("{}/tests/composer.json", root_path)).unwrap();

        let line_number = get_line_num(
            test_file.path(),
            "require",
            "composer/installers",
            "^2.0".to_string(),
        )
        .unwrap();

        assert_eq!(18, line_number);
    }

    #[test]
    fn it_can_get_the_correct_dev_dependency_line_number() {
        let root_path = env!("CARGO_MANIFEST_DIR");
        let test_file = Url::from_file_path(format!("{}/tests/composer.json", root_path)).unwrap();

        let line_number = get_line_num(
            test_file.path(),
            "require-dev",
            "fake/dependency",
            "^8.0".to_string(),
        )
        .unwrap();

        assert_eq!(25, line_number);
    }

    #[test]
    fn it_can_get_the_correct_dependency_line_number_with_same_name() {
        let root_path = env!("CARGO_MANIFEST_DIR");
        let test_file = Url::from_file_path(format!("{}/tests/composer.json", root_path)).unwrap();

        let required_dev_line_number = get_line_num(
            test_file.path(),
            "require-dev",
            "fake/dependency",
            "^8.0".to_string(),
        )
        .unwrap();

        let required_line_number = get_line_num(
            test_file.path(),
            "require",
            "fake/dependency",
            "^8.0".to_string(),
        )
        .unwrap();

        assert_eq!(25, required_dev_line_number);
        assert_eq!(20, required_line_number);
    }
}
