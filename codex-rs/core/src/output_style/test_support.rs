//! Test helpers: a file system that records what it reads, and config layer stacks.

use std::sync::Mutex;
use std::sync::PoisonError;

use codex_config::ConfigLayerEntry;
use codex_config::ConfigLayerSource;
use codex_config::ConfigLayerStack;
use codex_config::ConfigRequirementsToml;
use codex_exec_server::CopyOptions;
use codex_exec_server::CreateDirectoryOptions;
use codex_exec_server::ExecutorFileSystem;
use codex_exec_server::ExecutorFileSystemFuture;
use codex_exec_server::FileMetadata;
use codex_exec_server::FileSystemReadStream;
use codex_exec_server::FileSystemSandboxContext;
use codex_exec_server::GetMetadataOptions;
use codex_exec_server::LOCAL_FS;
use codex_exec_server::ReadDirectoryEntry;
use codex_exec_server::ReadFileOptions;
use codex_exec_server::RemoveOptions;
use codex_exec_server::WalkOptions;
use codex_exec_server::WalkOutcome;
use codex_exec_server::WriteFileOptions;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_path_uri::PathUri;

/// The local file system, recording the paths it was asked to read or list.
#[derive(Default)]
pub(crate) struct RecordingFs {
    accessed: Mutex<Vec<String>>,
}

impl RecordingFs {
    pub(crate) fn accessed(&self) -> Vec<String> {
        self.accessed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn record(&self, path: &PathUri) {
        let path = path
            .to_abs_path()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| format!("{path:?}"));
        self.accessed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(path);
    }
}

impl ExecutorFileSystem for RecordingFs {
    fn canonicalize<'a>(
        &'a self,
        path: &'a PathUri,
        sandbox: Option<&'a FileSystemSandboxContext>,
    ) -> ExecutorFileSystemFuture<'a, PathUri> {
        self.record(path);
        LOCAL_FS.canonicalize(path, sandbox)
    }

    fn read_file<'a>(
        &'a self,
        path: &'a PathUri,
        options: ReadFileOptions,
        sandbox: Option<&'a FileSystemSandboxContext>,
    ) -> ExecutorFileSystemFuture<'a, Vec<u8>> {
        self.record(path);
        LOCAL_FS.read_file(path, options, sandbox)
    }

    fn read_file_stream<'a>(
        &'a self,
        path: &'a PathUri,
        sandbox: Option<&'a FileSystemSandboxContext>,
    ) -> ExecutorFileSystemFuture<'a, FileSystemReadStream> {
        self.record(path);
        LOCAL_FS.read_file_stream(path, sandbox)
    }

    fn write_file<'a>(
        &'a self,
        path: &'a PathUri,
        contents: Vec<u8>,
        options: WriteFileOptions,
        sandbox: Option<&'a FileSystemSandboxContext>,
    ) -> ExecutorFileSystemFuture<'a, ()> {
        LOCAL_FS.write_file(path, contents, options, sandbox)
    }

    fn create_directory<'a>(
        &'a self,
        path: &'a PathUri,
        options: CreateDirectoryOptions,
        sandbox: Option<&'a FileSystemSandboxContext>,
    ) -> ExecutorFileSystemFuture<'a, ()> {
        LOCAL_FS.create_directory(path, options, sandbox)
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a PathUri,
        options: GetMetadataOptions,
        sandbox: Option<&'a FileSystemSandboxContext>,
    ) -> ExecutorFileSystemFuture<'a, FileMetadata> {
        self.record(path);
        LOCAL_FS.get_metadata(path, options, sandbox)
    }

    fn read_directory<'a>(
        &'a self,
        path: &'a PathUri,
        sandbox: Option<&'a FileSystemSandboxContext>,
    ) -> ExecutorFileSystemFuture<'a, Vec<ReadDirectoryEntry>> {
        self.record(path);
        LOCAL_FS.read_directory(path, sandbox)
    }

    fn walk<'a>(
        &'a self,
        path: &'a PathUri,
        options: WalkOptions,
        sandbox: Option<&'a FileSystemSandboxContext>,
    ) -> ExecutorFileSystemFuture<'a, WalkOutcome> {
        self.record(path);
        LOCAL_FS.walk(path, options, sandbox)
    }

    fn remove<'a>(
        &'a self,
        path: &'a PathUri,
        options: RemoveOptions,
        sandbox: Option<&'a FileSystemSandboxContext>,
    ) -> ExecutorFileSystemFuture<'a, ()> {
        LOCAL_FS.remove(path, options, sandbox)
    }

    fn copy<'a>(
        &'a self,
        source_path: &'a PathUri,
        destination_path: &'a PathUri,
        options: CopyOptions,
        sandbox: Option<&'a FileSystemSandboxContext>,
    ) -> ExecutorFileSystemFuture<'a, ()> {
        LOCAL_FS.copy(source_path, destination_path, options, sandbox)
    }
}

fn empty_config() -> toml::Value {
    toml::Value::Table(toml::map::Map::new())
}

/// A stack with the user layer of `codex_home` and one project layer per `(folder, trusted)`,
/// farthest project first.
pub(crate) fn layer_stack(
    codex_home: &AbsolutePathBuf,
    projects: &[(&AbsolutePathBuf, bool)],
) -> ConfigLayerStack {
    let mut layers = vec![ConfigLayerEntry::new(
        ConfigLayerSource::User {
            file: codex_home.join("config.toml"),
            profile: None,
        },
        empty_config(),
    )];
    for (dot_codex_folder, trusted) in projects {
        let source = ConfigLayerSource::Project {
            dot_codex_folder: (*dot_codex_folder).clone(),
        };
        layers.push(if *trusted {
            ConfigLayerEntry::new(source, empty_config())
        } else {
            ConfigLayerEntry::new_disabled(source, empty_config(), "untrusted project")
        });
    }
    ConfigLayerStack::new(
        layers,
        Default::default(),
        ConfigRequirementsToml::default(),
    )
    .expect("valid config stack")
}

/// Writes `output-styles/<file_name>` under `folder`.
pub(crate) fn write_style(folder: &AbsolutePathBuf, file_name: &str, contents: &str) {
    let dir = folder.join(super::catalog::OUTPUT_STYLES_DIR);
    std::fs::create_dir_all(&dir).expect("create output-styles folder");
    std::fs::write(dir.join(file_name), contents).expect("write style file");
}

/// A valid style file.
pub(crate) fn style_file(name: Option<&str>, description: &str, text: &str) -> String {
    let name = name
        .map(|name| format!("name: {name}\n"))
        .unwrap_or_default();
    format!("---\n{name}description: {description}\n---\n{text}\n")
}
