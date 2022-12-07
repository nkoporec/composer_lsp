use dashmap::DashMap;
use log::info;
use log4rs;
use serde_json::Value;
use std::env;
use std::{process::Command as ProcessCommand, str::from_utf8};
use tower_lsp::jsonrpc::{Error, ErrorCode::ServerError, Result};
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::{composer::ComposerFile, packagist::PackageVersion};

mod composer;
mod packagist;

#[derive(Debug)]
struct Backend {
    client: Client,
    composer_file: DashMap<String, ComposerFile>,
    packagist_packages: DashMap<String, Vec<String>>,
    buffer: DashMap<u32, String>,
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
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: {
                        let chars = ('a'..='z').into_iter().collect::<Vec<char>>();
                        let triggers: Vec<String> =
                            chars.clone().iter().map(|x| x.to_string()).collect();

                        Some(triggers)
                    },
                    work_done_progress_options: Default::default(),
                    all_commit_characters: None,
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
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
        let all_packages = packagist::get_all_packages().await;

        // Clear any old data.
        if self.packagist_packages.contains_key("data") {
            self.packagist_packages.remove("data").unwrap();
        }

        self.packagist_packages
            .insert("data".to_string(), all_packages);

        self.client
            .log_message(MessageType::INFO, "composer_lsp initialized!")
            .await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        Ok(self.on_hover(params.text_document_position_params).await)
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        self.on_code_action(params).await
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

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        self.on_change(params).await
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.on_save(TextDocumentItem {
            uri: params.text_document.uri,
            version: 1,
        })
        .await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        if !self.packagist_packages.contains_key("data") {
            return Ok(None);
        }

        let position = params.text_document_position.position;
        let line_text = self.buffer.get(&position.line).unwrap().to_owned();

        let start_completion_pos = line_text.rfind("\"");
        match start_completion_pos {
            Some(start_pos) => {
                let partial_completion = line_text[start_pos..]
                    .to_string()
                    .replace(" ", "")
                    .replace("\"", "")
                    .replace("\n", "");

                if partial_completion.len() >= 2 {
                    let completions = || -> Option<Vec<CompletionItem>> {
                        let mut ret = vec![];
                        let all_packages = self.packagist_packages.get("data").unwrap();
                        for name in all_packages.iter() {
                            if name.starts_with(&partial_completion) {
                                ret.push(CompletionItem {
                                    label: name.to_string(),
                                    insert_text: Some(name.to_string()),
                                    kind: Some(CompletionItemKind::VARIABLE),
                                    detail: Some(name.to_string()),
                                    ..Default::default()
                                });
                            }
                        }

                        Some(ret)
                    }();

                    return Ok(completions.map(CompletionResponse::Array));
                }
            }
            None => {}
        };

        Ok(None)
    }

    async fn execute_command(&self, params: ExecuteCommandParams) -> Result<Option<Value>> {
        self.on_execute_command(params).await
    }
}

impl Backend {
    async fn on_change(&self, params: DidChangeTextDocumentParams) {
        let changes = &params.content_changes[0];
        let ropey = ropey::Rope::from_str(&changes.text);

        // clear buffer.
        self.buffer.clear();

        // write to buffer.
        let mut line_num = 0;
        for line in ropey.lines() {
            self.buffer.insert(line_num, line.to_string());
            line_num += 1;
        }
    }

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
        if !self.composer_file.contains_key("data") {
            return None;
        }

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
                            None => {
                                // Just pull latest.
                                let latest_package_version =
                                    data.versions.get(0).unwrap().to_owned();

                                let description_contents = MarkedString::from_markdown(
                                    latest_package_version.description.unwrap().to_string(),
                                );
                                contents.push(description_contents);
                            }
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
                            None => {
                                // Just pull latest.
                                let latest_package_version =
                                    data.versions.get(0).unwrap().to_owned();

                                let homepage_contents = MarkedString::from_markdown(
                                    latest_package_version.homepage.unwrap().to_string(),
                                );
                                contents.push(homepage_contents);
                            }
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
        if !self.composer_file.contains_key("data") {
            return None;
        }

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
                                // Try to get the latest one.
                                let latest_package_version =
                                    data.versions.get(0).unwrap().to_owned();

