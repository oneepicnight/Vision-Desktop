param(
    [Parameter(Mandatory = $true, ParameterSetName = 'Run')]
    [string]$BinaryPath,
    [Parameter(Mandatory = $true, ParameterSetName = 'Run')]
    [string]$EvidenceDirectory,
    [Parameter(Mandatory = $true, ParameterSetName = 'Run')]
    [string]$WalletCustodyRoot,
    [Parameter(Mandatory = $true, ParameterSetName = 'Run')]
    [string]$VisionCoreRoot,
    [Parameter(ParameterSetName = 'Run')]
    [int]$CaseTimeoutSeconds = 60,
    [Parameter(Mandatory = $true, ParameterSetName = 'SelfTest')]
    [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'
$MaximumTranscriptBytes = 1MB
$MaximumErrorTranscriptBytes = 256KB

function Get-StringSha256([string]$Value) {
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($Value)
        return ([System.BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-', '')
    } finally {
        $sha.Dispose()
    }
}

function Write-Utf8NoBom([string]$Path, [string[]]$Lines) {
    $encoding = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllLines($Path, $Lines, $encoding)
}

function Get-GitProof([string]$Root) {
    $commit = (& git -C $Root rev-parse HEAD).Trim()
    $tree = (& git -C $Root rev-parse 'HEAD^{tree}').Trim()
    $status = (& git -C $Root status --porcelain=v1 --untracked-files=all) -join "`n"
    if ($LASTEXITCODE -ne 0) { throw "Unable to inspect Git state for $Root" }
    return [ordered]@{
        commit = $commit
        tree = $tree
        clean = $status.Length -eq 0
        status_sha256 = Get-StringSha256 $status
    }
}

function Get-TreeFingerprint([string]$Root) {
    $absolute = [System.IO.Path]::GetFullPath($Root)
    if (-not (Test-Path -LiteralPath $absolute)) {
        return [ordered]@{ exists = $false; file_count = 0; total_bytes = 0; sha256 = Get-StringSha256 'absent' }
    }
    $rootItem = Get-Item -LiteralPath $absolute -Force
    if (($rootItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw 'Wallet custody root cannot be a reparse point.'
    }
    $items = @(Get-ChildItem -LiteralPath $absolute -File -Force -Recurse | Sort-Object FullName)
    $entries = [System.Collections.Generic.List[string]]::new()
    [long]$total = 0
    foreach ($item in $items) {
        if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw 'Wallet custody evidence cannot traverse a reparse point.'
        }
        $relative = [System.IO.Path]::GetRelativePath($absolute, $item.FullName)
        $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $item.FullName).Hash
        $entries.Add("$relative|$($item.Length)|$($item.LastWriteTimeUtc.Ticks)|$hash")
        $total += $item.Length
    }
    return [ordered]@{
        exists = $true
        file_count = $items.Count
        total_bytes = $total
        sha256 = Get-StringSha256 ($entries -join "`n")
    }
}

function Get-LockChecksum([string]$LockPath, [string]$Name, [string]$Version) {
    $blocks = (Get-Content -Raw -LiteralPath $LockPath) -split '(?m)(?=^\[\[package\]\])'
    foreach ($block in $blocks) {
        if ($block -match "(?m)^name = `"$([regex]::Escape($Name))`"\r?$" -and
            $block -match "(?m)^version = `"$([regex]::Escape($Version))`"\r?$") {
            if ($block -match '(?m)^checksum = "([0-9a-f]{64})"\r?$') { return $Matches[1] }
            return 'workspace_or_unchecksummed'
        }
    }
    throw "Resolved package $Name $Version was not found in Cargo.lock."
}

function Get-FrameworkProvenance([string]$Repository) {
    $manifestPath = Join-Path $Repository 'src-tauri\Cargo.toml'
    $metadataText = & cargo metadata --locked --offline --format-version 1 --manifest-path $manifestPath
    if ($LASTEXITCODE -ne 0) { throw 'Locked offline Cargo metadata resolution failed.' }
    $metadata = $metadataText | ConvertFrom-Json
    $lockPath = Join-Path $Repository 'src-tauri\Cargo.lock'
    $sourceFiles = @{
        'tauri' = @('src\ipc\command.rs', 'src\ipc\protocol.rs', 'src\webview\mod.rs')
        'tauri-macros' = @('src\command\wrapper.rs')
        'tauri-runtime-wry' = @('src\lib.rs')
        'wry' = @('src\lib.rs', 'src\webview2\mod.rs')
        'serde' = @('src\lib.rs')
        'serde_json' = @('src\lib.rs', 'src\de.rs', 'src\map.rs')
    }
    $result = [System.Collections.Generic.List[object]]::new()
    foreach ($name in @('tauri', 'tauri-macros', 'tauri-runtime-wry', 'wry', 'serde', 'serde_json')) {
        $matches = @($metadata.packages | Where-Object { $_.name -eq $name })
        if ($matches.Count -ne 1) { throw "Expected exactly one resolved $name package." }
        $package = $matches[0]
        $packageRoot = Split-Path -Parent $package.manifest_path
        $inspected = [System.Collections.Generic.List[object]]::new()
        foreach ($relative in $sourceFiles[$name]) {
            $path = Join-Path $packageRoot $relative
            if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
                throw "Required inspected source is missing for $name."
            }
            $inspected.Add([ordered]@{
                relative_path = $relative
                sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash
            })
        }
        $result.Add([ordered]@{
            name = $name
            version = $package.version
            source = $package.source
            cargo_lock_checksum = Get-LockChecksum $lockPath $name $package.version
            manifest_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $package.manifest_path).Hash
            inspected_sources = $inspected
        })
    }
    return $result
}

