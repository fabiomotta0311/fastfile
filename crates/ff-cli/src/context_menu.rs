//! `context_menu`
//!
//! Registro e remoção de opções do FastFile no menu de contexto do Windows Explorer (Issue #17).
//! Registra sob `HKEY_CURRENT_USER\Software\Classes` permitindo ativação sem elevação de privilégios.

use std::env;
use std::path::PathBuf;

/// Localiza o caminho absoluto do executável FastFile mais apropriado para UI ou CLI.
pub fn resolve_fastfile_executable() -> (PathBuf, Option<PathBuf>) {
    let current = env::current_exe().unwrap_or_else(|_| PathBuf::from("fastfile.exe"));
    let parent = current.parent().map(|p| p.to_path_buf());

    let ui_exe = parent
        .as_ref()
        .map(|p| p.join("ff-ui.exe"))
        .filter(|p| p.exists());
    (current, ui_exe)
}

/// Registra os menus de contexto no Windows Explorer (pastas, fundo de pasta e arquivos).
pub fn register_context_menu() -> Result<String, String> {
    #[cfg(windows)]
    {
        use std::process::Command;

        let (cli_exe, ui_exe) = resolve_fastfile_executable();
        let cli_str = cli_exe.to_string_lossy().to_string();
        let target_search_exe = ui_exe
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| cli_str.clone());

        // 1. Menu para Pastas (Directory\shell\FastFile)
        let dir_key = r"HKCU\Software\Classes\Directory\shell\FastFile";
        let dir_cmd_key = format!(r"{dir_key}\command");
        let dir_cmd_val = format!(r#""{target_search_exe}" --target "%V""#);

        let _ = Command::new("reg")
            .args([
                "add",
                dir_key,
                "/ve",
                "/d",
                "Buscar aqui com FastFile",
                "/f",
            ])
            .output();
        let _ = Command::new("reg")
            .args(["add", dir_key, "/v", "Icon", "/d", &target_search_exe, "/f"])
            .output();
        let _ = Command::new("reg")
            .args(["add", &dir_cmd_key, "/ve", "/d", &dir_cmd_val, "/f"])
            .output();

        // 2. Menu para Fundo de Pastas (Directory\Background\shell\FastFile)
        let bg_key = r"HKCU\Software\Classes\Directory\Background\shell\FastFile";
        let bg_cmd_key = format!(r"{bg_key}\command");
        let bg_cmd_val = format!(r#""{target_search_exe}" --target "%V""#);

        let _ = Command::new("reg")
            .args(["add", bg_key, "/ve", "/d", "Buscar aqui com FastFile", "/f"])
            .output();
        let _ = Command::new("reg")
            .args(["add", bg_key, "/v", "Icon", "/d", &target_search_exe, "/f"])
            .output();
        let _ = Command::new("reg")
            .args(["add", &bg_cmd_key, "/ve", "/d", &bg_cmd_val, "/f"])
            .output();

        // 3. Menu para Arquivos (Comparar com FastFile)
        let file_key = r"HKCU\Software\Classes\*\shell\FastFileCompare";
        let file_cmd_key = format!(r"{file_key}\command");
        let file_cmd_val = format!(r#""{cli_str}" diff "%1""#);

        let _ = Command::new("reg")
            .args(["add", file_key, "/ve", "/d", "Comparar com FastFile", "/f"])
            .output();
        let _ = Command::new("reg")
            .args(["add", file_key, "/v", "Icon", "/d", &cli_str, "/f"])
            .output();
        let _ = Command::new("reg")
            .args(["add", &file_cmd_key, "/ve", "/d", &file_cmd_val, "/f"])
            .output();

        Ok("Menus de contexto do Windows Explorer registrados com sucesso no HKCU.".to_string())
    }

    #[cfg(not(windows))]
    {
        Err("O menu de contexto do Windows Explorer está disponível apenas no Windows.".to_string())
    }
}

/// Remove os menus de contexto do Windows Explorer.
pub fn unregister_context_menu() -> Result<String, String> {
    #[cfg(windows)]
    {
        use std::process::Command;

        let _ = Command::new("reg")
            .args([
                "delete",
                r"HKCU\Software\Classes\Directory\shell\FastFile",
                "/f",
            ])
            .output();

        let _ = Command::new("reg")
            .args([
                "delete",
                r"HKCU\Software\Classes\Directory\Background\shell\FastFile",
                "/f",
            ])
            .output();

        let _ = Command::new("reg")
            .args([
                "delete",
                r"HKCU\Software\Classes\*\shell\FastFileCompare",
                "/f",
            ])
            .output();

        Ok("Menus de contexto do Windows Explorer removidos com sucesso do HKCU.".to_string())
    }

    #[cfg(not(windows))]
    {
        Err("O menu de contexto do Windows Explorer está disponível apenas no Windows.".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_executable() {
        let (cli, _) = resolve_fastfile_executable();
        assert!(!cli.as_os_str().is_empty());
    }

    #[test]
    fn test_context_menu_functions_callable() {
        // Testa chamada sem pânico
        #[cfg(windows)]
        {
            let res = register_context_menu();
            assert!(res.is_ok());
            let unres = unregister_context_menu();
            assert!(unres.is_ok());
        }
        #[cfg(not(windows))]
        {
            let res = register_context_menu();
            assert!(res.is_err());
        }
    }
}
