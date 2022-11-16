use crate::composer::ComposerDependency;
use futures::future; // 0.3.4
use reqwest::Client; // 0.10.6
use reqwest::Result;
use semver::{Version, VersionReq};
use serde_json::Value;
use std::collections::HashMap;

const PACKAGIST_REPO_URL: &str = "https://repo.packagist.org/p2";

#[derive(Debug)]
pub struct Package {
    pub name: String,
    pub versions: Vec<String>,
}

pub async fn get_packages_info(packages: Vec<ComposerDependency>) -> HashMap<String, Package> {
    let client = Client::new();

    let bodies = future::join_all(packages.into_iter().map(|package| {
        let client = &client;
        async move {
            let url = format!("{}/{}.json", PACKAGIST_REPO_URL, package.name);
            let resp = client.get(url).send().await?;
            let text = resp.text().await;

            let contents: Value = serde_json::from_str(&text.unwrap()).unwrap();

            let mut package_struct = Package {
                name: package.name,
                versions: vec![],
            };

            let packages = contents.as_object().unwrap().get("packages");
            let packages_data = packages.unwrap().as_object().unwrap();
            for (_, data) in packages_data.into_iter() {
                let package_versions = data.as_array().unwrap();

                for item_version in package_versions {
                    let version = item_version
                        .as_object()
                        .unwrap()
                        .get("version")
                        .unwrap()
                        .as_str()
                        .unwrap();

                    package_struct
                        .versions
                        .push(version.to_string().replace("v", ""));
                }
            }

            let result: Result<Package> = Ok(package_struct);
            result
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