function Get-WebView2Provenance {
    $roots = @(
        (Join-Path ${env:ProgramFiles(x86)} 'Microsoft\EdgeWebView\Application'),
        (Join-Path $env:ProgramFiles 'Microsoft\EdgeWebView\Application'),
        (Join-Path $env:LOCALAPPDATA 'Microsoft\EdgeWebView\Application')
    ) | Where-Object { $_ -and (Test-Path -LiteralPath $_) }
    $executables = @($roots | ForEach-Object {
        Get-ChildItem -LiteralPath $_ -Filter msedgewebview2.exe -File -Recurse -ErrorAction SilentlyContinue
    })
    if ($executables.Count -eq 0) { throw 'WebView2 runtime identity could not be resolved.' }
    $selected = $executables | Sort-Object { [version]$_.VersionInfo.ProductVersion } -Descending | Select-Object -First 1
    return [ordered]@{
        product_version = $selected.VersionInfo.ProductVersion
        file_version = $selected.VersionInfo.FileVersion
        executable_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $selected.FullName).Hash
    }
}

function Get-HarnessSourceHashes([string]$Repository) {
    $relativePaths = @(
        'docs\WALLET_TAURI_LAYER_B_IMPLEMENTATION_HANDOFF.md',
        'src-tauri\Cargo.toml',
        'src-tauri\Cargo.lock',
        'src-tauri\build.rs',
        'src-tauri\src\wallet\lifecycle_command_boundary\generated_wrapper_qualification.rs',
        'src-tauri\qualification\wallet-layer-b\main.rs',
        'src-tauri\qualification\wallet-layer-b\tauri.conf.json',
        'src-tauri\qualification\wallet-layer-b\permissions\wallet-layer-b.toml',
        'src-tauri\qualification\wallet-layer-b\assets\index.html',
        'src-tauri\qualification\wallet-layer-b\assets\harness.js',
        'src-tauri\qualification\wallet-layer-b\assets\harness.css',
        'src-tauri\qualification\wallet-layer-b\run-layer-b-qualification.ps1',
        'src-tauri\tests\tauri_acl.rs'
    )
    $result = [System.Collections.Generic.List[object]]::new()
    foreach ($relative in $relativePaths) {
        $path = Join-Path $Repository $relative
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Missing qualification source: $relative" }
        $result.Add([ordered]@{ relative_path = $relative; sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash })
    }
    return $result
}

