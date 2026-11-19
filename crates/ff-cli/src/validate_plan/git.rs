//! Read-only Git plumbing. Never diff the checkout through Git: that can invoke clean filters.
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use crate::CliError;

const MAX_LIST: u64 = 2 * 1024 * 1024;
pub(super) const MAX_FILE: usize = 128 * 1024;

pub(super) fn error(reason: &str) -> CliError {
    CliError::Other(format!("validate-plan: {reason}"))
}

pub(super) fn relative(path: &str) -> bool {
    path == "."
        || (!path.is_empty()
            && !path.contains(['\\', ':'])
            && !path.chars().any(char::is_control)
            && path
                .split('/')
                .all(|p| !p.is_empty() && p != "." && p != ".."))
}

fn oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Entry {
    pub mode: String,
    pub oid: String,
}

pub(super) struct Git {
    root: PathBuf,
    exe: PathBuf,
}

impl Git {
    pub fn open(root: PathBuf) -> Result<Self, CliError> {
        if !root.join(".git").exists() {
            return Err(error("execute na raiz Git ou informe --root"));
        }
        let exe = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .filter(|p| p.is_absolute())
            .filter_map(|p| {
                p.join(format!("git{}", std::env::consts::EXE_SUFFIX))
                    .canonicalize()
                    .ok()
            })
            .find(|p| p.is_file() && !p.starts_with(&root))
            .ok_or_else(|| {
                error("Git não encontrado em um diretório absoluto de PATH fora da raiz")
            })?;
        let git = Self { root, exe };
        let top = git.list(&["rev-parse", "--show-toplevel"])?;
        if !std::str::from_utf8(&top)
            .ok()
            .and_then(|s| s.strip_suffix('\n'))
            .and_then(|s| Path::new(s).canonicalize().ok())
            .is_some_and(|top| top == git.root)
        {
            return Err(error("a raiz informada não é a raiz do working tree Git"));
        }
        Ok(git)
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(&self.exe);
        cmd.current_dir(&self.root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for (key, _) in std::env::vars_os() {
            if key
                .to_string_lossy()
                .to_ascii_uppercase()
                .starts_with("GIT_")
            {
                cmd.env_remove(key);
            }
        }
        cmd.env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_NO_REPLACE_OBJECTS", "1")
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("GIT_ALLOW_PROTOCOL", "")
            .args([
                "--no-pager",
                "-c",
                "core.fsmonitor=false",
                "-c",
                "core.excludesFile=/dev/null",
            ])
            .args(args);
        cmd
    }

    fn list(&self, args: &[&str]) -> Result<Vec<u8>, CliError> {
        let mut child = self
            .command(args)
            .spawn()
            .map_err(|_| error("falha ao iniciar Git"))?;
        let mut bytes = Vec::new();
        let read = child
            .stdout
            .take()
            .unwrap()
            .take(MAX_LIST + 1)
            .read_to_end(&mut bytes);
        if read.is_err() || bytes.len() as u64 > MAX_LIST {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error(
                "inventário Git excedeu o limite ou não pôde ser lido",
            ));
        }
        if !child.wait().is_ok_and(|s| s.success()) {
            return Err(error("Git não pôde ler a revisão ou o inventário local"));
        }
        Ok(bytes)
    }

    pub fn commit(&self, base: &str) -> Result<String, CliError> {
        // Plain refs only; no revision expressions, reflog syntax, or option-like names.
        if base.is_empty()
            || base.starts_with('-')
            || base.contains("..")
            || base.len() > 256
            || !base
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/_.-".contains(&b))
        {
            return Err(CliError::InvalidArgument(
                "--base requer um SHA ou ref Git simples".into(),
            ));
        }
        let bytes = self.list(&[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{base}^{{commit}}"),
        ])?;
        let id = String::from_utf8_lossy(&bytes).trim().to_owned();
        if !oid(&id) {
            return Err(error("Git retornou um ID inválido"));
        }
        Ok(id)
    }

    pub fn tree(&self, commit: &str) -> Result<Inventory, CliError> {
        Inventory::parse(&self.list(&["ls-tree", "-r", "-z", commit])?, false)
    }

    pub fn index(&self) -> Result<Inventory, CliError> {
        Inventory::parse(&self.list(&["ls-files", "--stage", "-z"])?, true)
    }

