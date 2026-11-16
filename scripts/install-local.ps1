$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    if (Get-Process -Name 'winagent' -ErrorAction SilentlyContinue) {
        throw 'Close WinAgent before building and installing the local version.'
    }

    $metadataJson = & cargo metadata --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Failed to resolve the Cargo target directory.' }
    $metadata = $metadataJson | ConvertFrom-Json
    $bundleDir = Join-Path $metadata.target_directory 'release/bundle/nsis'
    $startedAt = Get-Date

    & pnpm.cmd run build:local
    if ($LASTEXITCODE -ne 0) { throw 'Local build failed; installation was skipped.' }

    $installers = @(Get-ChildItem -LiteralPath $bundleDir -Filter '*-setup.exe' -File |
        Where-Object { $_.LastWriteTime -ge $startedAt })
    if ($installers.Count -ne 1) {
        throw "Expected one newly built NSIS installer in $bundleDir; found $($installers.Count)."
    }

    Write-Host "Installing $($installers[0].FullName)"
    $installer = Start-Process -FilePath $installers[0].FullName -ArgumentList '/S' -WindowStyle Hidden -Wait -PassThru
    if ($installer.ExitCode -ne 0) {
        throw "Installer failed with exit code $($installer.ExitCode)."
    }
    Write-Host 'WinAgent installation completed.'
} finally {
    Pop-Location
}