function Test-Transcript(
    [string]$Case,
    [string]$StdoutPath,
    [string]$StderrPath,
    [Nullable[int]]$ExitCode,
    [bool]$TimedOut
) {
    if ($TimedOut) { return [ordered]@{ classification = 'Inconclusive'; reason = 'case_timeout' } }
    $stdoutInfo = Get-Item -LiteralPath $StdoutPath
    $stderrInfo = Get-Item -LiteralPath $StderrPath
    if ($stdoutInfo.Length -gt $MaximumTranscriptBytes -or $stderrInfo.Length -gt $MaximumErrorTranscriptBytes) {
        return [ordered]@{ classification = 'Inconclusive'; reason = 'transcript_size_limit' }
    }
    $stderrText = Get-Content -Raw -LiteralPath $StderrPath
    if ($stderrText -match 'layer_b_browser_observation_rejected') {
        return [ordered]@{ classification = 'Failed'; reason = 'native_report_rejected' }
    }
    $records = [System.Collections.Generic.List[object]]::new()
    foreach ($line in @(Get-Content -LiteralPath $StdoutPath)) {
        if ([string]::IsNullOrWhiteSpace($line)) { continue }
        try {
            $record = $line | ConvertFrom-Json
        } catch {
            return [ordered]@{ classification = 'Inconclusive'; reason = 'non_json_stdout' }
        }
        if ($null -eq $record.marker) {
            return [ordered]@{ classification = 'Inconclusive'; reason = 'unmarked_stdout_record' }
        }
        $records.Add($record)
    }
    $browser = @($records | Where-Object { $_.marker -eq 'layer_b_browser_observation' })
    $primary = @($browser | Where-Object { $_.case -eq $Case -and $_.command -ne 'matrix' })
    $post = @($browser | Where-Object { $_.case -eq "$Case-post-revocation-proof" })
    $terminalBrowser = @($browser | Where-Object { $_.case -eq $Case -and $_.command -eq 'matrix' })
    $terminalNative = @($records | Where-Object { $_.marker -eq 'layer_b_terminal_observation' -and $_.case -eq $Case })
    if ($primary.Count -ne 1 -or $terminalBrowser.Count -ne 1 -or $terminalNative.Count -ne 1) {
        return [ordered]@{ classification = 'Inconclusive'; reason = 'missing_or_duplicate_primary_terminal_record' }
    }
    if (@($records | Where-Object { $_.outcome -eq 'case_not_observed' }).Count -ne 0) {
        return [ordered]@{ classification = 'Inconclusive'; reason = 'required_case_not_observed' }
    }
    $rejectionOutcomes = @('invalid_request', 'qualification_invalid_window', 'qualification_response_unavailable', 'qualification_runtime_unavailable')
    $postRequired = $rejectionOutcomes -contains $primary[0].expected -or $primary[0].outcome -eq 'qualification_transport_inconclusive'
    if (($postRequired -and ($post.Count -ne 1 -or $post[0].result -ne 'passed')) -or
        (-not $postRequired -and $post.Count -ne 0)) {
        return [ordered]@{ classification = 'Inconclusive'; reason = 'post_revocation_evidence_mismatch' }
    }
    $expectedWrappers = if ($Case.StartsWith('direct-') -or $Case.StartsWith('unknown-command--')) {
        0
    } elseif ($Case.StartsWith('concurrent-batch--')) {
        8 + [int]$postRequired
    } elseif ($postRequired) {
        2
    } else {
        1
    }
    if ([int]$terminalNative[0].wrapper_entries -ne $expectedWrappers -or
        [int]$terminalNative[0].primary_records -ne 1 -or
        [int]$terminalNative[0].post_records -ne [int]$postRequired -or
        [string]$terminalNative[0].result -ne [string]$terminalBrowser[0].result -or
        ($postRequired -and $terminalNative[0].revoked -ne $true)) {
        return [ordered]@{ classification = 'Inconclusive'; reason = 'native_terminal_evidence_mismatch' }
    }
    $result = [string]$terminalBrowser[0].result
    if ($result -eq 'passed' -and $primary[0].result -eq 'passed' -and $ExitCode -eq 0) {
        return [ordered]@{ classification = 'Passed'; reason = 'complete_matching_terminal_evidence'; terminal_result = $result; wrapper_entries = $expectedWrappers; primary_records = 1; post_records = $post.Count }
    }
    if ($result -eq 'failed' -and $primary[0].result -eq 'failed' -and $ExitCode -eq 2) {
        return [ordered]@{ classification = 'Failed'; reason = 'complete_failed_terminal_evidence'; terminal_result = $result; wrapper_entries = $expectedWrappers; primary_records = 1; post_records = $post.Count }
    }
    if ($result -eq 'inconclusive' -and $primary[0].result -eq 'inconclusive' -and $ExitCode -eq 3) {
        return [ordered]@{ classification = 'Inconclusive'; reason = 'complete_inconclusive_terminal_evidence'; terminal_result = $result; wrapper_entries = $expectedWrappers; primary_records = 1; post_records = $post.Count }
    }
    return [ordered]@{ classification = 'Inconclusive'; reason = 'exit_or_result_mismatch' }
}

