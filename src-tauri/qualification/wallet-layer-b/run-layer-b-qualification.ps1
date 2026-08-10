param(
    [Parameter(Mandatory = $true, ParameterSetName = 'Run')]
    [string]$BinaryPath,
    [Parameter(Mandatory = $true, ParameterSetName = 'Run')]
    [string]$EvidenceDirectory,
    [Parameter(Mandatory = $true, ParameterSetName = 'Run')]
    [string]$WalletCustodyRoot,
    [Parameter(Mandatory = $true, ParameterSetName = 'Run')]
    [string]$VisionCoreRoot,
    [Parameter(Mandatory = $true, ParameterSetName = 'Run')]
    [string]$ProductionExecutablePath,
    [Parameter(Mandatory = $true, ParameterSetName = 'Run')]
    [string]$ProductionInstallationRoot,
    [Parameter(Mandatory = $true, ParameterSetName = 'Run')]
    [string]$ProductionDataRoot,
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
    } | Sort-Object FullName -Unique)
    if ($executables.Count -eq 0) { throw 'WebView2 runtime identity could not be resolved.' }
    $result = [System.Collections.Generic.List[object]]::new()
    foreach ($executable in $executables) {
        $result.Add([ordered]@{
            product_version = $executable.VersionInfo.ProductVersion
            file_version = $executable.VersionInfo.FileVersion
            executable_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $executable.FullName).Hash
        })
    }
    return $result
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

