use crate::Url;
use log::{info, warn};
use serde::Deserialize;
use serde_json::Value;
use std::fs::{self, File};
use std::io::{prelude::*, BufReader};
use std::{collections::HashMap, hash::Hash};

#[derive(Debug, PartialEq, Deserialize, Clone)]
pub struct ComposerLockFile {
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

#[derive(Deserialize, Debug, Default)]
struct ComposerJsonFile {
    #[serde(default)]
    require: HashMap<String, String>,

    #[serde(rename(deserialize = "require-dev"), default)]
    require_dev: HashMap<String, String>,
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
pub struct ComposerFile {
    pub path: String,
    pub dependencies: Vec<ComposerDependency>,
    pub dev_dependencies: Vec<ComposerDependency>,
    pub lock: Option<ComposerLockFile>,
    pub dependencies_by_line: HashMap<u32, String>,
}

impl ComposerFile {
    pub fn new(
        path: String,
        dependencies: Vec<ComposerDependency>,
        dev_dependencies: Vec<ComposerDependency>,
        lock: Option<ComposerLockFile>,
        dependencies_by_line: HashMap<u32, String>,
    ) -> ComposerFile {
        ComposerFile {
            path,
            dependencies,
            dev_dependencies,
            lock,
            dependencies_by_line,
        }
    }

    pub fn parse_from_path(filepath: Url) -> Option<ComposerFile> {
        let file = Url::parse(&filepath.to_string()).unwrap();
        if file.path().ends_with("composer.json") == false {
            return None;
        }

        let mut composer_file = Self::new(
            filepath.to_string(),
            Vec::new(),
            Vec::new(),
            None,
            HashMap::new(),
        );

        let mut dependencies_by_line = HashMap::new();
        let file_open = File::open(file.path().to_string()).unwrap();
        let mut reader = BufReader::new(file_open);
        let composer_json_parsed: ComposerJsonFile =
            serde_json::from_reader(&mut reader).unwrap_or_default();

        // Get dependencies.
        for (name, version) in composer_json_parsed.require {
            let line_num = Self::get_line_num(filepath.path(), "require", &name, version.clone());

            match line_num {
                Some(num) => {
                    let composer_dependency = ComposerDependency {
                        name: name.to_string(),
                        version: version.to_string(),
                        // @todo figure out why we need to do this.
                        line: num - 1,
                    };

                    composer_file.dependencies.push(composer_dependency);
                    dependencies_by_line.insert(num - 1, name);
                }
                None => {
                    info!("Can't get a line number for dependency {}", name);
                }
            }
        }

        // Get dev dependencies.
        for (name, version) in composer_json_parsed.require_dev {
            let line_num =
                Self::get_line_num(filepath.path(), "require-dev", &name, version.clone());

            match line_num {
                Some(num) => {
                    let composer_dependency = ComposerDependency {
                        name: name.to_string(),
                        version: version.to_string(),
                        line: num - 1,
                    };

                    composer_file.dev_dependencies.push(composer_dependency);
                    dependencies_by_line.insert(num - 1, name);
                }
                None => {
                    info!("Can't get a line number for dev-dependency {}", name);
                }
            }
        }

        composer_file.dependencies_by_line = dependencies_by_line;
        composer_file.lock = Self::parse_lock_file(filepath);

        Some(composer_file)
    }

    fn parse_lock_file(composer_json_path: Url) -> Option<ComposerLockFile> {
        let composer_lock_path = composer_json_path
            .to_string()
            .replace("composer.json", "composer.lock");

        let file = Url::parse(&composer_lock_path);

        match file {
            Ok(file_url) => {
                let mut composer_lock = ComposerLockFile {
                    versions: HashMap::new(),
                };

                let contents = fs::read_to_string(file_url.path());

                match contents {
                    Ok(data) => {
                        let parsed_contents: Value = match serde_json::from_str(&data) {
                            Ok(v) => v,
                            Err(error) => {
                                warn!("Error while parsing lock file: {}", error);
                                Value::Null
                            }
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
                    Err(error) => {
                        info!("Can't read the lock file because its missing.");
                        info!("{}", error);

                        None
                    }
                }
            }
            Err(_error) => {
                info!("Can't parse the lock file URL.");
                None
            }
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
}

#[cfg(test)]
mod tests {
    use reqwest::Url;

    use crate::composer::ComposerFile;

    #[test]
    fn it_can_parse_a_valid_composer_json_file() {
        let root_path = env!("CARGO_MANIFEST_DIR");
        let test_file = Url::from_file_path(format!("{}/tests/composer.json", root_path));
        let parsed_contents = ComposerFile::parse_from_path(test_file.unwrap());

        assert_ne!(None, parsed_contents);
    }

    #[test]
    fn it_can_parse_required_dependencies() {
        let root_path = env!("CARGO_MANIFEST_DIR");
        let test_file = Url::from_file_path(format!("{}/tests/composer.json", root_path));
        let parsed_contents = ComposerFile::parse_from_path(test_file.unwrap()).unwrap();

        assert_eq!(3, parsed_contents.dependencies.len());
    }

    #[test]
    fn it_can_parse_required_dev_dependencies() {
        let root_path = env!("CARGO_MANIFEST_DIR");
        let test_file = Url::from_file_path(format!("{}/tests/composer.json", root_path));
        let parsed_contents = ComposerFile::parse_from_path(test_file.unwrap()).unwrap();

        assert_eq!(3, parsed_contents.dev_dependencies.len());
    }

    #[test]
    fn it_can_parse_a_valid_composer_lock_file() {
        let root_path = env!("CARGO_MANIFEST_DIR");
        let test_file = Url::from_file_path(format!("{}/tests/composer.json", root_path));
        let composer_file = ComposerFile::parse_from_path(test_file.unwrap()).unwrap();

        assert_eq!(83, composer_file.lock.unwrap().versions.len());
    }

    #[test]
    fn it_can_get_the_correct_dependency_line_number() {
        let root_path = env!("CARGO_MANIFEST_DIR");
        let test_file = Url::from_file_path(format!("{}/tests/composer.json", root_path)).unwrap();

        let line_number = ComposerFile::get_line_num(
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

        let line_number = ComposerFile::get_line_num(
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

        let required_dev_line_number = ComposerFile::get_line_num(
            test_file.path(),
            "require-dev",
            "fake/dependency",
            "^8.0".to_string(),
        )
        .unwrap();

        let required_line_number = ComposerFile::get_line_num(
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
