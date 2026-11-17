<#
.SYNOPSIS
  Build, sign and (optionally) publish a WACH release on GitHub.

.DESCRIPTION
  1. Sets the version in tauri.conf.json and Cargo.toml.
  2. Runs the unit tests.
  3. Builds the NSIS installer, signed with the updater key
     (%USERPROFILE%\.tauri\wach-updater.key; a git-ignored copy sits in
     the repo folder; never commit it).
  4. Writes latest.json, the file installed copies of WACH read to find updates.
  Everything lands in release\<version>\ (ignored by git).

  With -Publish it also commits the version bump, tags v<version>, pushes, and
  creates the GitHub release with the installer, its .sig and latest.json.
  Without -Publish nothing leaves this PC.

.EXAMPLE
  .\scripts\release.ps1 -Version 0.2.1 -Notes "Fixes the tray menu"
  .\scripts\release.ps1 -Version 0.2.1 -Notes "Fixes the tray menu" -Publish
#>
param(
  [Parameter(Mandatory)] [ValidatePattern('^\d+\.\d+\.\d+$')] [string] $Version,
  [string] $Notes = "",
  [switch] $Publish,
  [string] $Repo = "vincentrathbone-web/wach",
  [string] $Key = "$env:USERPROFILE\.tauri\wach-updater.key"
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$src = Join-Path $root 'src-tauri'

# Tools (see HANDOVER.md).
$env:CARGO_HOME = 'D:\Dev\toolshed\cargo'
$env:RUSTUP_HOME = 'D:\Dev\toolshed\rustup'
$env:PATH = "D:\Dev\toolshed\cargo\bin;D:\Dev\toolshed\nsis;$env:PATH"
$tauri = 'D:\Dev\toolshed\tauri-cli\node_modules\.bin\tauri.cmd'

if (-not (Test-Path $Key)) { throw "Updater key not found: $Key" }
if ($Publish) {
  if (git -C $root status --porcelain) { throw "Commit or stash your changes first." }
  if ((git -C $root branch --show-current) -ne 'main') { throw "Publish from main." }
  gh auth status 2>$null | Out-Null
  if ($LASTEXITCODE -ne 0) { throw "gh is not signed in." }
}
if (Get-Process wach -ErrorAction SilentlyContinue) { throw "Quit WACH first (the build replaces wach.exe)." }

# 1. Version
$conf = Join-Path $src 'tauri.conf.json'
$text = [IO.File]::ReadAllText($conf) -replace '"version": "\d+\.\d+\.\d+"', "`"version`": `"$Version`""
[IO.File]::WriteAllText($conf, $text)
$cargo = Join-Path $src 'Cargo.toml'
$text = [IO.File]::ReadAllText($cargo) -replace '(?m)^version = "\d+\.\d+\.\d+"', "version = `"$Version`""
[IO.File]::WriteAllText($cargo, $text)

Push-Location $src
try {
  # 2. Tests
  cargo test
  if ($LASTEXITCODE -ne 0) { throw "Tests failed." }

  # 3. Signed build
  $env:TAURI_SIGNING_PRIVATE_KEY = $Key
  $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ''
  & $tauri build
  if ($LASTEXITCODE -ne 0) { throw "Build failed." }
} finally {
  Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY, Env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD -ErrorAction SilentlyContinue
  Pop-Location
}

$name = "WACH_${Version}_x64-setup.exe"
$bundle = Join-Path $src "target\release\bundle\nsis"
$out = Join-Path $root "release\$Version"
New-Item -ItemType Directory -Force $out | Out-Null
Copy-Item (Join-Path $bundle $name), (Join-Path $bundle "$name.sig") $out -Force

# 4. latest.json (format: Tauri v2 updater, static JSON)
$manifest = [ordered]@{
  version   = $Version
  notes     = $Notes
  pub_date  = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
  platforms = [ordered]@{
    'windows-x86_64' = [ordered]@{
      signature = (Get-Content (Join-Path $out "$name.sig") -Raw).Trim()
      url       = "https://github.com/$Repo/releases/download/v$Version/$name"
    }
  }
}
$manifest | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $out 'latest.json') -Encoding utf8NoBOM
"Built: $out"
Get-ChildItem $out | ForEach-Object { "  $($_.Name)  $([math]::Round($_.Length / 1KB)) KB" }

if (-not $Publish) {
  "Not published (run again with -Publish)."
  return
}

# 5. Publish
git -C $root add src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock
git -C $root commit -m "Release $Version"
git -C $root tag "v$Version"
git -C $root push origin main "v$Version"
gh release create "v$Version" --repo $Repo --title "WACH $Version" --notes $(if ($Notes) { $Notes } else { "WACH $Version" }) `
  (Join-Path $out $name) (Join-Path $out "$name.sig") (Join-Path $out 'latest.json')
"Published: https://github.com/$Repo/releases/tag/v$Version"
