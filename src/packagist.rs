use crate::composer;
use crate::composer::ComposerDependency;
use futures::future; // 0.3.4
use reqwest::Client; // 0.10.6
use reqwest::Result;
use serde_json::Value;
use std::collections::HashMap;

const PACKAGIST_REPO_URL: &str = "https://repo.packagist.org/p2";

#[derive(Debug)]
pub struct Package {
    pub name: String,
    pub latest_version: String,
    pub versions: HashMap<i32, Vec<String>>,
}

enum VersionChar {
    Greater,
    GreaterOrEqual,
    Lesser,
    LesserOrEqual,
    Equal,
    NotSame,
    LogicalAnd,
    LogicalOr,
}

enum ConstraintChar {
    Hyphen,
    Wildcard,
    Tilde,
    Caret,
}

impl ConstraintChar {
    fn as_str(&self) -> &'static str {
        match self {
            ConstraintChar::Hyphen => "-",
            ConstraintChar::Wildcard => "*",
            ConstraintChar::Tilde => "~",
            ConstraintChar::Caret => ">",
        }
    }
}

impl VersionChar {
    fn as_str(&self) -> &'static str {
        match self {
            VersionChar::Greater => ">",
            VersionChar::GreaterOrEqual => ">=",
            VersionChar::Lesser => "<",
            VersionChar::LesserOrEqual => "<=",
            VersionChar::Equal => "=",
            VersionChar::NotSame => "!=",
            VersionChar::LogicalAnd => "||",
            VersionChar::LogicalOr => "&&",
        }
    }
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
                latest_version: String::new(),
                versions: HashMap::new(),
            };

            let packages = contents.as_object().unwrap().get("packages");
            let packages_data = packages.unwrap().as_object().unwrap();
            for (_, data) in packages_data.into_iter() {
                let package_versions = data.as_array().unwrap();

                for item_version in package_versions {
                    let version = item_version
                        .as_object()
                        .unwrap()
                        .get("version_normalized")
                        .unwrap()
                        .as_str()
                        .unwrap();

                    let version_split: Vec<&str> = version.split(".").collect();
                    // 2
                    let version_major = version_split.get(0).cloned().unwrap();
                    let version_major_int = version_major.parse::<i32>().unwrap();
                    // Either Some or None.
                    // If none, create a new vec.
                    let mut existing = package_struct.versions.get(&version_major_int).cloned();
                    if existing.is_none() {
                        existing = Some(vec![]);
                    }
                    let mut existing_vec = existing.unwrap();

                    // 2110
                    existing_vec.push(version.to_string().clone());

                    package_struct
                        .versions
                        .insert(version_major_int, existing_vec.to_vec());

                    // Get the latest version.
                    if &version.to_string() > &package_struct.latest_version {
                        package_struct.latest_version = version.to_string();
                    }
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

pub fn get_latest_constraint_version(package: &Package, constraint: String) -> &str {
    let or = constraint.find(VersionChar::LogicalOr.as_str());
    if or != None {
        todo!();
    }

    let and = constraint.find(VersionChar::LogicalAnd.as_str());
    if and != None {
        todo!()
    }

    let first_constraint_char = &constraint[0..1];
    match first_constraint_char {
        // *
        // *.1.0
        "*" => {
            if constraint.len() == 1 {
                let mut latest_major_version = 0;
                for (key, i) in package.versions.iter() {
                    if key > &latest_major_version {
                        latest_major_version = *key;
                    }
                }

                let latest_major_versions = package.versions.get(&latest_major_version).unwrap();

                return latest_major_versions.first().unwrap();
            }

            todo!();
        }
        // ^1.0
        "^" => todo!(),
        // ~1.3.1
        // ~1.5
        "~" => todo!(),
        // 1.3.1
        // 1.3.*
        // 1.*.*
        _ => todo!(),
    };

    "1"
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::packagist::{get_latest_constraint_version, Package};

    fn get_package_mock() -> Package {
        let package_data = Package {
            name: "Test".to_string(),
            latest_version: "2.1".to_string(),
            versions: HashMap::from([
                (
                    2,
                    vec![
                        String::from("2.2"),
                        String::from("2.1"),
                        String::from("2.0"),
                    ],
                ),
                (
                    1,
                    vec![
                        String::from("1.4"),
                        String::from("1.3"),
                        String::from("1.2"),
                    ],
                ),
            ]),
        };

        package_data
    }

    #[test]
    fn it_can_get_a_correct_latest_version() {
        assert_eq!(
            "2.2",
            get_latest_constraint_version(&get_package_mock(), "*".to_string())
        );
    }

    #[test]
    fn it_can_get_a_correct_latest_with_wildcard() {
        assert_eq!(
            "2.1",
            get_latest_constraint_version(&get_package_mock(), "*.1".to_string())
        );
    }

    #[test]
    fn it_can_get_a_correct_fixed_version() {
        assert_eq!(
            "2.1",
            get_latest_constraint_version(&get_package_mock(), "2.1".to_string())
        );
    }
}