function Get-WrapperCaseInfo([string]$Case) {
    $commands = @('wallet_get_status', 'wallet_select_recovery_destination', 'wallet_create', 'wallet_select_recovery_source', 'wallet_restore', 'wallet_unlock', 'wallet_lock')
    foreach ($command in $commands) {
        $prefix = "wrapper-$command-"
        if ($Case.StartsWith($prefix, [System.StringComparison]::Ordinal)) {
            $family = ($Case.Substring($prefix.Length) -split '--', 2)[0]
            return [ordered]@{ command = $command; family = $family; command_index = [array]::IndexOf($commands, $command); commands = $commands }
        }
    }
    return $null
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
    $wrapperCase = Get-WrapperCaseInfo $Case
    if ($wrapperCase.family -eq 'window-destruction-race') {
        $destruction = @($records | Where-Object { $_.marker -eq 'layer_b_native_destruction_observation' -and $_.case -eq $Case })
        $terminal = @($records | Where-Object { $_.marker -eq 'layer_b_terminal_observation' -and $_.case -eq $Case })
        $unexpectedBrowser = @($records | Where-Object { $_.marker -eq 'layer_b_browser_observation' })
        if ($records.Count -ne 2 -or $destruction.Count -ne 1 -or $terminal.Count -ne 1 -or $unexpectedBrowser.Count -ne 0) {
            return [ordered]@{ classification = 'Inconclusive'; reason = 'native_destruction_records_incomplete' }
        }
        $runtimeVersion = [string]$terminal[0].webview2_runtime_version
        if ($runtimeVersion -notmatch '^\d{1,5}(\.\d{1,5}){3}$') {
            return [ordered]@{ classification = 'Inconclusive'; reason = 'loaded_webview2_runtime_unproven' }
        }
        if ([string]$destruction[0].command -ne $wrapperCase.command -or
            [int]$terminal[0].wrapper_entries -ne 1 -or
            [int]$terminal[0].primary_records -ne 1 -or
            [int]$terminal[0].post_records -ne 1 -or
            $terminal[0].revoked -ne $true -or
            [string]$terminal[0].result -ne [string]$destruction[0].result) {
            return [ordered]@{ classification = 'Inconclusive'; reason = 'native_destruction_proof_mismatch' }
        }
        if ([string]$destruction[0].result -eq 'failed' -and $ExitCode -eq 2) {
            return [ordered]@{ classification = 'Failed'; reason = 'native_target_destruction_failed'; terminal_result = 'failed'; wrapper_entries = 1; primary_records = 1; post_records = 1; webview2_runtime_version = $runtimeVersion }
        }
        if ([string]$destruction[0].result -eq 'passed' -and $ExitCode -eq 0 -and
            $destruction[0].exact_authorized_hwnd -eq $true -and
            $destruction[0].exact_target_window -eq $true -and
            $destruction[0].destroy_call_succeeded -eq $true -and
            $destruction[0].target_window_absent -eq $true -and
            $destruction[0].native_hwnd_absent -eq $true -and
            $destruction[0].authority_revoked -eq $true -and
            $destruction[0].post_revocation_proven -eq $true) {
            return [ordered]@{ classification = 'Passed'; reason = 'verified_native_target_destruction'; terminal_result = 'passed'; wrapper_entries = 1; primary_records = 1; post_records = 1; webview2_runtime_version = $runtimeVersion }
        }
        return [ordered]@{ classification = 'Inconclusive'; reason = 'native_destruction_exit_mismatch' }
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
    } elseif ($wrapperCase.family -eq 'concurrent-batch') {
        8 + [int]$postRequired
    } elseif ($wrapperCase.family -in @('sequential-repeat', 'reordered-invoke')) {
        2
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
    $runtimeVersion = [string]$terminalNative[0].webview2_runtime_version
    if ($runtimeVersion -notmatch '^\d{1,5}(\.\d{1,5}){3}$') {
        return [ordered]@{ classification = 'Inconclusive'; reason = 'loaded_webview2_runtime_unproven' }
    }
    $nativeCommands = @($records | Where-Object { $_.marker -eq 'layer_b_observation' } | ForEach-Object { [string]$_.command })
    if ($wrapperCase.family -eq 'sequential-repeat' -and
        ($nativeCommands.Count -ne 2 -or $nativeCommands[0] -ne $wrapperCase.command -or $nativeCommands[1] -ne $wrapperCase.command)) {
        return [ordered]@{ classification = 'Inconclusive'; reason = 'sequential_repeat_native_order_unproven' }
    }
    if ($wrapperCase.family -eq 'reordered-invoke') {
        $next = $wrapperCase.commands[($wrapperCase.command_index + 1) % $wrapperCase.commands.Count]
        if ($nativeCommands.Count -ne 2 -or $nativeCommands[0] -ne $next -or $nativeCommands[1] -ne $wrapperCase.command) {
            return [ordered]@{ classification = 'Inconclusive'; reason = 'reordered_native_sequence_unproven' }
        }
    }
    if ($wrapperCase.family -eq 'concurrent-batch' -and
        ($nativeCommands.Count -ne 8 -or @($nativeCommands | Where-Object { $_ -ne $wrapperCase.command }).Count -ne 0)) {
        return [ordered]@{ classification = 'Inconclusive'; reason = 'concurrent_native_entries_unproven' }
    }
    if ($wrapperCase.family -eq 'declared-invoked-mismatch' -and
        ($nativeCommands.Count -ne 2 -or @($nativeCommands | Where-Object { $_ -ne $wrapperCase.command }).Count -ne 0)) {
        return [ordered]@{ classification = 'Inconclusive'; reason = 'generated_wrapper_name_mismatch_unproven' }
    }
    $result = [string]$terminalBrowser[0].result
    if ($result -eq 'passed' -and $primary[0].result -eq 'passed' -and $ExitCode -eq 0) {
        return [ordered]@{ classification = 'Passed'; reason = 'complete_matching_terminal_evidence'; terminal_result = $result; wrapper_entries = $expectedWrappers; primary_records = 1; post_records = $post.Count; webview2_runtime_version = $runtimeVersion }
    }
    if ($result -eq 'failed' -and $primary[0].result -eq 'failed' -and $ExitCode -eq 2) {
        return [ordered]@{ classification = 'Failed'; reason = 'complete_failed_terminal_evidence'; terminal_result = $result; wrapper_entries = $expectedWrappers; primary_records = 1; post_records = $post.Count; webview2_runtime_version = $runtimeVersion }
    }
    if ($result -eq 'inconclusive' -and $primary[0].result -eq 'inconclusive' -and $ExitCode -eq 3) {
        return [ordered]@{ classification = 'Inconclusive'; reason = 'complete_inconclusive_terminal_evidence'; terminal_result = $result; wrapper_entries = $expectedWrappers; primary_records = 1; post_records = $post.Count; webview2_runtime_version = $runtimeVersion }
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
            [ordered]@{ marker = 'layer_b_terminal_observation'; case = $passCase; result = 'passed'; wrapper_entries = 1; primary_records = 1; post_records = 0; revoked = $false; webview2_runtime_version = '151.0.4129.72' },
            [ordered]@{ marker = 'layer_b_browser_observation'; case = $passCase; command = 'matrix'; outcome = 'matrix_complete'; expected = 'matrix_complete'; result = 'passed' }
        )
        $pass = Join-Path $temporary 'pass.stdout.log'
        Write-Utf8NoBom $pass @($passRecords | ForEach-Object { $_ | ConvertTo-Json -Compress })
        $assessment = Test-Transcript $passCase $pass $stderr 0 $false
        if ($assessment.classification -ne 'Passed') { throw 'Complete matching evidence was not accepted.' }

        $passRecords[2].webview2_runtime_version = $null
        $missingRuntime = Join-Path $temporary 'missing-runtime.stdout.log'
        Write-Utf8NoBom $missingRuntime @($passRecords | ForEach-Object { $_ | ConvertTo-Json -Compress })
        $assessment = Test-Transcript $passCase $missingRuntime $stderr 0 $false
        if ($assessment.classification -ne 'Inconclusive') { throw 'Missing loaded WebView2 identity did not fail closed.' }
        $passRecords[2].webview2_runtime_version = '151.0.4129.72'

        $concurrentCase = 'wrapper-wallet_get_status-concurrent-batch--official-invoke'
        $concurrentRecords = @(
            [ordered]@{ marker = 'layer_b_observation'; command = 'wallet_get_status'; result = 'accepted' },
            [ordered]@{ marker = 'layer_b_observation'; command = 'wallet_get_status'; result = 'accepted' },
            [ordered]@{ marker = 'layer_b_observation'; command = 'wallet_get_status'; result = 'accepted' },
            [ordered]@{ marker = 'layer_b_observation'; command = 'wallet_get_status'; result = 'accepted' },
            [ordered]@{ marker = 'layer_b_observation'; command = 'wallet_get_status'; result = 'accepted' },
            [ordered]@{ marker = 'layer_b_observation'; command = 'wallet_get_status'; result = 'accepted' },
            [ordered]@{ marker = 'layer_b_observation'; command = 'wallet_get_status'; result = 'accepted' },
            [ordered]@{ marker = 'layer_b_observation'; command = 'wallet_get_status'; result = 'accepted' },
            [ordered]@{ marker = 'layer_b_browser_observation'; case = $concurrentCase; command = 'wallet_get_status'; outcome = 'layer_b_accepted'; expected = 'layer_b_accepted'; result = 'passed' },
            [ordered]@{ marker = 'layer_b_terminal_observation'; case = $concurrentCase; result = 'passed'; wrapper_entries = 8; primary_records = 1; post_records = 0; revoked = $false; webview2_runtime_version = '151.0.4129.72' },
            [ordered]@{ marker = 'layer_b_browser_observation'; case = $concurrentCase; command = 'matrix'; outcome = 'matrix_complete'; expected = 'matrix_complete'; result = 'passed' }
        )
        $concurrent = Join-Path $temporary 'concurrent.stdout.log'
        Write-Utf8NoBom $concurrent @($concurrentRecords | ForEach-Object { $_ | ConvertTo-Json -Compress })
        $assessment = Test-Transcript $concurrentCase $concurrent $stderr 0 $false
        if ($assessment.classification -ne 'Passed') { throw 'Eight-entry concurrency evidence was not accepted.' }

        $concurrentRecords[9].wrapper_entries = 1
        $badConcurrent = Join-Path $temporary 'bad-concurrent.stdout.log'
        Write-Utf8NoBom $badConcurrent @($concurrentRecords | ForEach-Object { $_ | ConvertTo-Json -Compress })
        $assessment = Test-Transcript $concurrentCase $badConcurrent $stderr 0 $false
        if ($assessment.classification -ne 'Inconclusive') { throw 'Incomplete concurrency evidence did not fail closed.' }

        $sequenceCase = 'wrapper-wallet_get_status-reordered-invoke--official-invoke'
        $sequenceRecords = @(
            [ordered]@{ marker = 'layer_b_observation'; command = 'wallet_select_recovery_destination'; result = 'accepted' },
            [ordered]@{ marker = 'layer_b_observation'; command = 'wallet_get_status'; result = 'accepted' },
            [ordered]@{ marker = 'layer_b_browser_observation'; case = $sequenceCase; command = 'wallet_get_status'; outcome = 'layer_b_accepted'; expected = 'layer_b_accepted'; result = 'passed' },
            [ordered]@{ marker = 'layer_b_terminal_observation'; case = $sequenceCase; result = 'passed'; wrapper_entries = 2; primary_records = 1; post_records = 0; revoked = $false; webview2_runtime_version = '151.0.4129.72' },
            [ordered]@{ marker = 'layer_b_browser_observation'; case = $sequenceCase; command = 'matrix'; outcome = 'matrix_complete'; expected = 'matrix_complete'; result = 'passed' }
        )
        $sequence = Join-Path $temporary 'sequence.stdout.log'
        Write-Utf8NoBom $sequence @($sequenceRecords | ForEach-Object { $_ | ConvertTo-Json -Compress })
        $assessment = Test-Transcript $sequenceCase $sequence $stderr 0 $false
        if ($assessment.classification -ne 'Passed') { throw 'Reordered wrapper sequence was not accepted.' }
        $sequenceRecords[0].command = 'wallet_get_status'
        $badSequence = Join-Path $temporary 'bad-sequence.stdout.log'
        Write-Utf8NoBom $badSequence @($sequenceRecords | ForEach-Object { $_ | ConvertTo-Json -Compress })
        $assessment = Test-Transcript $sequenceCase $badSequence $stderr 0 $false
        if ($assessment.classification -ne 'Inconclusive') { throw 'Incorrect wrapper order did not fail closed.' }

        $mismatchCase = 'wrapper-wallet_get_status-declared-invoked-mismatch--official-invoke'
        $mismatchRecords = @(
            [ordered]@{ marker = 'layer_b_observation'; command = 'wallet_get_status'; result = 'invalid_request' },
            [ordered]@{ marker = 'layer_b_observation'; command = 'wallet_get_status'; result = 'qualification_runtime_unavailable' },
            [ordered]@{ marker = 'layer_b_browser_observation'; case = $mismatchCase; command = 'wallet_select_recovery_destination'; outcome = 'invalid_request'; expected = 'invalid_request'; result = 'passed' },
            [ordered]@{ marker = 'layer_b_browser_observation'; case = "$mismatchCase-post-revocation-proof"; command = 'wallet_select_recovery_destination'; outcome = 'qualification_runtime_unavailable'; expected = 'qualification_runtime_unavailable'; result = 'passed' },
            [ordered]@{ marker = 'layer_b_terminal_observation'; case = $mismatchCase; result = 'passed'; wrapper_entries = 2; primary_records = 1; post_records = 1; revoked = $true; webview2_runtime_version = '151.0.4129.72' },
            [ordered]@{ marker = 'layer_b_browser_observation'; case = $mismatchCase; command = 'matrix'; outcome = 'matrix_complete'; expected = 'matrix_complete'; result = 'passed' }
        )
        $mismatch = Join-Path $temporary 'mismatch.stdout.log'
        Write-Utf8NoBom $mismatch @($mismatchRecords | ForEach-Object { $_ | ConvertTo-Json -Compress })
        $assessment = Test-Transcript $mismatchCase $mismatch $stderr 0 $false
        if ($assessment.classification -ne 'Passed') { throw 'Generated-wrapper mismatch evidence was not accepted.' }
        $mismatchRecords[0].command = 'wallet_select_recovery_destination'
        $badMismatch = Join-Path $temporary 'bad-mismatch.stdout.log'
        Write-Utf8NoBom $badMismatch @($mismatchRecords | ForEach-Object { $_ | ConvertTo-Json -Compress })
        $assessment = Test-Transcript $mismatchCase $badMismatch $stderr 0 $false
        if ($assessment.classification -ne 'Inconclusive') { throw 'Wrong generated-wrapper mismatch evidence did not fail closed.' }

        $destructionCase = 'wrapper-wallet_get_status-window-destruction-race--official-invoke'
        $destructionRecords = @(
            [ordered]@{ marker = 'layer_b_native_destruction_observation'; case = $destructionCase; command = 'wallet_get_status'; result = 'passed'; exact_authorized_hwnd = $true; exact_target_window = $true; destroy_call_succeeded = $true; target_window_absent = $true; native_hwnd_absent = $true; authority_revoked = $true; post_revocation_proven = $true },
            [ordered]@{ marker = 'layer_b_terminal_observation'; case = $destructionCase; result = 'passed'; wrapper_entries = 1; primary_records = 1; post_records = 1; revoked = $true; webview2_runtime_version = '151.0.4129.72' }
        )
        $destruction = Join-Path $temporary 'destruction.stdout.log'
        Write-Utf8NoBom $destruction @($destructionRecords | ForEach-Object { $_ | ConvertTo-Json -Compress })
        $assessment = Test-Transcript $destructionCase $destruction $stderr 0 $false
        if ($assessment.classification -ne 'Passed') { throw 'Verified native destruction evidence was not accepted.' }
        $destructionRecords[0].target_window_absent = $false
        $badDestruction = Join-Path $temporary 'bad-destruction.stdout.log'
        Write-Utf8NoBom $badDestruction @($destructionRecords | ForEach-Object { $_ | ConvertTo-Json -Compress })
        $assessment = Test-Transcript $destructionCase $badDestruction $stderr 0 $false
        if ($assessment.classification -ne 'Inconclusive') { throw 'Incomplete native destruction evidence did not fail closed.' }

        $inconclusiveCase = 'exact-wallet_get_status--official-invoke'
        $inconclusiveRecords = @(
            [ordered]@{ marker = 'layer_b_observation'; result = 'qualification_transport_inconclusive' },
            [ordered]@{ marker = 'layer_b_browser_observation'; case = $inconclusiveCase; command = 'wallet_get_status'; outcome = 'qualification_transport_inconclusive'; expected = 'layer_b_accepted'; result = 'inconclusive' },
            [ordered]@{ marker = 'layer_b_browser_observation'; case = "$inconclusiveCase-post-revocation-proof"; command = 'wallet_get_status'; outcome = 'qualification_runtime_unavailable'; expected = 'qualification_runtime_unavailable'; result = 'passed' },
            [ordered]@{ marker = 'layer_b_terminal_observation'; case = $inconclusiveCase; result = 'inconclusive'; wrapper_entries = 2; primary_records = 1; post_records = 1; revoked = $true; webview2_runtime_version = '151.0.4129.72' },
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
        $webViewProof = @(Get-WebView2Provenance)
        if ($webViewProof.Count -eq 0 -or [string]::IsNullOrWhiteSpace($webViewProof[0].product_version)) { throw 'WebView2 provenance is incomplete.' }
        $sourceProof = @(Get-HarnessSourceHashes $selfTestRepository)
        if ($sourceProof.Count -lt 10) { throw 'Harness source provenance is incomplete.' }

        Write-Output 'Layer B runner self-tests passed: 16 (13 transcript, 3 provenance)'
        exit 0
    } finally {
        Remove-Item -LiteralPath $temporary -Recurse -Force -ErrorAction SilentlyContinue
    }
}

$binary = (Resolve-Path -LiteralPath $BinaryPath).Path
$evidence = [System.IO.Path]::GetFullPath($EvidenceDirectory)
$walletRoot = [System.IO.Path]::GetFullPath($WalletCustodyRoot)
$coreRoot = (Resolve-Path -LiteralPath $VisionCoreRoot).Path
$productionExecutable = (Resolve-Path -LiteralPath $ProductionExecutablePath).Path
$productionInstallationRoot = (Resolve-Path -LiteralPath $ProductionInstallationRoot).Path
$productionDataRoot = [System.IO.Path]::GetFullPath($ProductionDataRoot)
$repository = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path
if ($CaseTimeoutSeconds -lt 10 -or $CaseTimeoutSeconds -gt 300) { throw 'Case timeout must be between 10 and 300 seconds.' }
$repositoryPrefix = $repository.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
if ($evidence.StartsWith($repositoryPrefix, [System.StringComparison]::OrdinalIgnoreCase)) { throw 'Evidence must be written outside the reviewed repository.' }
if (Test-Path -LiteralPath $evidence) { throw 'Evidence directory already exists; qualification never overwrites evidence.' }
$productionInstallationPrefix = $productionInstallationRoot.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
if (-not $productionExecutable.StartsWith($productionInstallationPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw 'Production executable must be inside the supplied production installation root.'
}
$protectedRoots = @($walletRoot, $coreRoot, $productionInstallationRoot, $productionDataRoot)
foreach ($protectedRoot in $protectedRoots) {
    $protectedPrefix = $protectedRoot.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
    $evidencePrefix = $evidence.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
    if ($evidence.Equals($protectedRoot, [System.StringComparison]::OrdinalIgnoreCase) -or
        $evidence.StartsWith($protectedPrefix, [System.StringComparison]::OrdinalIgnoreCase) -or
        $protectedRoot.StartsWith($evidencePrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw 'Evidence must be outside wallet custody, Vision-Core, and production application roots.'
    }
}

$preRepository = Get-GitProof $repository
if (-not $preRepository.clean) { throw 'Qualification requires the exact clean reviewed worktree.' }
$preCore = Get-GitProof $coreRoot
$preWallet = Get-TreeFingerprint $walletRoot
$preProductionInstallation = Get-TreeFingerprint $productionInstallationRoot
$preProductionData = Get-TreeFingerprint $productionDataRoot
$productionExecutablePreHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $productionExecutable).Hash
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
foreach ($command in $commands) {
    $cases.Add("wrapper-$command-exact--official-invoke")
    foreach ($family in @(
        'raw-empty', 'raw-json-looking', 'raw-arbitrary', 'raw-bytes',
        'json-null', 'json-boolean', 'json-number', 'json-string', 'json-array',
        'shape-mismatch', 'missing-top-level', 'extra-top-level', 'wrong-case-top-level',
        'secret-like-top-level', 'wrong-command-envelope'
    )) {
        $cases.Add("wrapper-$command-$family--official-invoke")
    }
    if ($command -in @('wallet_create', 'wallet_restore')) {
        foreach ($family in @('malformed-nested', 'oversized-nested', 'unknown-nested', 'secret-like-nested')) {
            $cases.Add("wrapper-$command-$family--official-invoke")
        }
    }
    $cases.Add("wrapper-$command-declared-invoked-mismatch--official-invoke")
    foreach ($family in @(
        'window-other-local', 'window-remote-origin', 'window-recreated-main',
        'window-reloaded-generation', 'window-destruction-race', 'window-revocation-race'
    )) {
        $cases.Add("wrapper-$command-$family--official-invoke")
    }
    foreach ($point in @('metadata', 'body', 'response', 'observation', 'fixed-error')) {
        $cases.Add("wrapper-$command-panic-$point--internals-post-message")
    }
    $cases.Add("wrapper-$command-sequential-repeat--official-invoke")
    $cases.Add("wrapper-$command-concurrent-batch--official-invoke")
    $cases.Add("wrapper-$command-reordered-invoke--official-invoke")
    $cases.Add("wrapper-$command-post-revocation--official-invoke")
}
foreach ($route in @('internals-invoke', 'internals-ipc')) { $cases.Add("exact-status-$route--$route") }
foreach ($family in $duplicateFamilies) {
    foreach ($representation in $representations) {
        foreach ($route in $routes) {
            foreach ($commandName in @('create', 'restore')) {
                $cases.Add("duplicate-$commandName-$family-$representation--$route")
                $cases.Add("nested-duplicate-$commandName-$family-$representation--$route")
            }
        }
    }
}
$cases.Add('direct-fetch-text-missing-invoke-key--direct-fetch-text')
$cases.Add('direct-fetch-bytes-missing-invoke-key--direct-fetch-bytes')
$cases.Add('direct-xhr-missing-invoke-key--direct-xhr')
$cases.Add('forced-post-message-fallback--internals-post-message')

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
        webview2_runtime_version = $assessment.webview2_runtime_version
        stdout_bytes = (Get-Item -LiteralPath $stdoutPath).Length
        stderr_bytes = (Get-Item -LiteralPath $stderrPath).Length
        stdout_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $stdoutPath).Hash
        stderr_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $stderrPath).Hash
    })
}