if ($SelfTest) {
    $temporary = Join-Path ([System.IO.Path]::GetTempPath()) ("vision-layer-b-runner-test-" + [guid]::NewGuid().ToString('N'))
    [System.IO.Directory]::CreateDirectory($temporary) | Out-Null
    try {
        $stderr = Join-Path $temporary 'stderr.log'
        Set-Content -LiteralPath $stderr -Value '' -NoNewline

        $missing = Join-Path $temporary 'missing.stdout.log'
        Set-Content -LiteralPath $missing -Value '' -NoNewline
        $assessment = Test-Transcript 'exact-wallet_get_status--official-invoke' $missing $stderr 0 $false
        if ($assessment.classification -ne 'Inconclusive') { throw 'Clean exit without evidence did not fail closed.' }

        $passCase = 'exact-wallet_get_status--official-invoke'
        $passRecords = @(
            [ordered]@{ marker = 'layer_b_observation'; result = 'accepted' },
            [ordered]@{ marker = 'layer_b_browser_observation'; case = $passCase; command = 'wallet_get_status'; outcome = 'layer_b_accepted'; expected = 'layer_b_accepted'; result = 'passed' },
            [ordered]@{ marker = 'layer_b_terminal_observation'; case = $passCase; result = 'passed'; wrapper_entries = 1; primary_records = 1; post_records = 0; revoked = $false },
            [ordered]@{ marker = 'layer_b_browser_observation'; case = $passCase; command = 'matrix'; outcome = 'matrix_complete'; expected = 'matrix_complete'; result = 'passed' }
        )
        $pass = Join-Path $temporary 'pass.stdout.log'
        Write-Utf8NoBom $pass @($passRecords | ForEach-Object { $_ | ConvertTo-Json -Compress })
        $assessment = Test-Transcript $passCase $pass $stderr 0 $false
        if ($assessment.classification -ne 'Passed') { throw 'Complete matching evidence was not accepted.' }

        $concurrentCase = 'concurrent-batch--official-invoke'
        $concurrentRecords = @(
            [ordered]@{ marker = 'layer_b_browser_observation'; case = $concurrentCase; command = 'wallet_get_status'; outcome = 'layer_b_accepted'; expected = 'layer_b_accepted'; result = 'passed' },
            [ordered]@{ marker = 'layer_b_terminal_observation'; case = $concurrentCase; result = 'passed'; wrapper_entries = 8; primary_records = 1; post_records = 0; revoked = $false },
            [ordered]@{ marker = 'layer_b_browser_observation'; case = $concurrentCase; command = 'matrix'; outcome = 'matrix_complete'; expected = 'matrix_complete'; result = 'passed' }
        )
        $concurrent = Join-Path $temporary 'concurrent.stdout.log'
        Write-Utf8NoBom $concurrent @($concurrentRecords | ForEach-Object { $_ | ConvertTo-Json -Compress })
        $assessment = Test-Transcript $concurrentCase $concurrent $stderr 0 $false
        if ($assessment.classification -ne 'Passed') { throw 'Eight-entry concurrency evidence was not accepted.' }

        $concurrentRecords[1].wrapper_entries = 1
        $badConcurrent = Join-Path $temporary 'bad-concurrent.stdout.log'
        Write-Utf8NoBom $badConcurrent @($concurrentRecords | ForEach-Object { $_ | ConvertTo-Json -Compress })
        $assessment = Test-Transcript $concurrentCase $badConcurrent $stderr 0 $false
        if ($assessment.classification -ne 'Inconclusive') { throw 'Incomplete concurrency evidence did not fail closed.' }

        $inconclusiveCase = 'exact-wallet_get_status--official-invoke'
        $inconclusiveRecords = @(
            [ordered]@{ marker = 'layer_b_observation'; result = 'qualification_transport_inconclusive' },
            [ordered]@{ marker = 'layer_b_browser_observation'; case = $inconclusiveCase; command = 'wallet_get_status'; outcome = 'qualification_transport_inconclusive'; expected = 'layer_b_accepted'; result = 'inconclusive' },
            [ordered]@{ marker = 'layer_b_browser_observation'; case = "$inconclusiveCase-post-revocation-proof"; command = 'wallet_get_status'; outcome = 'qualification_runtime_unavailable'; expected = 'qualification_runtime_unavailable'; result = 'passed' },
            [ordered]@{ marker = 'layer_b_terminal_observation'; case = $inconclusiveCase; result = 'inconclusive'; wrapper_entries = 2; primary_records = 1; post_records = 1; revoked = $true },
            [ordered]@{ marker = 'layer_b_browser_observation'; case = $inconclusiveCase; command = 'matrix'; outcome = 'matrix_inconclusive'; expected = 'matrix_complete'; result = 'inconclusive' }
        )
        $inconclusive = Join-Path $temporary 'inconclusive.stdout.log'
        Write-Utf8NoBom $inconclusive @($inconclusiveRecords | ForEach-Object { $_ | ConvertTo-Json -Compress })
        $assessment = Test-Transcript $inconclusiveCase $inconclusive $stderr 3 $false
        if ($assessment.classification -ne 'Inconclusive') { throw 'Unproven transport was not classified Inconclusive.' }

        $rejectedStderr = Join-Path $temporary 'rejected.stderr.log'
        Set-Content -LiteralPath $rejectedStderr -Value 'layer_b_browser_observation_rejected'
        $assessment = Test-Transcript $passCase $pass $rejectedStderr 0 $false
        if ($assessment.classification -ne 'Failed') { throw 'Native report rejection was not classified Failed.' }

        $selfTestRepository = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path
        $frameworkProof = @(Get-FrameworkProvenance $selfTestRepository)
        if ($frameworkProof.Count -ne 6) { throw 'Framework provenance did not resolve the six required packages.' }
        $webViewProof = Get-WebView2Provenance
        if ([string]::IsNullOrWhiteSpace($webViewProof.product_version)) { throw 'WebView2 provenance is incomplete.' }
        $sourceProof = @(Get-HarnessSourceHashes $selfTestRepository)
        if ($sourceProof.Count -lt 10) { throw 'Harness source provenance is incomplete.' }

        Write-Output 'Layer B runner self-tests passed: 9 (6 transcript, 3 provenance)'
        exit 0
    } finally {
        Remove-Item -LiteralPath $temporary -Recurse -Force -ErrorAction SilentlyContinue
    }
}

