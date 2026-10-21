# Termbook: make a licence key for a school that has paid. Run it every payment.
# Native tools write notes to stderr; every result is checked by hand below.
$ErrorActionPreference = 'Continue'
$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$keys = Join-Path $env:USERPROFILE 'TermbookKeys'
$tool = Join-Path $repo 'tools\termbook-licence\Cargo.toml'
$lic = Join-Path $keys 'termbook-issuer.key'

function Say($text, $colour = 'Gray') { Write-Host " $text" -ForegroundColor $colour }

if (-not (Test-Path $lic)) {
    Say 'Your licence key is not on this computer.' 'Red'
    Say "Run '1 Set up my keys (once).bat' first, or copy the TermbookKeys folder back from your USB disk to:"
    Say "  $keys"
    exit 1
}

Write-Host ''
Say 'TERMBOOK: LICENCE KEY FOR A SCHOOL' 'Green'
Say 'The school code is on the school''s Termbook: Settings, Subscription.'
Write-Host ''
$school = ''
while ($true) {
    $school = (Read-Host ' School code (like 9F3AC-2B1E0)').Trim()
    if ($school -match '^[0-9A-Za-z-]{6,20}$') { break }
    Say 'That does not look like a school code. Copy it exactly from the school.' 'Yellow'
}
do { $name = (Read-Host ' School name').Trim() } while ($name -eq '')
do { $plan = (Read-Host ' Plan: type term or year').Trim().ToLower() } while ($plan -notin 'term', 'year')
$today = Get-Date -Format 'yyyy-MM-dd'
$from = (Read-Host " Starts on (press Enter for today, $today)").Trim()
if ($from -eq '') { $from = $today }
while ($from -notmatch '^\d{4}-\d{2}-\d{2}$') { $from = (Read-Host ' Type the date like 2026-09-01').Trim() }
$ref = (Read-Host ' Payment reference (or press Enter)').Trim()

Write-Host ''
Say "$name, $plan plan from $from. Press Enter to make the key, or close this window to stop."
[void](Read-Host)

$cmd = @('run', '--release', '--quiet', '--manifest-path', $tool, '--', 'issue', '--key', $lic, '--school', $school, '--name', $name, '--plan', $plan, '--from', $from)
if ($ref -ne '') { $cmd += @('--id', $ref) }
Push-Location $keys
try { $out = & cargo @cmd 2>&1 | Out-String } finally { Pop-Location }
if ($LASTEXITCODE -ne 0) { Say $out 'Red'; Say 'No key was made.' 'Red'; exit 1 }

$key = ($out -split "`r?`n" | Where-Object { $_ -like 'TBK1.*' } | Select-Object -First 1).Trim()
Set-Clipboard -Value $key
Write-Host ''
Say $out
Say 'The key is COPIED. Paste it into WhatsApp or email to the school.' 'Green'
Say 'They paste it into Termbook: Settings, Subscription, Install key.'
Say "It is also written in your sales record: $keys\issued-keys.csv"
