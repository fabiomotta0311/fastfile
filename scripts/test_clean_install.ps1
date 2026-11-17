<#
.SYNOPSIS
    Teste automatizado de instalação em máquina limpa (Clean Machine Install & Uninstall).
    Valida empacotamento, integridade do ZIP, instalação, execução de CLI e desinstalação.
#>

param(
    [string]$SourceDir = ""
)

$ErrorActionPreference = "Stop"

Write-Host "=== Teste de Instalação e Desinstalação Limpa do FastFile ===" -ForegroundColor Cyan

$RepoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $RepoRoot

# 1. Determina diretório de binários
if (-not $SourceDir) {
    if (Test-Path "$RepoRoot\target\release\ff.exe") {
        $SourceDir = "$RepoRoot\target\release"
    } elseif (Test-Path "$RepoRoot\target\debug\ff.exe") {
        $SourceDir = "$RepoRoot\target\debug"
    } else {
        Write-Host "Compilando binários para teste..." -ForegroundColor Yellow
        cargo build --workspace
        $SourceDir = "$RepoRoot\target\debug"
    }
}

Write-Host "[1/6] Usando binários de: $SourceDir" -ForegroundColor Green

# 2. Monta pasta de staging e cria o pacote ZIP de distribuição
$TestTemp = Join-Path $env:TEMP "ff_clean_test_$([System.Guid]::NewGuid().ToString('N'))"
$StageDir = Join-Path $TestTemp "stage"
$ExtractDir = Join-Path $TestTemp "extract"
$InstallDir = Join-Path $TestTemp "installed"

New-Item -ItemType Directory -Path $StageDir -Force | Out-Null

$Bins = @("ff.exe", "fastfile.exe", "ff-ui.exe", "ff-indexer.exe")
foreach ($b in $Bins) {
    $src = Join-Path $SourceDir $b
    if (-not (Test-Path $src)) {
        Write-Error "Binário necessário não encontrado: $src"
        exit 1
    }
    Copy-Item $src (Join-Path $StageDir $b) -Force
}

# Copia scripts, metadados e ícone
New-Item -ItemType Directory -Path (Join-Path $StageDir "scripts") -Force | Out-Null
Copy-Item "$RepoRoot\scripts\install.ps1" (Join-Path $StageDir "scripts\install.ps1") -Force
Copy-Item "$RepoRoot\scripts\uninstall.ps1" (Join-Path $StageDir "scripts\uninstall.ps1") -Force
Copy-Item "$RepoRoot\install.ps1" (Join-Path $StageDir "install.ps1") -Force
Copy-Item "$RepoRoot\uninstall.ps1" (Join-Path $StageDir "uninstall.ps1") -Force
Copy-Item "$RepoRoot\assets\fastfile.ico" (Join-Path $StageDir "fastfile.ico") -Force
Copy-Item "$RepoRoot\README.md" (Join-Path $StageDir "README.md") -Force
Copy-Item "$RepoRoot\LICENSE-MIT" (Join-Path $StageDir "LICENSE-MIT") -Force

$ZipPath = Join-Path $TestTemp "fastfile-test-windows-x86_64.zip"
Write-Host "[2/6] Gerando pacote ZIP de distribuição em: $ZipPath" -ForegroundColor Yellow
Compress-Archive -Path "$StageDir\*" -DestinationPath $ZipPath -Force

# 3. Simula download e extração em máquina limpa
Write-Host "[3/6] Extraindo pacote ZIP simulando máquina limpa..." -ForegroundColor Yellow
New-Item -ItemType Directory -Path $ExtractDir -Force | Out-Null
Expand-Archive -Path $ZipPath -DestinationPath $ExtractDir -Force

# Validação: scripts/install.ps1 DEVE estar presente no pacote descompactado
$InstalledScript = Join-Path $ExtractDir "scripts\install.ps1"
if (-not (Test-Path $InstalledScript)) {
    Write-Error "FALHA CRÍTICA: scripts\install.ps1 NÃO está presente no pacote ZIP extraído!"
    exit 1
}
Write-Host "  -> scripts\install.ps1 confirmado no ZIP extraído!" -ForegroundColor Green

# 4. Executa a instalação a partir do pacote extraído
Write-Host "[4/6] Executando instalador do pacote extraído..." -ForegroundColor Yellow
& "$InstalledScript" -InstallDir $InstallDir -NoContextMenu -NoPath
if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne $null) {
    Write-Error "Falha ao executar scripts/install.ps1: código de saída $LASTEXITCODE"
    exit 1
}

# 5. Valida funcionamento dos binários instalados
Write-Host "[5/6] Validando executáveis no diretório instalado ($InstallDir)..." -ForegroundColor Yellow
$InstalledFf = Join-Path $InstallDir "ff.exe"
$InstalledFastfile = Join-Path $InstallDir "fastfile.exe"

if (-not (Test-Path $InstalledFf) -or -not (Test-Path $InstalledFastfile)) {
    Write-Error "Binários não foram copiados para $InstallDir!"
    exit 1
}

$versionOutput = & $InstalledFf --version
Write-Host "  -> ff --version output: $versionOutput" -ForegroundColor Green

$jsonOutput = & $InstalledFf find . --limit 1 --compact
Write-Host "  -> ff find compact output: $jsonOutput" -ForegroundColor Green
if ($jsonOutput -notmatch '"schema"\s*:\s*"ff/1"') {
    Write-Error "Resposta JSON do fastfile instalado não contém schema ff/1!"
    exit 1
}

# 6. Valida desinstalação completa
Write-Host "[6/6] Executando desinstalação..." -ForegroundColor Yellow
$InstalledUninstall = Join-Path $InstallDir "uninstall.ps1"
if (Test-Path $InstalledUninstall) {
    & $InstalledUninstall -InstallDir $InstallDir
} else {
    & "$ExtractDir\scripts\uninstall.ps1" -InstallDir $InstallDir
}

Start-Sleep -Seconds 1
if (Test-Path (Join-Path $InstallDir "ff.exe")) {
    Write-Error "Desinstalação falhou: ff.exe ainda existe em $InstallDir!"
    exit 1
}
Write-Host "  -> Desinstalação limpa verificada com sucesso!" -ForegroundColor Green

# Limpeza dos diretórios de teste temporários
Remove-Item -Path $TestTemp -Recurse -Force -ErrorAction SilentlyContinue
Write-Host "`n=== Teste de Clean Install & Uninstall concluído com SUCESSO! ===" -ForegroundColor Green
