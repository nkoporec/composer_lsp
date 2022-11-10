use crate::Url;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{prelude::*, BufReader};

#[derive(Debug, PartialEq, Deserialize)]
pub struct ComposerFile {
    pub name: String,
    pub dependencies: Vec<ComposerDependency>,
    pub dev_dependencies: Vec<ComposerDependency>,
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
pub struct ComposerDependency {
    pub name: String,
    pub version: String,
    pub line: u32,
}

pub fn parse_file(filepath: Url) -> Option<ComposerFile> {
    let file = Url::parse(&filepath.to_string()).unwrap();
    let mut composer_file = ComposerFile {
        name: String::new(),
        dependencies: Vec::new(),
        dev_dependencies: Vec::new(),
    };

    if file.path().ends_with("composer.json") == false {
        return None;
    }

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
    let buffer = parse_by_line(file.path());

    // Get dependencies.
    if parsed_contents_object.contains_key("require") {
        let dependencies = &parsed_contents_object["require"];
        let dep_obj = dependencies.as_object().unwrap();
        for (name, version) in dep_obj {
            let line = buffer
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
    if parsed_contents_object.contains_key("require-dev") {
        let dependencies = &parsed_contents_object["require-dev"];
        let dep_obj = dependencies.as_object().unwrap();
        for (name, version) in dep_obj {
            let line = buffer
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

fn parse_by_line(filepath: &str) -> HashMap<String, u32> {
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

#[cfg(test)]
mod tests {
    use reqwest::Url;

    use crate::composer::parse_file;

    #[test]
    fn it_can_parse_a_valid_composer_json_file() {
        let root_path = env!("CARGO_MANIFEST_DIR");
        let test_file = Url::from_file_path(format!("{}/tests/composer.json", root_path));
        let parsed_contents = parse_file(test_file.unwrap());

        assert_ne!(None, parsed_contents);
    }

    #[test]
    fn it_can_parse_required_dependencies() {
        let root_path = env!("CARGO_MANIFEST_DIR");
        let test_file = Url::from_file_path(format!("{}/tests/composer.json", root_path));
        let parsed_contents = parse_file(test_file.unwrap()).unwrap();

        assert_eq!(2, parsed_contents.dependencies.len());
    }

    #[test]
    fn it_can_parse_required_dev_dependencies() {
        let root_path = env!("CARGO_MANIFEST_DIR");
        let test_file = Url::from_file_path(format!("{}/tests/composer.json", root_path));
        let parsed_contents = parse_file(test_file.unwrap()).unwrap();

        assert_eq!(1, parsed_contents.dev_dependencies.len());
    }
}
