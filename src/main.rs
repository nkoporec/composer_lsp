use dashmap::DashMap;
use log::info;
use log4rs;
use std::env;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::{composer::ComposerFile, packagist::PackageVersion};

mod composer;
mod packagist;

#[derive(Debug)]
struct Backend {
    client: Client,
    composer_file: DashMap<String, ComposerFile>,
}

struct TextDocumentItem {
    uri: Url,
    version: i32,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: None,
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    file_operations: None,
                }),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "composer_lsp initialized!")
            .await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        Ok(self.on_hover(params.text_document_position_params).await)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        Ok(self.goto_definition(params).await)
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.on_save(TextDocumentItem {
            uri: params.text_document.uri,
            version: 1,
        })
        .await;
    }
}

impl Backend {
    async fn on_save(&self, params: TextDocumentItem) {
        let composer_file =
            ComposerFile::parse_from_path(params.uri.clone()).expect("Can't parse composer file");

        // Clear any old data.
        if self.composer_file.contains_key("data") {
            self.composer_file.remove("data").unwrap();
        }

        self.composer_file
            .insert("data".to_string(), composer_file.clone());

        let update_data = packagist::get_packages_info(composer_file.dependencies.clone()).await;

        let mut diagnostics: Vec<Diagnostic> = vec![];

        // Loop through "require".
        for item in composer_file.dependencies {
            if item.name == "" {
                continue;
            }

            // Packagist data.
            let packagist_data = update_data.get(&item.name);
            match packagist_data {
                Some(package) => {
                    let mut composer_lock_version = "".to_string();

                    let composer_json_version = item.version.replace("\"", "");
                    if composer_file.lock.is_some() {
                        let lock_file = composer_file.lock.clone().unwrap();

                        if lock_file.versions.len() > 0 {
                            let installed_package = lock_file.versions.get(&item.name);
                            match installed_package {
                                Some(installed) => {
                                    composer_lock_version = installed.version.clone()
                                }
                                None => {}
                            }
                        }
                    }

                    if let Some(version) = packagist::check_for_package_update(
                        package,
                        composer_json_version,
                        composer_lock_version,
                    ) {
                        let diagnostic = || -> Option<Diagnostic> {
                            Some(Diagnostic::new(
                                Range::new(
                                    Position {
                                        line: item.line,
                                        character: 1,
                                    },
                                    Position {
                                        line: 0,
                                        character: 1,
                                    },
                                ),
                                Some(DiagnosticSeverity::WARNING),
                                None,
                                None,
                                format!("Update available: {:?}", version),
                                None,
                                None,
                            ))
                        }();

                        diagnostics.push(diagnostic.unwrap());
                    }
                }
                None => {}
            }
        }

        self.client
            .publish_diagnostics(params.uri.clone(), diagnostics, Some(params.version))
            .await;
    }

