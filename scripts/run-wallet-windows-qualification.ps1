param(
    [Parameter(Mandatory = $true)]
    [ValidateSet(
        "SessionLock",
        "Suspend",
        "Hibernate",
        "CrossSessionOwner",
        "CrossSessionContender",
        "CrossSessionRecovery"
    )]
    [string]$Action,

    [Parameter(Mandatory = $true)]
    [string]$EvidenceDirectory,

    [ValidateRange(30, 1800)]
    [int]$TimeoutSeconds = 600
)

$ErrorActionPreference = "Stop"
$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$manifestPath = Join-Path $repositoryRoot "src-tauri\Cargo.toml"
$evidenceRoot = [System.IO.Path]::GetFullPath($EvidenceDirectory)
[System.IO.Directory]::CreateDirectory($evidenceRoot) | Out-Null

$commit = (& git -C $repositoryRoot rev-parse HEAD).Trim()
$tree = (& git -C $repositoryRoot rev-parse "HEAD^{tree}").Trim()
$status = & git -C $repositoryRoot status --porcelain
if ($LASTEXITCODE -ne 0 -or $status) {
    throw "Qualification requires a clean exact Git worktree."
}

$timestamp = [DateTime]::UtcNow.ToString("yyyyMMddTHHmmssZ")
$safeAction = $Action.ToLowerInvariant()
$evidencePath = Join-Path $evidenceRoot "wallet-$safeAction-$timestamp.log"
$header = @(
    "VISION_WALLET_WINDOWS_QUALIFICATION"
    "action=$Action"
    "commit=$commit"
    "tree=$tree"
    "started_utc=$([DateTime]::UtcNow.ToString('o'))"
    "machine=$env:COMPUTERNAME"
    "windows_user=$env:USERNAME"
    "timeout_seconds=$TimeoutSeconds"
)
[System.IO.File]::WriteAllLines($evidencePath, $header)

$testName = $null
switch ($Action) {
    "SessionLock" {
        $env:VISION_WALLET_QUALIFICATION_EVENT = "session_lock"
        $env:VISION_WALLET_QUALIFICATION_TIMEOUT_SECONDS = $TimeoutSeconds.ToString()
        $testName = "wallet::windows_lifecycle::tests::real_windows_security_event_revokes_runtime_authority"
    }
    "Suspend" {
        $env:VISION_WALLET_QUALIFICATION_EVENT = "suspend"
        $env:VISION_WALLET_QUALIFICATION_TIMEOUT_SECONDS = $TimeoutSeconds.ToString()
        $testName = "wallet::windows_lifecycle::tests::real_windows_security_event_revokes_runtime_authority"
    }
    "Hibernate" {
        $env:VISION_WALLET_QUALIFICATION_EVENT = "hibernate"
        $env:VISION_WALLET_QUALIFICATION_TIMEOUT_SECONDS = $TimeoutSeconds.ToString()
        $testName = "wallet::windows_lifecycle::tests::real_windows_security_event_revokes_runtime_authority"
    }
    "CrossSessionOwner" {
        $env:VISION_WALLET_QUALIFICATION_ROLE = "owner"
        $env:VISION_WALLET_QUALIFICATION_HOLD_SECONDS = $TimeoutSeconds.ToString()
        $testName = "wallet::runtime::tests::real_windows_cross_session_wallet_ownership"
    }
    "CrossSessionContender" {
        $env:VISION_WALLET_QUALIFICATION_ROLE = "contender"
        $testName = "wallet::runtime::tests::real_windows_cross_session_wallet_ownership"
    }
    "CrossSessionRecovery" {
        $env:VISION_WALLET_QUALIFICATION_ROLE = "recovery"
        $testName = "wallet::runtime::tests::real_windows_cross_session_wallet_ownership"
    }
}

try {
    & cargo test --manifest-path $manifestPath --release $testName -- --ignored --exact --nocapture --test-threads=1 2>&1 |
        Tee-Object -FilePath $evidencePath -Append
    $testExitCode = $LASTEXITCODE
} finally {
    Remove-Item Env:VISION_WALLET_QUALIFICATION_EVENT -ErrorAction SilentlyContinue
    Remove-Item Env:VISION_WALLET_QUALIFICATION_TIMEOUT_SECONDS -ErrorAction SilentlyContinue
    Remove-Item Env:VISION_WALLET_QUALIFICATION_ROLE -ErrorAction SilentlyContinue
    Remove-Item Env:VISION_WALLET_QUALIFICATION_HOLD_SECONDS -ErrorAction SilentlyContinue
}

[System.IO.File]::AppendAllLines(
    $evidencePath,
    @(
        "completed_utc=$([DateTime]::UtcNow.ToString('o'))"
        "exit_code=$testExitCode"
    )
)
$evidenceHash = (Get-FileHash -LiteralPath $evidencePath -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Output "Evidence: $evidencePath"
Write-Output "SHA-256: $evidenceHash"
if ($testExitCode -ne 0) {
    throw "Wallet Windows qualification failed with exit code $testExitCode."
}
