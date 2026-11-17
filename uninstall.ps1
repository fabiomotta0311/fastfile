<#
.SYNOPSIS
    Wrapper raiz do desinstalador FastFile para Windows.
    Executa scripts/uninstall.ps1 repassando todos os parâmetros.
#>
param(
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\FastFile",
    [switch]$RemoveSnapshots
)

& "$PSScriptRoot\scripts\uninstall.ps1" @PSBoundParameters
