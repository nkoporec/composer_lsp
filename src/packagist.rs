use crate::composer::ComposerDependency;
use futures::future; // 0.3.4
use reqwest::Client; // 0.10.6
use serde_json::Value;
use reqwest::Result;
use std::collections::HashMap;

const PACKAGIST_REPO_URL: &str = "https://repo.packagist.org/p2";

#[derive(Debug)]
pub struct Package {
    pub name: String,
    pub latest_version: String,
    pub versions: HashMap<i32, Vec<String>>,
}

pub async fn get_packages_info(packages: Vec<ComposerDependency>) -> HashMap<String, Package> {
    log::info!("get_packages_info");
    let client = Client::new();

    let bodies = future::join_all(packages.into_iter().map(|package| {
        let client = &client;
        async move {
            let url = format!("{}/{}.json", PACKAGIST_REPO_URL, package.name);
            let resp = client.get(url).send().await?;
            let text = resp.text().await;

            let contents: Value = serde_json::from_str(&text.as_ref().unwrap())
                .unwrap();

            let mut package_struct = Package{
                name: package.name,
                latest_version: String::new(),
                versions: HashMap::new(),
            };

            let packages = contents.as_object().unwrap().get("packages");
            let packages_data = packages.unwrap().as_object().unwrap();
            for (_key,data) in packages_data.into_iter() {
                let package_versions = data.as_array().unwrap();

                for item_version in package_versions {
                    let version = item_version.as_object()
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
                    let version_int = version.replace(".", ""); 
                    existing_vec.push(version_int.clone());

                    package_struct.versions.insert(version_major_int, existing_vec.to_vec());

                    // Get the latest version.
                    if &version_int > &package_struct.latest_version {
                        package_struct.latest_version = version_int
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

pub fn get_latest_constraints_version(package: &Package, constraint: String) -> String {
    let pkg = package.clone();
    let mut version = "0";

    if constraint == "*" {
        return pkg.latest_version;
    }

    version.to_string()
}
