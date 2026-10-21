# Termbook: set up the owner's two secret keys. Run it once.
# Safe to run again: it never replaces a key that already exists, it only
# puts the public halves back into the program.
# Native tools write notes to stderr; every result is checked by hand below.
$ErrorActionPreference = 'Continue'
$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$keys = Join-Path $env:USERPROFILE 'TermbookKeys'
$tool = Join-Path $repo 'tools\termbook-licence\Cargo.toml'
New-Item -ItemType Directory -Force -Path $keys | Out-Null

function Say($text, $colour = 'Gray') { Write-Host " $text" -ForegroundColor $colour }

Write-Host ''
Say 'TERMBOOK OWNER SETUP' 'Green'
Say "Your secret keys are kept in: $keys"
Write-Host ''

# A key made earlier inside the project folder moves to the keys folder.
foreach ($name in 'termbook-issuer.key', 'issued-keys.csv') {
    $old = Join-Path $repo "tools\termbook-licence\$name"
    $new = Join-Path $keys $name
    if ((Test-Path $old) -and -not (Test-Path $new)) { Move-Item $old $new; Say "Moved $name into the keys folder." }
}

# 1. The licence key: only it can make keys that unlock Termbook.
$lic = Join-Path $keys 'termbook-issuer.key'
if (Test-Path $lic) {
    Say '1. Licence key: you already have one. Keeping it.' 'Yellow'
} else {
    Say '1. Making your licence key (the first time takes a minute)...'
    & cargo run --release --quiet --manifest-path $tool -- keygen --key $lic | Out-Null
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $lic)) { throw 'The licence key could not be made. Is Rust installed?' }
    Say '   Done.' 'Green'
}
$pub = (& cargo run --release --quiet --manifest-path $tool -- pubkey --key $lic | Out-String).Trim()
if ($LASTEXITCODE -ne 0 -or $pub -notmatch '^[0-9a-f]{64}$') { throw 'Could not read the licence key.' }
$licRs = Join-Path $repo 'src-tauri\src\licence.rs'
$text = [IO.File]::ReadAllText($licRs)
$updated = [regex]::Replace($text, 'pub const ISSUER_PUBLIC_KEY_HEX: &str = "[0-9a-fA-F]*";', "pub const ISSUER_PUBLIC_KEY_HEX: &str = `"$pub`";")
if ($updated -ne $text) { [IO.File]::WriteAllText($licRs, $updated) }
Say '   Termbook will now ask schools for a licence key (after the 30-day trial).' 'Green'
Write-Host ''

# 2. The update key: only it can sign an update that schools will install.
$up = Join-Path $keys 'updater.key'
if (Test-Path $up) {
    Say '2. Update key: you already have one. Keeping it.' 'Yellow'
} else {
    Say '2. Making your update key...'
    Push-Location $repo
    try { & npx tauri signer generate --ci -w $up | Out-Null } finally { Pop-Location }
    if (-not (Test-Path "$up.pub")) { throw 'The update key could not be made. Run "npm install" in the project folder first.' }
    Say '   Done.' 'Green'
}
$upPub = (Get-Content "$up.pub" -Raw).Trim()
[IO.File]::WriteAllText((Join-Path $repo 'src-tauri\updater.pub'), $upPub)
$conf = Join-Path $repo 'src-tauri\tauri.conf.json'
$json = [IO.File]::ReadAllText($conf)
$json2 = [regex]::Replace($json, '"pubkey": "[^"]*"', "`"pubkey`": `"$upPub`"")
if ($json2 -ne $json) { [IO.File]::WriteAllText($conf, $json2) }
Say '   Termbook will now accept updates signed by you, and nobody else.' 'Green'
Write-Host ''

Say 'NOW, PLEASE:' 'Cyan'
Say '  a) Copy the whole TermbookKeys folder onto a USB flash disk and keep it somewhere safe.'
Say '     If this computer dies without that copy, you cannot make licence keys or updates again.'
Say '  b) Never put these files on a school computer, Google Drive, WhatsApp or email.'
Say '  c) In GitHub Desktop, commit the changed files ("Set my keys") and push.'
Say '  d) Build the installer again (npm run app:build). Copies built before today do not check licences.'
Write-Host ''
Start-Process explorer.exe $keys
