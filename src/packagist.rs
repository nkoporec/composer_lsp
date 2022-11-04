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

pub async fn get_packages_info(packages: Vec<ComposerDependency>) -> HashMap<String, Package> {
    let client = Client::new();

    let bodies = future::join_all(packages.into_iter().map(|package| {
        let client = &client;
        async move {
            let url = format!("{}/{}.json", PACKAGIST_REPO_URL, package.name);
            let resp = client.get(url).send().await?;
            let text = resp.text().await;

            let contents: Value = serde_json::from_str(&text.as_ref().unwrap()).unwrap();

            let mut package_struct = Package {
                name: package.name,
                latest_version: String::new(),
                versions: HashMap::new(),
            };

            let packages = contents.as_object().unwrap().get("packages");
            let packages_data = packages.unwrap().as_object().unwrap();
            for (_key, data) in packages_data.into_iter() {
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

pub fn get_latest_constraints_version(
    package: &Package,
    constraint: String,
    composer_version: String,
) -> String {
    let mut version = "0";
    let mut higher_minor_versions: Vec<String> = vec![];
    let mut lower_minor_versions: Vec<String> = vec![];

    let major_version = get_major_version(&composer_version);
    let minor_version = get_minor_version(&composer_version);

    let all_minor_versions = &package.versions.get_key_value(&major_version).unwrap().1;
    for i in all_minor_versions.iter() {
        let i_minor: Vec<&str> = i.split(".").collect();
        let i_ver = i_minor[1].replace("\"", "").parse::<i32>().unwrap();

        if i_ver > minor_version {
            higher_minor_versions.push(i.to_string());
        } else {
            lower_minor_versions.push(i.to_string());
        }
    }

    if constraint == "*" {
        version = &package.latest_version;
    }

    if constraint == "^" {
        version = &package
            .versions
            .get_key_value(&major_version)
            .unwrap()
            .1
            .first()
            .unwrap();
    }

    if constraint == ">" {
        version = higher_minor_versions.last().unwrap();
    }

    if constraint == "<" {}

    version.to_string()
}

fn get_major_version(version: &String) -> i32 {
    let mut v = version.clone();

    for i in composer::composer_constraints_chars() {
        v = v.replace(i, "");
    }

    let major = v.get(1..2).unwrap().parse::<i32>().unwrap();

    major
}

fn get_minor_version(version: &String) -> i32 {
    let mut v = version.clone();

    for i in composer::composer_constraints_chars() {
        v = v.replace(i, "");
    }

    let minor: Vec<&str> = v.split(".").collect();
    let minor_ver = minor[1].replace("\"", "").parse::<i32>().unwrap();

    minor_ver
}
