use crate::composer::ComposerDependency;
use futures::future;
use log::info;
// 0.3.4
use reqwest::Client; // 0.10.6
use reqwest::Result;
use semver::{Version, VersionReq};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::HashMap, fmt::format, vec};

const PACKAGIST_REPO_URL: &str = "https://repo.packagist.org/p2";

#[derive(Debug)]
pub struct Package {
    pub name: String,
    pub versions: Vec<String>,
    pub description: String,
    pub homepage: String,
    pub authors: Vec<String>,
    pub definition_url: String,
}

pub async fn get_packages_info(packages: Vec<ComposerDependency>) -> HashMap<String, Package> {
    let client = Client::new();

    let bodies = future::join_all(packages.into_iter().map(|package| {
        let client = &client;
        async move {
            let url = format!("{}/{}.json", PACKAGIST_REPO_URL, package.name);
            let resp = client.get(url).send().await?;
            let text = resp.text().await;

            let contents: Value = serde_json::from_str(&text.unwrap()).unwrap_or(Value::Null);

            if !contents.is_null() {
                let mut package_struct = Package {
                    name: package.name,
                    versions: vec![],
                    description: "".to_string(),
                    homepage: "".to_string(),
                    authors: vec![],
                    definition_url: "".to_string(),
                };

                match contents.as_object() {
                    Some(contents_data) => {
                        let contents_packages_object = contents_data.get("packages");
                        match contents_packages_object {
                            Some(contents_packages) => {
                                let package_data =
                                    contents_packages.get(package_struct.name.clone());
                                match package_data {
                                    Some(data) => match data.as_array() {
                                        Some(data_array) => {
                                            for item in data_array {
                                                let version = item
                                                    .as_object()
                                                    .unwrap()
                                                    .get("version")
                                                    .expect("Can't get the version string")
                                                    .as_str()
                                                    .unwrap();

                                                package_struct
                                                    .versions
                                                    .push(version.to_string().replace("v", ""));
                                            }
                                        }
                                        None => {
                                            info!(
                                                "Can't turn package data to array for {}",
                                                package_struct.name
                                            );
                                        }
                                    },
                                    None => {
                                        info!("Can't get package data for {}", package_struct.name);
                                    }
                                }
                            }
                            None => {
                                info!("Can't get packages array for {}", package_struct.name)
                            }
                        }

                        let result: Result<Package> = Ok(package_struct);
                        return result;
                    }
                    None => {
                        info!("Can't fetch Packagist data for {}", package_struct.name)
                    }
                }
            }

            let empty_package = Package {
                name: "".to_string(),
                versions: vec![],
                description: "".to_string(),
                homepage: "".to_string(),
                authors: vec![],
                definition_url: "".to_string(),
            };

            Ok(empty_package)
        }
    }))
    .await;

    let mut result: Vec<Package> = Vec::new();
    for item in bodies {
        match item {
            Ok(item) => result.push(item),
            Err(e) => log::error!("Got an error: {}", e),
        }
    }

    let mut hashmap: HashMap<String, Package> = HashMap::new();
    for i in result.into_iter() {
        hashmap.insert(i.name.to_string(), i);
    }

    return hashmap;
}

pub fn check_for_package_update(
    package: &Package,
    constraint: String,
    installed: String,
) -> Option<&str> {
    let version_constraint = VersionReq::parse(&constraint[..]);

    match version_constraint {
        Ok(req) => {
            let mut matching_versions = vec![];

            for ver in package.versions.iter() {
                let parsed_version = &Version::parse(&ver);

                match parsed_version {
                    Ok(parsed_version) => {
                        if req.matches(parsed_version) {
                            matching_versions.push(ver);
                        }
                    }
                    Err(_error) => {}
                }
            }

            if matching_versions.len() <= 0 {
                return None;
            }

            if installed == "" {
                return Some(matching_versions.first().unwrap());
            }

            let installed_normalized = installed.replace(".", "");
            let installed_as_int = installed_normalized.parse::<i32>().unwrap();
            let mut matching = vec![];

            for i in matching_versions.into_iter() {
                let i_normalized = i.replace(".", "");
                let i_as_int = i_normalized.parse::<i32>().unwrap();

                if i_as_int > installed_as_int {
                    matching.push(i);
                }
            }

            if matching.len() <= 0 {
                return None;
            }

            return Some(matching.first().unwrap());
        }
        Err(_error) => None,
    }
}

fn parse_or_constraint(package: &Package, constraint: String, installed: String) -> String {
    let split: Vec<String> = constraint
        .split("||")
        .map(|s| s.to_string().replace("||", "").replace(" ", ""))
        .collect();

    let mut versions = vec![];
    for item in split {
        let version = check_for_package_update(package, item.clone(), installed.clone());
        versions.push(version);
    }

    let mut result = "".to_string();
    for a in versions {
        match a {
            Some(ver) => result.push_str(ver),
            None => {}
        }
    }

    result
}

