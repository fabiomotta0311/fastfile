<#
.SYNOPSIS
    Script de instalação e configuração do FastFile para Windows (Issue #18).
.DESCRIPTION
    Copia os binários (ff, fastfile, ff-ui, ff-indexer) para a pasta de programas do usuário,
    configura o PATH, registra menus de contexto no Windows Explorer e opcionalmente
    configura o indexador como Serviço do Windows.
.PARAMETER InstallDir
    Diretório de instalação de destino. Padrão: $env:LOCALAPPDATA\Programs\FastFile
.PARAMETER RegisterService
    Registra o daemon ff-indexer como Serviço do Windows.
.PARAMETER NoPath
    Não adiciona o diretório de instalação à variável de ambiente PATH do usuário.
.PARAMETER NoContextMenu
    Não registra os atalhos no menu de contexto do Windows Explorer.
.PARAMETER BuildRelease
    Compila o workspace em modo release (cargo build --release) antes de instalar.
#>

param(
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\FastFile",
    [switch]$RegisterService,
    [switch]$NoPath,
    [switch]$NoContextMenu,
    [switch]$BuildRelease
)

$ErrorActionPreference = "Stop"

Write-Host "=== FastFile — Instalador Windows ===" -ForegroundColor Cyan
Write-Host "Destino: $InstallDir"

$RepoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $RepoRoot

# 1. Compilação (se solicitada ou se os binários não existirem)
$ReleaseDir = Join-Path $RepoRoot "target\release"
$Bins = @("ff.exe", "fastfile.exe", "ff-ui.exe", "ff-indexer.exe")
$MissingBin = $false

foreach ($b in $Bins) {
    if (-not (Test-Path (Join-Path $ReleaseDir $b))) {
        $MissingBin = $true
        break
    }
}

if ($BuildRelease -or $MissingBin) {
    Write-Host "`n[1/5] Compilando binários em modo release..." -ForegroundColor Yellow
    cargo build --release --workspace
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Falha na compilação do FastFile."
        exit 1
    }
} else {
    Write-Host "`n[1/5] Binários release encontrados em $ReleaseDir." -ForegroundColor Green
}

# 2. Criação do diretório de instalação e cópia dos binários
Write-Host "`n[2/5] Instalando binários em $InstallDir..." -ForegroundColor Yellow
if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

foreach ($b in $Bins) {
    $src = Join-Path $ReleaseDir $b
    if (Test-Path $src) {
        $dest = Join-Path $InstallDir $b
        try {
            Copy-Item -Path $src -Destination $dest -Force -ErrorAction Stop
        } catch {
            # Se o processo estiver em execução no Windows, renomeia para .old e instala o novo
            $old = "$dest.old"
            Remove-Item $old -Force -ErrorAction SilentlyContinue
            Move-Item $dest $old -Force -ErrorAction SilentlyContinue
            Copy-Item -Path $src -Destination $dest -Force
        }
        Write-Host "  -> Instalado: $b"
    } else {
        Write-Warning "Binário não encontrado: $src"
    }
}

# Cria wrapper fastfile.cmd para unificar CLI e GUI no CMD
$cmdContent = @"
@echo off
if "%~1"=="" (
    start "" "%~dp0fastfile.exe"
) else (
    "%~dp0ff.exe" %*
)
"@
Set-Content -Path (Join-Path $InstallDir "fastfile.cmd") -Value $cmdContent -Force