    async fn on_hover(&self, params: TextDocumentPositionParams) -> Option<Hover> {
        let composer_file = self.composer_file.get("data").unwrap();

        let line = params.position.line;
        let dependency = composer_file.dependencies_by_line.get(&line);

        match dependency {
            Some(name) => {
                let package_info = packagist::get_package_info(name.to_string()).await;
                match package_info {
                    Some(data) => {
                        let mut package_version = PackageVersion {
                            name: None,
                            description: None,
                            keywords: None,
                            homepage: None,
                            version: None,
                            version_normalized: None,
                            license: None,
                            authors: None,
                            packagist_url: None,
                        };

                        match &composer_file.lock {
                            Some(lock) => {
                                if lock.versions.contains_key(name) {
                                    let installed_package = lock.versions.get(name).unwrap();

                                    for item in data.versions.iter() {
                                        let item_version =
                                            item.version.as_ref().unwrap().to_owned();

                                        if item_version.replace(".", "")
                                            == installed_package.version.replace(".", "")
                                        {
                                            package_version = item.to_owned();
                                        }
                                    }
                                } else {
                                    package_version = data.versions.get(0).unwrap().to_owned();
                                }
                            }
                            None => {
                                package_version = data.versions.get(0).unwrap().to_owned();
                            }
                        }

                        let mut contents = vec![];

                        let description = package_version.description.as_ref();
                        match description {
                            Some(desc) => {
                                let description_contents =
                                    MarkedString::from_markdown(desc.to_string());
                                contents.push(description_contents);

                                let new_line = MarkedString::from_markdown("".to_string());
                                contents.push(new_line);
                            }
                            None => {}
                        }

                        let homepage = package_version.homepage.as_ref();
                        match homepage {
                            Some(page) => {
                                let homepage_contents =
                                    MarkedString::from_markdown(format!("Homepage: {}", page));
                                contents.push(homepage_contents);

                                let new_line = MarkedString::from_markdown("".to_string());
                                contents.push(new_line);
                            }
                            None => {}
                        }

                        let range = Range::new(
                            Position { line, character: 1 },
                            Position {
                                line: 0,
                                character: 1,
                            },
                        );

                        return Some(Hover {
                            contents: HoverContents::Array(contents),
                            range: Some(range),
                        });
                    }
                    None => {
                        let error = format!("No hover data found for: {}", name);
                        log::error!("{}", error);
                        self.client.log_message(MessageType::ERROR, error).await;
                    }
                }
            }

            None => {
                let error = format!(
                    "Hover failed, because we can't find this line number: {}",
                    line
                );

                log::error!("{}", error);
                self.client.log_message(MessageType::ERROR, error).await;
            }
        }

        None
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Option<GotoDefinitionResponse> {
        let composer_file = self.composer_file.get("data").unwrap();

        let line = params.text_document_position_params.position.line;
        let dependency = composer_file.dependencies_by_line.get(&line);

        match dependency {
            Some(name) => {
                let package_info = packagist::get_package_info(name.to_string()).await;
                match package_info {
                    Some(data) => {
                        let mut package_version = PackageVersion {
                            name: None,
                            description: None,
                            keywords: None,
                            homepage: None,
                            version: None,
                            version_normalized: None,
                            license: None,
                            authors: None,
                            packagist_url: None,
                        };

                        match &composer_file.lock {
                            Some(lock) => {
                                if lock.versions.contains_key(name) {
                                    let installed_package = lock.versions.get(name).unwrap();

                                    for item in data.versions.iter() {
                                        let item_version =
                                            item.version.as_ref().unwrap().to_owned();

                                        if item_version == installed_package.version {
                                            package_version = item.to_owned();
                                        }
                                    }
                                } else {
                                    package_version = data.versions.get(0).unwrap().to_owned();
                                }
                            }
                            None => {
                                package_version = data.versions.get(0).unwrap().to_owned();
                            }
                        }

                        let packagist_url = package_version.packagist_url.as_ref();
                        match packagist_url {
                            Some(page) => {
                                if webbrowser::open(page).is_ok() {
                                    return None;
                                }
                            }
                            None => {
                                let error = format!("Can't open the definition_url for: {}", name);
                                log::error!("{}", error);
                                self.client.log_message(MessageType::ERROR, error).await;
                            }
                        }
                    }
                    None => {
                        let error = format!("No definiton data found for: {}", name);
                        log::error!("{}", error);
                        self.client.log_message(MessageType::ERROR, error).await;
                    }
                }
            }
            None => {
                let error = format!(
                    "Go to definition failed, because we can't find this line number: {}",
                    line
                );

                log::error!("{}", error);
                self.client.log_message(MessageType::ERROR, error).await;
            }
        }

        None
    }
}

#[tokio::main]
async fn main() {
    match env::var("COMPOSER_LSP_LOG") {
        Ok(value) => {
            log4rs::init_file(value, Default::default()).unwrap();
            info!("LOG4RS logging enabled")
        }
        Err(_error) => {}
    }

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::build(|client| Backend {
        client,
        composer_file: DashMap::new(),
    })
    .finish();
    Server::new(stdin, stdout, socket).serve(service).await;
}
