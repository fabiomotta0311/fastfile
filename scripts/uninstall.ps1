<#
.SYNOPSIS
    Script de desinstalação completa do FastFile para Windows (Issue #18).
.DESCRIPTION
    Remove serviços do Windows, menus de contexto, atalhos, variáveis de ambiente e arquivos
    instalados do FastFile.
.PARAMETER InstallDir
    Diretório onde o FastFile está instalado. Padrão: $env:LOCALAPPDATA\Programs\FastFile
.PARAMETER RemoveSnapshots
    Remove também caches de snapshot do FastFile em %LOCALAPPDATA%\FastFile.
#>

param(
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\FastFile",
    [switch]$RemoveSnapshots
)

$ErrorActionPreference = "Continue"

Write-Host "=== FastFile — Desinstalador Windows ===" -ForegroundColor Yellow
Write-Host "Removendo de: $InstallDir"

# 1. Parar e desinstalar serviço do Windows se ativo
$IndexerExe = Join-Path $InstallDir "ff-indexer.exe"
if (Test-Path $IndexerExe) {
    Write-Host "`n[1/5] Parando e removendo serviço do Windows..." -ForegroundColor Yellow
    & $IndexerExe service stop | Out-Null
    & $IndexerExe service uninstall | Out-Null
} else {
    Write-Host "`n[1/5] Executável do serviço não encontrado, tentando sc.exe diretamente..." -ForegroundColor Gray
    sc.exe stop FastFileIndexer | Out-Null
    sc.exe delete FastFileIndexer | Out-Null
}

# 2. Desregistrar menu de contexto do Explorer
$FfExe = Join-Path $InstallDir "ff.exe"
Write-Host "`n[2/5] Removendo menus de contexto do Windows Explorer..." -ForegroundColor Yellow
if (Test-Path $FfExe) {
    & $FfExe unregister-context-menu | Out-Null
} else {
    reg delete "HKCU\Software\Classes\Directory\shell\FastFile" /f | Out-Null
    reg delete "HKCU\Software\Classes\Directory\Background\shell\FastFile" /f | Out-Null
    reg delete "HKCU\Software\Classes\*\shell\FastFileCompare" /f | Out-Null
}
Write-Host "  -> Menus de contexto removidos." -ForegroundColor Green

# 3. Remover atalho do Menu Iniciar e Desktop
Write-Host "`n[3/5] Removendo atalhos..." -ForegroundColor Yellow
$StartShortcut = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\FastFile.lnk"
if (Test-Path $StartShortcut) {
    Remove-Item -Path $StartShortcut -Force
    Write-Host "  -> Atalho do Menu Iniciar removido." -ForegroundColor Green
}

$DesktopShortcut = Join-Path ([Environment]::GetFolderPath("Desktop")) "FastFile.lnk"
if (Test-Path $DesktopShortcut) {
    Remove-Item -Path $DesktopShortcut -Force
    Write-Host "  -> Atalho da Área de Trabalho removido." -ForegroundColor Green
}

# 4. Remover do PATH do usuário
Write-Host "`n[4/5] Removendo do PATH do usuário..." -ForegroundColor Yellow
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath) {
    $parts = $userPath -split ';' | Where-Object { $_ -ne "" -and $_ -ne $InstallDir }
    $newPath = $parts -join ';'
    [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
    Write-Host "  -> $InstallDir removido da variável PATH." -ForegroundColor Green
}

# 5. Remover arquivos e diretório de instalação
Write-Host "`n[5/5] Removendo arquivos de instalação..." -ForegroundColor Yellow
if (Test-Path $InstallDir) {
    Remove-Item -Path $InstallDir -Recurse -Force
    Write-Host "  -> Diretório $InstallDir removido." -ForegroundColor Green
}

# 6. Opcional: remover dados de snapshot e cache
if ($RemoveSnapshots) {
    $DataDir = Join-Path $env:LOCALAPPDATA "FastFile"
    if (Test-Path $DataDir) {
        Remove-Item -Path $DataDir -Recurse -Force
        Write-Host "  -> Dados e snapshots removidos em $DataDir." -ForegroundColor Green
    }
}

Write-Host "`n=== Desinstalação concluída com sucesso! ===" -ForegroundColor Green