# 3. Adicionar ao PATH do usuário
if (-not $NoPath) {
    Write-Host "`n[3/5] Configurando variável PATH do usuário..." -ForegroundColor Yellow
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if (-not $userPath) { $userPath = "" }
    $parts = $userPath -split ';' | Where-Object { $_ -ne "" }

    if ($parts -notcontains $InstallDir) {
        $newPath = ($parts + $InstallDir) -join ';'
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        $env:Path = "$env:Path;$InstallDir"
        Write-Host "  -> $InstallDir adicionado ao PATH do usuário." -ForegroundColor Green
    } else {
        Write-Host "  -> $InstallDir já está presente no PATH." -ForegroundColor Gray
    }

    # Registra App Paths no Registro do Windows para execução instantânea via Win + R
    try {
        $appPathsKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\App Paths"
        foreach ($name in @("fastfile.exe", "ff.exe", "ff-ui.exe")) {
            $key = Join-Path $appPathsKey $name
            if (-not (Test-Path $key)) { New-Item -Path $key -Force | Out-Null }
            $targetExe = if ($name -eq "ff.exe") { Join-Path $InstallDir "ff.exe" } else { Join-Path $InstallDir "fastfile.exe" }
            Set-ItemProperty -Path $key -Name "(Default)" -Value $targetExe
            Set-ItemProperty -Path $key -Name "Path" -Value $InstallDir
        }
        Write-Host "  -> Registrado em 'App Paths' (disponível via Win + R: 'fastfile' ou 'ff')." -ForegroundColor Green
    } catch {}

    # Notifica o shell do Windows sobre a atualização de ambiente
    try {
        $sig = @'
[DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Auto)]
public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);
'@
        $type = Add-Type -MemberDefinition $sig -Name NativeEnvBroadcast -Namespace Win32Env -PassThru
        $HWND_BROADCAST = [IntPtr]0xffff
        $WM_SETTINGCHANGE = 0x001A
        $res = [UIntPtr]::Zero
        $type::SendMessageTimeout($HWND_BROADCAST, $WM_SETTINGCHANGE, [UIntPtr]::Zero, "Environment", 2, 2000, [ref]$res) | Out-Null
    } catch {}
} else {
    Write-Host "`n[3/5] Configuração de PATH ignorada (-NoPath)." -ForegroundColor Gray
}

# 4. Registro no menu de contexto do Explorer
if (-not $NoContextMenu) {
    Write-Host "`n[4/5] Registrando menus de contexto no Windows Explorer..." -ForegroundColor Yellow
    $ffExe = Join-Path $InstallDir "ff.exe"
    if (Test-Path $ffExe) {
        & $ffExe register-context-menu
        Write-Host "  -> Menus de contexto ativados." -ForegroundColor Green
    }
} else {
    Write-Host "`n[4/5] Menus de contexto ignorados (-NoContextMenu)." -ForegroundColor Gray
}

# 5. Atalho no Menu Iniciar
Write-Host "`n[5/5] Criando atalho no Menu Iniciar..." -ForegroundColor Yellow
$StartMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
$ShortcutPath = Join-Path $StartMenuDir "FastFile.lnk"
$UiExePath = Join-Path $InstallDir "fastfile.exe"

if (-not (Test-Path $UiExePath)) {
    $UiExePath = Join-Path $InstallDir "ff-ui.exe"
}

if (Test-Path $UiExePath) {
    try {
        $WshShell = New-Object -ComObject WScript.Shell
        $Shortcut = $WshShell.CreateShortcut($ShortcutPath)
        $Shortcut.TargetPath = $UiExePath
        $Shortcut.WorkingDirectory = $InstallDir
        $Shortcut.Description = "FastFile — Busca, Prévia, Diff e Uso de Disco Instantâneos"
        $Shortcut.Save()
        Write-Host "  -> Atalho criado: $ShortcutPath" -ForegroundColor Green
    } catch {
        Write-Warning "Não foi possível criar atalho no Menu Iniciar: $_"
    }
}

# 6. Opcional: Serviço do Windows
if ($RegisterService) {
    Write-Host "`n[Opcional] Registrando FastFile Indexer como Serviço do Windows..." -ForegroundColor Yellow
    $IndexerExe = Join-Path $InstallDir "ff-indexer.exe"
    if (Test-Path $IndexerExe) {
        & $IndexerExe service install
        & $IndexerExe service start
    }
}

Write-Host "`n=== Instalação concluída com sucesso! ===" -ForegroundColor Green
Write-Host "Comandos e acessos disponíveis:"
Write-Host "  Win + R -> digite 'fastfile' (abre a interface gráfica instantaneamente)" -ForegroundColor Green
Write-Host "  fastfile --help ou fastfile find <termo> (terminal / agentes)" -ForegroundColor Cyan
Write-Host "  ff --help ou ff find <termo> (CLI curta para terminal)" -ForegroundColor Cyan
Write-Host "  ff-ui (alias para a interface)" -ForegroundColor Gray