$postRepository = Get-GitProof $repository
$postCore = Get-GitProof $coreRoot
$postWallet = Get-TreeFingerprint $walletRoot
$postProductionInstallation = Get-TreeFingerprint $productionInstallationRoot
$postProductionData = Get-TreeFingerprint $productionDataRoot
$productionExecutablePostHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $productionExecutable).Hash
$postWebview2 = Get-WebView2Provenance
$binaryPostHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $binary).Hash
$webview2InventoryBeforeHash = Get-StringSha256 (($webview2 | ConvertTo-Json -Depth 5 -Compress) -join '')
$webview2InventoryAfterHash = Get-StringSha256 (($postWebview2 | ConvertTo-Json -Depth 5 -Compress) -join '')
$actualWebView2Versions = @($records | ForEach-Object { $_.webview2_runtime_version } | Where-Object { $_ } | Sort-Object -Unique)
$installedWebView2Versions = @($webview2 | ForEach-Object { ([string]$_.product_version -split ' ')[0] } | Sort-Object -Unique)
$actualWebView2Proven = $actualWebView2Versions.Count -eq 1 -and
    @($records | Where-Object { [string]::IsNullOrWhiteSpace([string]$_.webview2_runtime_version) }).Count -eq 0 -and
    $installedWebView2Versions -contains $actualWebView2Versions[0]
