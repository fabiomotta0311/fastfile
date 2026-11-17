<#
.SYNOPSIS
    Wrapper raiz do instalador FastFile para Windows.
    Executa scripts/install.ps1 repassando todos os parâmetros.
#>
param(
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\FastFile",
    [switch]$RegisterService,
    [switch]$NoPath,
    [switch]$NoContextMenu,
    [switch]$BuildRelease
)

$targetScript = Join-Path $PSScriptRoot "scripts\install.ps1"
if (-not (Test-Path $targetScript)) {
    # Caso este script esteja no mesmo nível dos binários no ZIP
    $targetScript = Join-Path $PSScriptRoot "install.ps1"
}

& "$PSScriptRoot\scripts\install.ps1" @PSBoundParameters
