use crate::composer::ComposerDependency;
use futures::future;
// 0.3.4
use reqwest::Client; // 0.10.6
use semver::{Version, VersionReq};
use serde_json::Value;
use std::{collections::HashMap, vec};

use serde::Deserialize;

const PACKAGIST_API_URL: &str = "https://repo.packagist.org/p2";
const PACKAGIST_REPO_URL: &str = "https://packagist.org/packages";

#[derive(Debug, Clone)]
pub struct Package {
    pub name: String,
    pub versions: Vec<PackageVersion>,
}

impl Package {
    pub fn new(name: String, versions: Vec<PackageVersion>) -> Package {
        Package { name, versions }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct PackageVersion {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub keywords: Option<Vec<String>>,
    pub homepage: Option<String>,
    pub version: Option<String>,
    #[serde(rename = "version_normalized")]
    pub version_normalized: Option<String>,
    #[serde(default)]
    pub license: Option<Vec<String>>,
    #[serde(default)]
    pub authors: Option<Vec<PackageAuthorField>>,
    pub packagist_url: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageAuthorField {
    pub name: Option<String>,
    pub email: Option<String>,
    pub homepage: Option<String>,
    pub role: Option<String>,
}

pub async fn get_packages_info(packages: Vec<ComposerDependency>) -> HashMap<String, Package> {
    let mut result = HashMap::new();

    let bodies = future::join_all(packages.into_iter().map(|package| async move {
        let package_data = get_package_info(package.clone().name).await;
        match package_data {
            Some(data) => {
                return Some(data);
            }
            None => {
                log::info!("Can't get packagist data for {}", package.clone().name);
                return None;
            }
        }
    }))
    .await;

    for item in bodies {
        if item.is_some() {
            let data = item.unwrap();
            result.insert(data.clone().name, data.clone());
        }
    }

    return result;
}

pub fn check_for_package_update(
    package: &Package,
    constraint: String,
    installed: String,
) -> Option<String> {
    let version_constraint = VersionReq::parse(&constraint[..]);

    match version_constraint {
        Ok(req) => {
            let mut matching_versions = vec![];

            for item in package.versions.iter() {
                let ver = item.clone().version.unwrap();
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
                return Some(matching_versions.first().unwrap().to_string());
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

            return Some(matching.first().unwrap().to_string());
        }
        Err(_error) => None,
    }
}

pub async fn get_package_info(name: String) -> Option<Package> {
    let client = Client::new();
    let url = format!("{}/{}.json", PACKAGIST_API_URL, name);
    let resp = client.get(url).send().await.unwrap();
    let text = resp.text().await;

    let contents: Value = serde_json::from_str(&text.unwrap()).unwrap_or(Value::Null);

    if contents.is_null() {
        return None;
    }

    match contents.as_object() {
        Some(contents_data) => {
            let contents_packages_object = contents_data.get("packages");
            match contents_packages_object {
                Some(contents_packages) => {
                    let package_data = contents_packages.get(name.clone());
                    match package_data {
                        Some(versions) => {
                            let mut package = Package::new(name.clone(), vec![]);
                            let all_versions = versions.as_array().unwrap().to_owned();
                            for item in all_versions.into_iter() {
                                let mut package_version: PackageVersion =
                                    serde_json::from_value(item.clone()).unwrap();

                                package_version.packagist_url = Some(format!(
                                    "{}/{}",
                                    PACKAGIST_REPO_URL,
                                    name.replace("\"", "")
                                ));

                                package.versions.push(package_version);
                            }

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

    return None;
}

#[cfg(test)]
mod tests {
    use crate::packagist::{check_for_package_update, Package, PackageVersion};

    fn get_package_mock() -> Package {
        let package_data = Package {
            name: "Test".to_string(),
            versions: vec![
                PackageVersion {
                    name: Some("Test".to_string()),
                    description: None,
                    keywords: None,
                    homepage: None,
                    version: Some("2.2.1".to_string()),
                    version_normalized: Some("221".to_string()),
                    license: None,
                    authors: None,
                    packagist_url: None,
                },
                PackageVersion {
                    name: Some("Test".to_string()),
                    description: None,
                    keywords: None,
                    homepage: None,
                    version: Some("2.1.1".to_string()),
                    version_normalized: Some("211".to_string()),
                    license: None,
                    authors: None,
                    packagist_url: None,
                },
                PackageVersion {
                    name: Some("Test".to_string()),
                    description: None,
                    keywords: None,
                    homepage: None,
                    version: Some("2.1.0".to_string()),
                    version_normalized: Some("210".to_string()),
                    license: None,
                    authors: None,
                    packagist_url: None,
                },
                PackageVersion {
                    name: Some("Test".to_string()),
                    description: None,
                    keywords: None,
                    homepage: None,
                    version: Some("2.0.0".to_string()),
                    version_normalized: Some("200".to_string()),
                    license: None,
                    authors: None,
                    packagist_url: None,
                },
                PackageVersion {
                    name: Some("Test".to_string()),
                    description: None,
                    keywords: None,
                    homepage: None,
                    version: Some("1.9.0".to_string()),
                    version_normalized: Some("190".to_string()),
                    license: None,
                    authors: None,
                    packagist_url: None,
                },
                PackageVersion {
                    name: Some("Test".to_string()),
                    description: None,
                    keywords: None,
                    homepage: None,
                    version: Some("1.8.1".to_string()),
                    version_normalized: Some("181".to_string()),
                    license: None,
                    authors: None,
                    packagist_url: None,
                },
                PackageVersion {
                    name: Some("Test".to_string()),
                    description: None,
                    keywords: None,
                    homepage: None,
                    version: Some("1.8.0".to_string()),
                    version_normalized: Some("180".to_string()),
                    license: None,
                    authors: None,
                    packagist_url: None,
                },
            ],
        };

        package_data
    }

    #[test]
    fn it_can_get_a_correct_caret_version() {
        assert_eq!(
            Some("1.9.0".to_string()),
            check_for_package_update(&get_package_mock(), "^1.0".to_string(), "".to_string())
        );
    }

    #[test]
    fn it_can_get_a_correct_higher_version() {
        assert_eq!(
            Some("2.2.1".to_string()),
            check_for_package_update(&get_package_mock(), ">2.0".to_string(), "".to_string())
        );
    }

    #[test]
    fn it_can_get_a_correct_higher_or_equal_version() {
        assert_eq!(
            Some("2.2.1".to_string()),
            check_for_package_update(&get_package_mock(), ">=2.0".to_string(), "".to_string())
        );
    }

    #[test]
    fn it_can_get_a_correct_lower_or_equal_version() {
        assert_eq!(
            Some("2.0.0".to_string()),
            check_for_package_update(&get_package_mock(), "<=2.0".to_string(), "".to_string())
        );
    }

    #[test]
    fn it_can_get_a_correct_lower_version() {
        assert_eq!(
            Some("2.1.1".to_string()),
            check_for_package_update(&get_package_mock(), "<=2.1".to_string(), "".to_string())
        );
    }

    #[test]
    fn it_can_get_a_correct_latest_version() {
        assert_eq!(
            Some("2.2.1".to_string()),
            check_for_package_update(&get_package_mock(), "*".to_string(), "".to_string())
        );
    }

    #[test]
    fn it_can_get_a_correct_tilde_version() {
        assert_eq!(
            Some("1.8.1".to_string()),
            check_for_package_update(&get_package_mock(), "~1.8".to_string(), "".to_string())
        );
    }

    #[test]
    fn it_can_get_a_correct_latest_version_with_installed_lower_version() {
        assert_eq!(
            Some("2.2.1".to_string()),
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