$binary = (Resolve-Path -LiteralPath $BinaryPath).Path
$evidence = [System.IO.Path]::GetFullPath($EvidenceDirectory)
$walletRoot = [System.IO.Path]::GetFullPath($WalletCustodyRoot)
$coreRoot = (Resolve-Path -LiteralPath $VisionCoreRoot).Path
$repository = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path
if ($CaseTimeoutSeconds -lt 10 -or $CaseTimeoutSeconds -gt 300) { throw 'Case timeout must be between 10 and 300 seconds.' }
$repositoryPrefix = $repository.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
if ($evidence.StartsWith($repositoryPrefix, [System.StringComparison]::OrdinalIgnoreCase)) { throw 'Evidence must be written outside the reviewed repository.' }
if (Test-Path -LiteralPath $evidence) { throw 'Evidence directory already exists; qualification never overwrites evidence.' }
$protectedRoots = @($walletRoot, $coreRoot)
foreach ($protectedRoot in $protectedRoots) {
    $protectedPrefix = $protectedRoot.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
    if ($evidence.Equals($protectedRoot, [System.StringComparison]::OrdinalIgnoreCase) -or
        $evidence.StartsWith($protectedPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw 'Evidence must be outside wallet custody and Vision-Core roots.'
    }
}

$preRepository = Get-GitProof $repository
if (-not $preRepository.clean) { throw 'Qualification requires the exact clean reviewed worktree.' }
$preCore = Get-GitProof $coreRoot
$preWallet = Get-TreeFingerprint $walletRoot
$binaryPreHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $binary).Hash
$commit = $preRepository.commit
$tree = $preRepository.tree
$parent = (& git -C $repository rev-parse 'HEAD^').Trim()
$framework = Get-FrameworkProvenance $repository
$webview2 = Get-WebView2Provenance
$harnessSources = Get-HarnessSourceHashes $repository
[System.IO.Directory]::CreateDirectory($evidence) | Out-Null

