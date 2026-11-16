//! `service`
//!
//! Gerenciamento do `ff-indexer` como Serviço do Windows e Systemd (Issue #18).

use std::env;
use std::path::PathBuf;

/// Nome padrão do serviço no Windows Service Control Manager.
pub const SERVICE_NAME: &str = "FastFileIndexer";
pub const SERVICE_DISPLAY_NAME: &str = "FastFile Indexer Service";
pub const SERVICE_DESCRIPTION: &str =
    "Serviço residente de indexação em memória ultra-rápida com IPC segura do FastFile.";

/// Resolve o caminho absoluto do executável `ff-indexer`.
pub fn resolve_indexer_exe() -> PathBuf {
    env::current_exe().unwrap_or_else(|_| PathBuf::from("ff-indexer.exe"))
}

/// Instala o indexador como serviço do Windows (SCM) ou gera unit file systemd.
pub fn install_service() -> Result<String, String> {
    #[cfg(windows)]
    {
        use std::process::Command;

        let exe_path = resolve_indexer_exe();
        let bin_path_arg = format!(r#"binPath= "\"{}\" --service""#, exe_path.to_string_lossy());
        let display_arg = format!(r#"DisplayName= "{SERVICE_DISPLAY_NAME}""#);

        let output = Command::new("sc.exe")
            .args([
                "create",
                SERVICE_NAME,
                &bin_path_arg,
                "start=",
                "auto",
                &display_arg,
            ])
            .output()
            .map_err(|e| format!("falha ao executar sc.exe create: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(format!(
                "erro ao criar serviço Windows: {}\n{}",
                stdout.trim(),
                stderr.trim()
            ));
        }

        // Define a descrição do serviço
        let _ = Command::new("sc.exe")
            .args(["description", SERVICE_NAME, SERVICE_DESCRIPTION])
            .output();

        Ok(format!(
            "Serviço '{SERVICE_NAME}' instalado com sucesso no Windows Service Control Manager."
        ))
    }

    #[cfg(not(windows))]
    {
        let unit_content = systemd_unit_file();
        Ok(format!(
            "Instalação no Linux concluída. Copie o arquivo unit para /etc/systemd/system/fastfile-indexer.service:\n\n{unit_content}"
        ))
    }
}

/// Remove o serviço do Windows (SCM).
pub fn uninstall_service() -> Result<String, String> {
    #[cfg(windows)]
    {
        use std::process::Command;

        // Tenta parar antes de remover
        let _ = Command::new("sc.exe").args(["stop", SERVICE_NAME]).output();

        let output = Command::new("sc.exe")
            .args(["delete", SERVICE_NAME])
            .output()
            .map_err(|e| format!("falha ao executar sc.exe delete: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(format!(
                "erro ao deletar serviço Windows: {}\n{}",
                stdout.trim(),
                stderr.trim()
            ));
        }

        Ok(format!(
            "Serviço '{SERVICE_NAME}' desinstalado e removido com sucesso."
        ))
    }

    #[cfg(not(windows))]
    {
        Ok("Para desinstalar no Linux, execute: systemctl stop fastfile-indexer && rm -f /etc/systemd/system/fastfile-indexer.service".to_string())
    }
}

/// Inicia o serviço pelo Windows Service Control Manager.
pub fn start_service() -> Result<String, String> {
    #[cfg(windows)]
    {
        use std::process::Command;

        let output = Command::new("sc.exe")
            .args(["start", SERVICE_NAME])
            .output()
            .map_err(|e| format!("falha ao executar sc.exe start: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(format!(
                "erro ao iniciar serviço Windows: {}\n{}",
                stdout.trim(),
                stderr.trim()
            ));
        }

        Ok(format!("Serviço '{SERVICE_NAME}' iniciado com sucesso."))
    }

    #[cfg(not(windows))]
    {
        Ok("No Linux, utilize: systemctl start fastfile-indexer".to_string())
    }
}

/// Para o serviço pelo Windows Service Control Manager.
pub fn stop_service() -> Result<String, String> {
    #[cfg(windows)]
    {
        use std::process::Command;

        let output = Command::new("sc.exe")
            .args(["stop", SERVICE_NAME])
            .output()
            .map_err(|e| format!("falha ao executar sc.exe stop: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(format!(
                "erro ao parar serviço Windows: {}\n{}",
                stdout.trim(),
                stderr.trim()
            ));
        }

        Ok(format!("Serviço '{SERVICE_NAME}' parado com sucesso."))
    }

    #[cfg(not(windows))]
    {
        Ok("No Linux, utilize: systemctl stop fastfile-indexer".to_string())
    }
}

/// Consulta o status atual do serviço no Windows Service Control Manager.
pub fn status_service() -> Result<String, String> {
    #[cfg(windows)]
    {
        use std::process::Command;

        let output = Command::new("sc.exe")
            .args(["query", SERVICE_NAME])
            .output()
            .map_err(|e| format!("falha ao executar sc.exe query: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            return Err(format!(
                "erro ao consultar status: {}\n{}",
                stdout.trim(),
                stderr.trim()
            ));
        }

        Ok(stdout.to_string())
    }

    #[cfg(not(windows))]
    {
        Ok("No Linux, utilize: systemctl status fastfile-indexer".to_string())
    }
}

/// Gera o conteúdo de unit file para o Systemd no Linux.
#[must_use]
pub fn systemd_unit_file() -> String {
    format!(
        r#"[Unit]
Description={SERVICE_DISPLAY_NAME}
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/ff-indexer / --snapshot /var/cache/fastfile/snapshot.bin
Restart=always
RestartSec=5
User=root

[Install]
WantedBy=multi-user.target
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_systemd_unit_content() {
        let unit = systemd_unit_file();
        assert!(unit.contains("Description="));
        assert!(unit.contains("ExecStart="));
        assert!(unit.contains("WantedBy="));
    }

    #[test]
    fn test_resolve_indexer_exe() {
        let exe = resolve_indexer_exe();
        assert!(!exe.as_os_str().is_empty());
    }
}