                                if webbrowser::open(&latest_package_version.packagist_url.unwrap())
                                    .is_ok()
                                {
                                    return None;
                                } else {
                                    let error =
                                        format!("Can't open the definition_url for: {}", name);
                                    log::error!("{}", error);
                                    self.client.log_message(MessageType::ERROR, error).await;
                                }
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

    async fn on_code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        if !self.composer_file.contains_key("data") {
            return Err(Error::method_not_found());
        }

        let composer_file = self.composer_file.get("data").unwrap();

        let range_start_line = params.range.start.line;
        let range_end_line = params.range.end.line;

        if range_start_line != range_end_line {
            return Err(Error::method_not_found());
        }

        let line = range_start_line;
        let dependency_found = composer_file.dependencies_by_line.get(&line);

        match dependency_found {
            Some(dependency) => {
                let mut commands = vec![];

                if composer_file.lock.is_none() {
                    let install_command = Command {
                        title: "Install all packages".to_string(),
                        command: "install".to_string(),
                        arguments: Some(vec![]),
                    };

                    commands.push(CodeActionOrCommand::Command(install_command));
                } else {
                    let update_command = Command {
                        title: "Update package".to_string(),
                        command: "update".to_string(),
                        arguments: Some(vec![Value::from(dependency.to_owned())]),
                    };

                    commands.push(CodeActionOrCommand::Command(update_command));
                }

                return Ok(Some(commands));
            }
            None => {
                return Err(Error::method_not_found());
            }
        }
    }

    async fn on_execute_command(&self, params: ExecuteCommandParams) -> Result<Option<Value>> {
        if !self.composer_file.contains_key("data") {
            return Ok(None);
        }

        let composer_file = self.composer_file.get("data").unwrap();
        let command = &params.command[..];

        match command {
            "update" => {
                let command_path = composer_file
                    .path
                    .replace("/composer.json", "")
                    .replace("file://", "");
                if params.arguments.len() <= 0 {
                    return Ok(None);
                }

                let dependency = params.arguments.get(0).unwrap().as_str().unwrap();
                let output = ProcessCommand::new("composer")
                    .arg(format!("--working-dir={}", command_path).as_str())
                    .arg("update")
                    .arg(dependency)
                    .output()
                    .expect("failed to execute process");

                if !output.status.success() {
                    self.client
                        .show_message(MessageType::INFO, "Composer command failed.")
                        .await;
                    return Err(Error::new(ServerError(400)));
                }

                match from_utf8(&output.stderr) {
                    Ok(message) => {
                        if message.contains("Your requirements could not be resolved to an installable set of packages") {
                            self.client.show_message(MessageType::INFO, "Composer dependencies could not be resolved.").await;
                            return Ok(None);
                        }

                        self.client
                            .show_message(
                                MessageType::INFO,
                                format!("Composer package {} was updated.", dependency),
                            )
                            .await;
                        return Ok(None);
                    }
                    Err(_) => {
                        return Err(Error::new(ServerError(400)));
                    }
                };
            }
            "install" => {
                let command_path = composer_file
                    .path
                    .replace("/composer.json", "")
                    .replace("file://", "");

                let output = ProcessCommand::new("composer")
                    .arg(format!("--working-dir={}", command_path).as_str())
                    .arg("install")
                    .output()
                    .expect("failed to execute process");

                if !output.status.success() {
                    self.client
                        .show_message(MessageType::INFO, "Composer command failed.")
                        .await;
                    return Err(Error::new(ServerError(400)));
                }

                match from_utf8(&output.stderr) {
                    Ok(message) => {
                        if message.contains("Your requirements could not be resolved to an installable set of packages") {
                            self.client.show_message(MessageType::INFO, "Composer dependencies could not be resolved.").await;
                            return Ok(None);
                        }

                        self.client
                            .show_message(
                                MessageType::INFO,
                                format!("Composer packages were installed.",),
                            )
                            .await;
                        return Ok(None);
                    }
                    Err(_) => {
                        return Err(Error::new(ServerError(400)));
                    }
                };
            }
            _ => return Err(Error::method_not_found()),
        }
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
        packagist_packages: DashMap::new(),
        buffer: DashMap::new(),
    })
    .finish();
    Server::new(stdin, stdout, socket).serve(service).await;
}