$commands = @('wallet_get_status', 'wallet_select_recovery_destination', 'wallet_create', 'wallet_select_recovery_source', 'wallet_restore', 'wallet_unlock', 'wallet_lock')
$routes = @('official-invoke', 'internals-invoke', 'internals-ipc')
$duplicateFamilies = @('identical', 'conflicting', 'valid-then-malformed', 'malformed-then-valid', 'public-then-secret-like', 'exact-and-wrong-case', 'three-repeated', 'escaped-equivalent', 'bounded-whitespace')
$representations = @('string', 'bytes', 'object-normalized')
$cases = [System.Collections.Generic.List[string]]::new()
foreach ($scenario in @('window-other-local', 'window-remote-origin', 'window-recreated-main', 'window-reloaded-generation', 'window-destruction-race', 'window-revocation-race')) { $cases.Add("$scenario--official-invoke") }
foreach ($command in $commands) { $cases.Add("exact-$command--official-invoke") }
foreach ($route in @('internals-invoke', 'internals-ipc')) { $cases.Add("exact-status-$route--$route") }
foreach ($id in @('raw-empty', 'raw-json-looking', 'raw-arbitrary', 'raw-bytes', 'json-null', 'json-boolean', 'json-number', 'json-string', 'json-array', 'extra-top-level', 'wrong-case-top-level', 'secret-like-top-level', 'key-name-canary-top-level', 'oversized-key-top-level', 'excessive-key-count-top-level')) { $cases.Add("$id--official-invoke") }
foreach ($id in @('create-empty', 'create-request-empty', 'create-request-wrong-type', 'create-unknown-field', 'create-secret-like-field', 'create-invalid-handle', 'restore-wrong-handle-name', 'unknown-command')) { $cases.Add("$id--official-invoke") }
foreach ($family in $duplicateFamilies) {
    foreach ($representation in $representations) {
        foreach ($route in $routes) {
            $cases.Add("duplicate-$family-$representation--$route")
            $cases.Add("nested-duplicate-$family-$representation--$route")
        }
    }
}
$cases.Add('concurrent-batch--official-invoke')
$cases.Add('direct-fetch-text-missing-invoke-key--direct-fetch-text')
$cases.Add('direct-fetch-bytes-missing-invoke-key--direct-fetch-bytes')
$cases.Add('direct-xhr-missing-invoke-key--direct-xhr')
$cases.Add('forced-post-message-fallback--internals-post-message')
foreach ($point in @('metadata', 'body', 'response', 'observation')) { $cases.Add("contained-$point-panic--internals-post-message") }
$cases.Add('contained-fixed-error-panic--internals-post-message')

