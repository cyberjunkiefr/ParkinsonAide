# Construit l'installateur ParkinsonAide : compile l'app en release puis le script Inno Setup.
# Usage (depuis n'importe où) :  pwsh installer\build.ps1
# Résultat : target\installer\ParkinsonAide-Setup-<version>.exe
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
    foreach ($p in 'vosk\libvosk.dll', 'vosk_small_fr\am\final.mdl') {
        if (-not (Test-Path $p)) { throw "Fichier manquant : $p (voir README.md, section Setup)" }
    }

    $version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' |
        Select-Object -First 1).Matches[0].Groups[1].Value

    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build a échoué" }

    $iscc = @(
        (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source,
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
    ) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
    if (-not $iscc) { throw "Inno Setup 6 introuvable (winget install JRSoftware.InnoSetup)" }

    & $iscc /Q "/DAppVersion=$version" installer\ParkinsonAide.iss
    if ($LASTEXITCODE -ne 0) { throw "ISCC a échoué" }

    Get-Item "target\installer\ParkinsonAide-Setup-$version.exe"
}
finally {
    Pop-Location
}