    pub fn others(&self) -> Result<(BTreeSet<String>, bool), CliError> {
        let bytes = self.list(&["ls-files", "--others", "--exclude-standard", "-z"])?;
        let mut paths = BTreeSet::new();
        let mut incomplete = false;
        for row in bytes.split(|&b| b == 0).filter(|s| !s.is_empty()) {
            if let Some(path) = valid_path(row) {
                paths.insert(path.to_owned());
            } else {
                incomplete = true;
            }
        }
        Ok((paths, incomplete))
    }

    pub fn blobs(&self) -> Result<Blobs, CliError> {
        let mut child = self
            .command(&["cat-file", "--batch-command"])
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|_| error("falha ao iniciar leitura de objetos"))?;
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        Ok(Blobs {
            child,
            input,
            output,
        })
    }
}

fn valid_path(bytes: &[u8]) -> Option<&str> {
    std::str::from_utf8(bytes).ok().filter(|p| {
        *p != "."
            && relative(p)
            && Path::new(p)
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
    })
}

#[derive(Default)]
pub(super) struct Inventory {
    pub entries: BTreeMap<String, Entry>,
    pub unknown: BTreeSet<String>,
    pub incomplete: bool,
}

impl Inventory {
    fn parse(bytes: &[u8], index: bool) -> Result<Self, CliError> {
        let mut result = Self::default();
        for row in bytes.split(|&b| b == 0).filter(|s| !s.is_empty()) {
            let (header, path) = row.split_at(
                row.iter()
                    .position(|&b| b == b'\t')
                    .ok_or_else(|| error("inventário Git inválido"))?,
            );
            let Some(path) = valid_path(&path[1..]) else {
                result.incomplete = true;
                continue;
            };
            let header =
                std::str::from_utf8(header).map_err(|_| error("inventário Git inválido"))?;
            let parts: Vec<_> = header.split(' ').collect();
            if parts.len() != 3 {
                return Err(error("inventário Git inválido"));
            }
            if index && parts[2] != "0" {
                result.unknown.insert(path.to_owned());
                continue;
            }
            let id = parts[if index { 1 } else { 2 }];
            if !oid(id) {
                return Err(error("ID de objeto Git inválido"));
            }
            result.entries.insert(
                path.to_owned(),
                Entry {
                    mode: parts[0].to_owned(),
                    oid: id.to_owned(),
                },
            );
        }
        Ok(result)
    }
}

pub(super) struct Blobs {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl Blobs {
    pub fn read(
        &mut self,
        entry: &Entry,
        remaining: &mut usize,
    ) -> Result<Option<Vec<u8>>, CliError> {
        if !matches!(entry.mode.as_str(), "100644" | "100755") {
            return Ok(None);
        }
        self.read_blob(entry, remaining)
            .map_err(|_| error("objeto Git indisponível ou inválido"))
    }

    fn read_blob(
        &mut self,
        entry: &Entry,
        remaining: &mut usize,
    ) -> std::io::Result<Option<Vec<u8>>> {
        use std::io::Error;
        let Some(size) = self.request("info", entry)? else {
            return Ok(None);
        };
        if size > MAX_FILE || size > *remaining {
            return Ok(None);
        }
        if self.request("contents", entry)? != Some(size) {
            return Err(Error::other("invalid size"));
        }
        let mut data = vec![0; size];
        self.output.read_exact(&mut data)?;
        let mut end = [0];
        self.output.read_exact(&mut end)?;
        if end != [b'\n'] {
            return Err(Error::other("invalid delimiter"));
        }
        *remaining -= size;
        Ok(Some(data))
    }

    fn request(&mut self, verb: &str, entry: &Entry) -> std::io::Result<Option<usize>> {
        use std::io::Error;
        writeln!(self.input, "{verb} {}", entry.oid)?;
        self.input.flush()?;
        let mut header = String::new();
        self.output.by_ref().take(128).read_line(&mut header)?;
        let mut parts = header.split_ascii_whitespace();
        if parts.next() != Some(entry.oid.as_str()) || parts.next() != Some("blob") {
            return Err(Error::other("not a blob"));
        }
        parts
            .next()
            .and_then(|n| n.parse().ok())
            .map(Some)
            .ok_or_else(|| Error::other("invalid size"))
    }
}

impl Drop for Blobs {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