$records = [System.Collections.Generic.List[object]]::new()
$overall = 'Passed'
foreach ($case in $cases) {
    $safeName = $case -replace '[^a-z0-9-]', '-'
    $stdoutPath = Join-Path $evidence "$safeName.stdout.log"
    $stderrPath = Join-Path $evidence "$safeName.stderr.log"
    $startedUtc = [DateTimeOffset]::UtcNow.ToString('O')
    $process = Start-Process -FilePath $binary -ArgumentList @('--wallet-layer-b-qualification', "--wallet-layer-b-case=$case") -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath -PassThru -WindowStyle Hidden
    $processId = $process.Id
    $timedOut = -not $process.WaitForExit($CaseTimeoutSeconds * 1000)
    if ($timedOut) { $process.Kill($true); $process.WaitForExit() }
    $endedUtc = [DateTimeOffset]::UtcNow.ToString('O')
    $exitCode = if ($timedOut) { $null } else { $process.ExitCode }
    $assessment = Test-Transcript $case $stdoutPath $stderrPath $exitCode $timedOut
    if ($assessment.classification -eq 'Failed') { $overall = 'Failed' }
    if ($assessment.classification -eq 'Inconclusive' -and $overall -eq 'Passed') { $overall = 'Inconclusive' }
    $records.Add([ordered]@{
        case = $case
        process_id = $processId
        launch_arguments = @('--wallet-layer-b-qualification', "--wallet-layer-b-case=$case")
        started_utc = $startedUtc
        ended_utc = $endedUtc
        exit_code = $exitCode
        timed_out = $timedOut
        classification = $assessment.classification
        classification_reason = $assessment.reason
        terminal_result = $assessment.terminal_result
        wrapper_entries = $assessment.wrapper_entries
        primary_records = $assessment.primary_records
        post_records = $assessment.post_records
        stdout_bytes = (Get-Item -LiteralPath $stdoutPath).Length
        stderr_bytes = (Get-Item -LiteralPath $stderrPath).Length
        stdout_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $stdoutPath).Hash
        stderr_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $stderrPath).Hash
    })
}

$postRepository = Get-GitProof $repository
$postCore = Get-GitProof $coreRoot
$postWallet = Get-TreeFingerprint $walletRoot
$postWebview2 = Get-WebView2Provenance
$binaryPostHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $binary).Hash
$integrityPreserved = $preRepository.commit -eq $postRepository.commit -and
    $preRepository.tree -eq $postRepository.tree -and $postRepository.clean -and
    $preRepository.status_sha256 -eq $postRepository.status_sha256 -and
    $preCore.commit -eq $postCore.commit -and $preCore.tree -eq $postCore.tree -and
    $preCore.status_sha256 -eq $postCore.status_sha256 -and
    $preWallet.exists -eq $postWallet.exists -and $preWallet.file_count -eq $postWallet.file_count -and
    $preWallet.total_bytes -eq $postWallet.total_bytes -and $preWallet.sha256 -eq $postWallet.sha256 -and
    $webview2.product_version -eq $postWebview2.product_version -and
    $webview2.file_version -eq $postWebview2.file_version -and
    $webview2.executable_sha256 -eq $postWebview2.executable_sha256 -and
    $binaryPreHash -eq $binaryPostHash
if (-not $integrityPreserved) { $overall = 'Failed' }

$manifest = [ordered]@{
    marker = 'vision_wallet_layer_b_primary_evidence'
    result = $overall
    binary_sha256_before = $binaryPreHash
    binary_sha256_after = $binaryPostHash
    implementation_commit = $commit
    implementation_tree = $tree
    implementation_parent = $parent
    operator = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name
    operating_system = [System.Environment]::OSVersion.VersionString
    architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    webview2_before = $webview2
    webview2_after = $postWebview2
    framework_dependencies = $framework
    harness_source_hashes = $harnessSources
    cargo_lock_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $repository 'src-tauri\Cargo.lock')).Hash
    repository_before = $preRepository
    repository_after = $postRepository
    vision_core_before = $preCore
    vision_core_after = $postCore
    wallet_custody_before = $preWallet
    wallet_custody_after = $postWallet
    integrity_preserved = $integrityPreserved
    screenshots = @()
    screenshots_note = 'No screenshots are required; complete bounded stdout and stderr transcripts are the primary evidence.'
    case_timeout_seconds = $CaseTimeoutSeconds
    total = $records.Count
    cases = $records
}
$manifestPath = Join-Path $evidence 'layer-b-evidence-manifest.json'
Write-Utf8NoBom $manifestPath @(($manifest | ConvertTo-Json -Depth 12))
$manifestHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $manifestPath).Hash
Write-Output "Layer B result: $overall"
Write-Output "Evidence manifest: $manifestPath"
Write-Output "Evidence manifest SHA-256: $manifestHash"
if ($overall -eq 'Passed') { exit 0 }
if ($overall -eq 'Inconclusive') { exit 3 }
exit 2
