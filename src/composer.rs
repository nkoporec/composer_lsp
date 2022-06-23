use crate::Url;
use std::fs;
use serde_json::Value;
use serde::Deserialize;

#[derive(Debug, PartialEq, Deserialize)]
pub struct ComposerFile {
    pub name: String,
    pub dependencies: Vec<ComposerDependency>,
    pub dev_dependencies: Vec<ComposerDependency>,
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct ComposerDependency {
    pub name: String,
    pub version: String,
    pub line: i32,
}

pub fn parse_file(filepath: Url) -> Option<ComposerFile> {
    let file = Url::parse(&filepath.to_string()).unwrap();
    let mut composer_file = ComposerFile{
        name: String::new(),
        dependencies: Vec::new(),
        dev_dependencies: Vec::new(),
    };

    if file.path().ends_with("composer.json") == false {
        return None;
    }

    let contents = fs::read_to_string(file.path())
        .expect("Error while reading the composer file");

    let parsed_contents: Value = serde_json::from_str(&contents).unwrap();
    let parsed_contents_object = parsed_contents.as_object().unwrap();

    // Get dependencies.
    if parsed_contents_object.contains_key("require") {
        let dependencies = &parsed_contents_object["require"];
        let dep_obj = dependencies.as_object().unwrap();
        for (name,version) in dep_obj {
            let composer_dependency = ComposerDependency{
                name: name.to_string(),
                version: version.to_string(),
                line: 1,
            };

            composer_file.dependencies.push(composer_dependency);
        }
    }

    // Get dev dependencies.
    if parsed_contents_object.contains_key("require-dev") {
        let dependencies = &parsed_contents_object["require-dev"];
        let dep_obj = dependencies.as_object().unwrap();
        for (name,version) in dep_obj {
            let composer_dependency = ComposerDependency{
                name: name.to_string(),
                version: version.to_string(),
                line: 1,
            };

            composer_file.dev_dependencies.push(composer_dependency);
        }
    }

    log::info!("{:?}", composer_file.dev_dependencies);

    Some(composer_file)
}
