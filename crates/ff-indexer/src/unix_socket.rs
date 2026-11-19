//! Socket Unix privado: nunca existe, nem por um instante, com permissões mais amplas que `0o600`.
//!
//! Fazer `bind` direto no caminho final e só depois `chmod` deixa uma janela em que o socket tem as
//! permissões do `umask`; e `remove_file` + `bind` em `/tmp` permite disputa com outro usuário. Aqui o
//! socket é criado dentro de um diretório de preparo `0o700`, restringido a `0o600` e só então movido
//! (`rename`, atômico) para o caminho final. Se o caminho final pertencer a outro usuário em um
//! diretório com *sticky bit*, o `rename` falha em vez de sobrescrever (falha fechada).

use std::io;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::net::UnixListener;
use std::path::Path;

/// Cria um `UnixListener` em `path`, acessível somente ao usuário atual desde o primeiro instante.
pub fn bind_private(path: &Path) -> io::Result<UnixListener> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidInput, "caminho de socket inválido");
    let name = path.file_name().ok_or_else(invalid)?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    let staging = parent.join(format!(
        ".{}.{}.d",
        name.to_string_lossy(),
        std::process::id()
    ));
    // Resíduo de uma execução anterior com o mesmo PID: remove sem seguir links simbólicos.
    match std::fs::symlink_metadata(&staging) {
        Ok(m) if m.is_dir() => std::fs::remove_dir_all(&staging)?,
        Ok(_) => std::fs::remove_file(&staging)?,
        Err(_) => {}
    }
    // O modo é aplicado na criação (limitado pelo umask, que só pode restringir mais).
    std::fs::DirBuilder::new().mode(0o700).create(&staging)?;

    let result = (|| {
        let tmp = staging.join("s");
        let listener = UnixListener::bind(&tmp)?;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        std::fs::rename(&tmp, path)?;
        Ok(listener)
    })();
    let _ = std::fs::remove_dir_all(&staging);
    result
}