if (-not $actualWebView2Proven -and $overall -eq 'Passed') { $overall = 'Inconclusive' }
$integrityPreserved = $preRepository.commit -eq $postRepository.commit -and
    $preRepository.tree -eq $postRepository.tree -and $postRepository.clean -and
    $preRepository.status_sha256 -eq $postRepository.status_sha256 -and
    $preCore.commit -eq $postCore.commit -and $preCore.tree -eq $postCore.tree -and
    $preCore.status_sha256 -eq $postCore.status_sha256 -and
    $preWallet.exists -eq $postWallet.exists -and $preWallet.file_count -eq $postWallet.file_count -and
    $preWallet.total_bytes -eq $postWallet.total_bytes -and $preWallet.sha256 -eq $postWallet.sha256 -and
    $preProductionInstallation.exists -eq $postProductionInstallation.exists -and
    $preProductionInstallation.file_count -eq $postProductionInstallation.file_count -and
    $preProductionInstallation.total_bytes -eq $postProductionInstallation.total_bytes -and
    $preProductionInstallation.sha256 -eq $postProductionInstallation.sha256 -and
    $preProductionData.exists -eq $postProductionData.exists -and
    $preProductionData.file_count -eq $postProductionData.file_count -and
    $preProductionData.total_bytes -eq $postProductionData.total_bytes -and
    $preProductionData.sha256 -eq $postProductionData.sha256 -and
    $productionExecutablePreHash -eq $productionExecutablePostHash -and
    $webview2InventoryBeforeHash -eq $webview2InventoryAfterHash -and
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
    actual_loaded_webview2_runtime_version = if ($actualWebView2Versions.Count -eq 1) { $actualWebView2Versions[0] } else { $null }
    actual_loaded_webview2_runtime_proven = $actualWebView2Proven
    framework_dependencies = $framework
    harness_source_hashes = $harnessSources
    cargo_lock_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $repository 'src-tauri\Cargo.lock')).Hash
    repository_before = $preRepository
    repository_after = $postRepository
    vision_core_before = $preCore
    vision_core_after = $postCore
    wallet_custody_before = $preWallet
    wallet_custody_after = $postWallet
    production_executable_sha256_before = $productionExecutablePreHash
    production_executable_sha256_after = $productionExecutablePostHash
    production_installation_before = $preProductionInstallation
    production_installation_after = $postProductionInstallation
    production_data_before = $preProductionData
    production_data_after = $postProductionData
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