pub async fn get_package_info(name: String) -> Option<Package> {
    let client = Client::new();
    let url = format!("{}/{}.json", PACKAGIST_REPO_URL, name);
    let resp = client.get(url).send().await.unwrap();
    let text = resp.text().await;

    let contents: Value = serde_json::from_str(&text.unwrap()).unwrap_or(Value::Null);

    // @todo: cleanup this mess.
    if !contents.is_null() {
        match contents.as_object() {
            Some(contents_data) => {
                let contents_packages_object = contents_data.get("packages");
                match contents_packages_object {
                    Some(contents_packages) => {
                        let package_data = contents_packages.get(name.clone());
                        match package_data {
                            Some(data) => {
                                let data_array = data.as_array();
                                match data_array {
                                    Some(package_data_array) => {
                                        let item =
                                            package_data_array.get(0).unwrap().as_object().unwrap();

                                        let description = item.get("description").unwrap();
                                        let name = item.get("name").unwrap();
                                        let homepage = item.get("homepage").unwrap();
                                        let definition_url = format!(
                                            "https://packagist.org/packages/{}",
                                            name.as_str().unwrap().replace("\"", "")
                                        );

                                        let mut authors = vec![];
                                        let authors_array =
                                            item.get("authors").unwrap().as_array().unwrap();
                                        for item in authors_array {
                                            let author = item.as_object().unwrap();
                                            let author_name =
                                                author.get("name").unwrap().as_str().unwrap();
                                            authors.push(author_name.to_string());
                                        }

                                        let package = Package {
                                            name: name.to_string(),
                                            versions: vec![],
                                            description: description.to_string(),
                                            homepage: homepage.to_string(),
                                            authors,
                                            definition_url: definition_url.to_string(),
                                        };

                                        return Some(package);
                                    }
                                    None => {}
                                }
                            }
                            None => {}
                        }
                    }
                    None => {}
                }
            }
            None => {}
        }
    }

    return None;
}

#[cfg(test)]
mod tests {
    use crate::packagist::{check_for_package_update, Package};

    fn get_package_mock() -> Package {
        let package_data = Package {
            name: "Test".to_string(),
            versions: vec![
                String::from("2.2.1"),
                String::from("2.1.1"),
                String::from("2.1.0"),
                String::from("2.0.0"),
                String::from("1.9.0"),
                String::from("1.8.1"),
                String::from("1.8.0"),
            ],
            description: "".to_string(),
            homepage: "".to_string(),
            authors: vec![],
            definition_url: "".to_string(),
        };

        package_data
    }

    #[test]
    fn it_can_get_a_correct_caret_version() {
        assert_eq!(
            Some("1.9.0"),
            check_for_package_update(&get_package_mock(), "^1.0".to_string(), "".to_string())
        );
    }

    #[test]
    fn it_can_get_a_correct_higher_version() {
        assert_eq!(
            Some("2.2.1"),
            check_for_package_update(&get_package_mock(), ">2.0".to_string(), "".to_string())
        );
    }

    #[test]
    fn it_can_get_a_correct_higher_or_equal_version() {
        assert_eq!(
            Some("2.2.1"),
            check_for_package_update(&get_package_mock(), ">=2.0".to_string(), "".to_string())
        );
    }

    #[test]
    fn it_can_get_a_correct_lower_or_equal_version() {
        assert_eq!(
            Some("2.0.0"),
            check_for_package_update(&get_package_mock(), "<=2.0".to_string(), "".to_string())
        );
    }

    #[test]
    fn it_can_get_a_correct_lower_version() {
        assert_eq!(
            Some("2.1.1"),
            check_for_package_update(&get_package_mock(), "<=2.1".to_string(), "".to_string())
        );
    }

    #[test]
    fn it_can_get_a_correct_latest_version() {
        assert_eq!(
            Some("2.2.1"),
            check_for_package_update(&get_package_mock(), "*".to_string(), "".to_string())
        );
    }

    #[test]
    fn it_can_get_a_correct_tilde_version() {
        assert_eq!(
            Some("1.8.1"),
            check_for_package_update(&get_package_mock(), "~1.8".to_string(), "".to_string())
        );
    }

    #[test]
    fn it_can_get_a_correct_latest_version_with_installed_lower_version() {
        assert_eq!(
            Some("2.2.1"),
            check_for_package_update(&get_package_mock(), "^2.0".to_string(), "2.1.0".to_string())
        );
    }

    #[test]
    fn it_wont_get_anything_if_latest_is_installed_and_major_is_lower() {
        assert_eq!(
            None,
            check_for_package_update(&get_package_mock(), "^1.0".to_string(), "2.2.0".to_string())
        );
    }

    // @todo Not yet working.
    // #[test]
    // fn it_can_get_a_correct_version_if_and_constraint_is_used() {
    //     assert_eq!(
    //         Some("2.2.0"),
    //         check_for_package_update(
    //             &get_package_mock(),
    //             "^2.1.0 || ^2.2.0".to_string(),
    //             "2.1.0".to_string()
    //         )
    //     );
    // }

    #[test]
    fn it_wont_get_anything_if_latest_is_installed() {
        assert_eq!(
            None,
            check_for_package_update(&get_package_mock(), "^2.0".to_string(), "2.2.1".to_string())
        );
    }
}
