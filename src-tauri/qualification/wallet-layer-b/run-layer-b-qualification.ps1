param(
    [Parameter(Mandatory = $true)]
    [string]$BinaryPath,
    [Parameter(Mandatory = $true)]
    [string]$EvidenceDirectory,
    [int]$CaseTimeoutSeconds = 60
)

$ErrorActionPreference = 'Stop'
$binary = (Resolve-Path -LiteralPath $BinaryPath).Path
$evidence = [System.IO.Path]::GetFullPath($EvidenceDirectory)
$repository = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path
if ($CaseTimeoutSeconds -lt 10 -or $CaseTimeoutSeconds -gt 300) {
    throw 'Case timeout must be between 10 and 300 seconds.'
}
$repositoryPrefix = $repository.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
if ($evidence.StartsWith($repositoryPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw 'Evidence must be written outside the reviewed repository.'
}
$commit = (& git -C $repository rev-parse HEAD).Trim()
$tree = (& git -C $repository rev-parse 'HEAD^{tree}').Trim()
$parent = (& git -C $repository rev-parse 'HEAD^').Trim()
$worktreeStatus = (& git -C $repository status --porcelain=v1) -join "`n"
if ($LASTEXITCODE -ne 0 -or $worktreeStatus.Length -ne 0) {
    throw 'Qualification requires the exact clean reviewed worktree.'
}
if (Test-Path -LiteralPath $evidence) {
    throw 'Evidence directory already exists; qualification never overwrites evidence.'
}
[System.IO.Directory]::CreateDirectory($evidence) | Out-Null

$commands = @(
    'wallet_get_status',
    'wallet_select_recovery_destination',
    'wallet_create',
    'wallet_select_recovery_source',
    'wallet_restore',
    'wallet_unlock',
    'wallet_lock'
)
$routes = @('official-invoke', 'internals-invoke', 'internals-ipc')
$duplicateFamilies = @(
    'identical',
    'conflicting',
    'valid-then-malformed',
    'malformed-then-valid',
    'public-then-secret-like',
    'exact-and-wrong-case',
    'three-repeated',
    'escaped-equivalent',
    'bounded-whitespace'
)
$representations = @('string', 'bytes', 'object-normalized')

$cases = [System.Collections.Generic.List[string]]::new()
foreach ($scenario in @(
    'window-other-local',
    'window-remote-origin',
    'window-recreated-main',
    'window-reloaded-generation',
    'window-destruction-race',
    'window-revocation-race'
)) {
    $cases.Add("$scenario--official-invoke")
}
foreach ($command in $commands) { $cases.Add("exact-$command--official-invoke") }
foreach ($route in @('internals-invoke', 'internals-ipc')) {
    $cases.Add("exact-status-$route--$route")
}
foreach ($id in @('raw-empty', 'raw-json-looking', 'raw-arbitrary', 'raw-bytes')) {
    $cases.Add("$id--official-invoke")
}
foreach ($id in @(
    'json-null',
    'json-boolean',
    'json-number',
    'json-string',
    'json-array',
    'extra-top-level',
    'wrong-case-top-level',
    'secret-like-top-level',
    'key-name-canary-top-level',
    'oversized-key-top-level',
    'excessive-key-count-top-level'
)) {
    $cases.Add("$id--official-invoke")
}
foreach ($id in @(
    'create-empty',
    'create-request-empty',
    'create-request-wrong-type',
    'create-unknown-field',
    'create-secret-like-field',
    'create-invalid-handle',
    'restore-wrong-handle-name',
    'unknown-command'
)) {
    $cases.Add("$id--official-invoke")
}
foreach ($family in $duplicateFamilies) {
    foreach ($representation in $representations) {
        foreach ($route in $routes) {
            $cases.Add("duplicate-$family-$representation--$route")
            $cases.Add("nested-duplicate-$family-$representation--$route")
        }
    }
}
foreach ($index in 0..7) { $cases.Add("concurrent-$index--official-invoke") }
$cases.Add('direct-fetch-text-missing-invoke-key--direct-fetch-text')
$cases.Add('direct-fetch-bytes-missing-invoke-key--direct-fetch-bytes')
$cases.Add('direct-xhr-missing-invoke-key--direct-xhr')
$cases.Add('forced-post-message-fallback--internals-post-message')
foreach ($point in @('metadata', 'body', 'response', 'observation')) {
    $cases.Add("contained-$point-panic--internals-post-message")
}
$cases.Add('contained-fixed-error-panic--internals-post-message')

$records = [System.Collections.Generic.List[object]]::new()
$overall = 'Passed'
foreach ($case in $cases) {
    $safeName = $case -replace '[^a-z0-9-]', '-'
    $stdoutPath = Join-Path $evidence "$safeName.stdout.log"
    $stderrPath = Join-Path $evidence "$safeName.stderr.log"
    $startedUtc = [DateTimeOffset]::UtcNow.ToString('O')
    $process = Start-Process -FilePath $binary -ArgumentList @(
        '--wallet-layer-b-qualification',
        "--wallet-layer-b-case=$case"
    ) -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath -PassThru -WindowStyle Hidden
    $timedOut = -not $process.WaitForExit($CaseTimeoutSeconds * 1000)
    if ($timedOut) {
        $process.Kill($true)
        $process.WaitForExit()
    }
    $endedUtc = [DateTimeOffset]::UtcNow.ToString('O')
    $exitCode = if ($timedOut) { $null } else { $process.ExitCode }
    $classification = if ($timedOut) {
        'Inconclusive'
    } elseif ($exitCode -eq 0) {
        'Passed'
    } else {
        'Failed'
    }
    if ($classification -eq 'Failed') { $overall = 'Failed' }
    if ($classification -eq 'Inconclusive' -and $overall -eq 'Passed') { $overall = 'Inconclusive' }
    $records.Add([ordered]@{
        case = $case
        command = "$binary --wallet-layer-b-qualification --wallet-layer-b-case=$case"
        started_utc = $startedUtc
        ended_utc = $endedUtc
        exit_code = $exitCode
        timed_out = $timedOut
        classification = $classification
        stdout_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $stdoutPath).Hash
        stderr_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $stderrPath).Hash
    })
}

$manifest = [ordered]@{
    marker = 'vision_wallet_layer_b_primary_evidence'
    result = $overall
    binary_path = $binary
    binary_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $binary).Hash
    implementation_commit = $commit
    implementation_tree = $tree
    implementation_parent = $parent
    cargo_lock_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $repository 'src-tauri\Cargo.lock')).Hash
    operator = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name
    operating_system = [System.Environment]::OSVersion.VersionString
    architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    screenshots = @()
    case_timeout_seconds = $CaseTimeoutSeconds
    total = $records.Count
    cases = $records
}
$manifestPath = Join-Path $evidence 'layer-b-evidence-manifest.json'
$manifest | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $manifestPath -Encoding utf8NoBOM
$manifestHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $manifestPath).Hash
Write-Output "Layer B result: $overall"
Write-Output "Evidence manifest: $manifestPath"
Write-Output "Evidence manifest SHA-256: $manifestHash"
if ($overall -ne 'Passed') { exit 2 }
