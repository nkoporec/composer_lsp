use crate::composer::ComposerDependency;
use futures::future; // 0.3.4
use reqwest::Client; // 0.10.6
use serde_json::Value;
use reqwest::Result;
use std::collections::HashMap;

const PACKAGIST_REPO_URL: &str = "https://repo.packagist.org/p2";

#[derive(Debug)]
pub struct Package {
    name: String,
    latest_version: String,
}

pub async fn get_packages_info(packages: Vec<ComposerDependency>) -> HashMap<String, String> {
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
            };

            let packages = contents.as_object().unwrap().get("packages");
            let packages_data = packages.unwrap().as_object().unwrap();
            for (_key,data) in packages_data.into_iter() {
                // @todo best var names eveer.
                let _d = data.as_array().unwrap();
                let a = data.get(0).unwrap();
                let c = a.as_object().unwrap().get("version_normalized").unwrap();

                // log::info!("{:?}", a);

                package_struct.latest_version = c.to_string();
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

    let mut hashmap: HashMap<String, String> = HashMap::new();
    for i in result.iter() {
        hashmap.insert(i.name.to_string(), i.latest_version.to_string());
    }

    return hashmap;
}
