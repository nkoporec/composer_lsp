use crate::Url;
use std::fs;
use serde_json::{Result, Value};

#[derive(Debug, PartialEq)]
pub struct ComposerFile {
    pub uri: Url,
    pub dependencies: Vec<ComposerDependency>,
}

#[derive(Debug, PartialEq)]
pub struct ComposerDependency {
    pub name: String,
    pub version: String,
    pub line: i32,
}

pub fn parse_file(filepath: Url) -> Option<ComposerFile> {
    let file = Url::parse(&filepath.to_string()).unwrap();

    if file.path().ends_with("composer.json") == false {
        return None;
    }

    let contents = fs::read_to_string(file.path())
        .expect("Error while reading the composer file");


    let parsed_contents: Value = serde_json::to_value(&contents).unwrap();

    Some(ComposerFile{
        uri: filepath,
        dependencies: vec![],
    })
}
