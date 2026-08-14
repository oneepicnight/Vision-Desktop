param(
    [Parameter(Mandatory = $true)]
    [string]$EvidenceRoot,

    [string]$RepositoryRoot = (Split-Path -Parent $PSScriptRoot)
)

$ErrorActionPreference = 'Stop'

$expectedManifestHash = '35f3233003a0b0c39d9331e0d3771b6d472aef3e556a516557f2b62d0aacb64a'
$expectedAcceptanceHash = '2576e87f46dd7cd878a5aa39daebc11e027d23bbeeeca54e5db6110cec9e3449'
$expectedRuntimeManifestHash = 'cf713d116acca7a848d0537968f81373ee14a965fb3855a6480a59f4971536ec'
$expectedCandidateHash = '8082d57c0f4a5cb82af9696fe4d53aeb65fcb280c062afe81abdcfe78e12ed28'
$expectedCandidateSize = 4486144
$expectedSourceCommit = '890c98a02c7147e166805fe52002d22d1fcd81f9'
$expectedSourceTree = '2ae583bbfc887490b8af1398aead7b916796700c'

function Resolve-FixedLocalDirectory([string]$Path) {
    $resolved = [System.IO.Path]::GetFullPath($Path)
    $root = [System.IO.Path]::GetPathRoot($resolved)
    if ([string]::IsNullOrWhiteSpace($root)) {
        throw 'Path has no filesystem root.'
    }
    $drive = [System.IO.DriveInfo]::new($root)
    if ($drive.DriveType -ne [System.IO.DriveType]::Fixed) {
        throw 'Path is not on a fixed local disk.'
    }
    return $resolved.TrimEnd([System.IO.Path]::DirectorySeparatorChar)
}

function Resolve-ContainedPath([string]$Root, [string]$RelativePath) {
    $prefix = $Root.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
    $candidate = [System.IO.Path]::GetFullPath((Join-Path $Root $RelativePath.Replace('/', [System.IO.Path]::DirectorySeparatorChar)))
    if (-not $candidate.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Evidence path escapes its package root: $RelativePath"
    }
    return $candidate
}

$evidence = Resolve-FixedLocalDirectory $EvidenceRoot
$repository = Resolve-FixedLocalDirectory $RepositoryRoot
$acceptancePath = Join-Path $repository 'bundled\core\windows-x64\integration-acceptance.json'
$manifestPath = Join-Path $evidence 'FINAL_ARTIFACT_MANIFEST.json'
$sidecarPath = Join-Path $evidence 'FINAL_ARTIFACT_MANIFEST.sha256'

if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf) -or
    -not (Test-Path -LiteralPath $sidecarPath -PathType Leaf)) {
    throw 'The accepted evidence manifest or detached checksum is missing.'
}
if (-not (Test-Path -LiteralPath $acceptancePath -PathType Leaf)) {
    throw 'The separate Vision Desktop integration acceptance record is missing.'
}

$actualAcceptanceHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $acceptancePath).Hash.ToLowerInvariant()
if ($actualAcceptanceHash -ne $expectedAcceptanceHash) {
    throw 'The Vision Desktop integration acceptance record is not the independently pinned record.'
}
$acceptance = Get-Content -Raw -LiteralPath $acceptancePath | ConvertFrom-Json
if ($acceptance.schema_version -ne 1 -or
    $acceptance.record_type -ne 'vision-desktop-core-integration-acceptance-v1' -or
    $acceptance.vision_desktop_integration_authorized -ne $true -or
    $acceptance.authorized_scope -ne 'isolated_artifact_admission_and_controlled_compatibility_validation' -or
    $acceptance.authorizing_principal -ne 'Vision Desktop owner' -or
    $acceptance.authorization_reference -ne 'explicit integration authorization following frozen artifact acceptance' -or
    $acceptance.source_commit -ne $expectedSourceCommit -or
    $acceptance.source_tree -ne $expectedSourceTree -or
    $acceptance.candidate_sha256 -ne $expectedCandidateHash -or
    [int64]$acceptance.candidate_size_bytes -ne $expectedCandidateSize -or
    $acceptance.evidence_manifest_sha256 -ne $expectedManifestHash -or
    $acceptance.runtime_manifest_sha256 -ne $expectedRuntimeManifestHash -or
    $acceptance.accepted_evidence_limitations.Count -ne 2 -or
    $acceptance.accepted_evidence_limitations[0] -ne 'authenticated_ci_archive_covers_223e2f745ebb5f7eb0d48c88397684b9037767bc_not_the_exact_candidate' -or
    $acceptance.accepted_evidence_limitations[1] -ne 'exact_candidate_runtime_and_deterministic_compatibility_qualification_remain_required') {
    throw 'The Vision Desktop integration acceptance record does not authorize this exact frozen identity.'
}

$actualManifestHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $manifestPath).Hash.ToLowerInvariant()
if ($actualManifestHash -ne $expectedManifestHash) {
    throw 'The accepted evidence manifest hash does not match the frozen identity.'
}
$sidecar = (Get-Content -Raw -LiteralPath $sidecarPath).Trim()
if ($sidecar -ne "$expectedManifestHash  FINAL_ARTIFACT_MANIFEST.json") {
    throw 'The detached evidence-manifest checksum is invalid.'
}

$manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
if ($manifest.schema -ne 'vision-core-wallet-compatibility-final-artifact-manifest-v1' -or
    $manifest.source.commit -ne $expectedSourceCommit -or
    $manifest.source.tree -ne $expectedSourceTree -or
    $manifest.candidate.path -ne 'candidate/vision-core.exe' -or
    $manifest.candidate.sha256 -ne $expectedCandidateHash -or
    [int64]$manifest.candidate.size_bytes -ne $expectedCandidateSize -or
    $manifest.candidate.pe_architecture -ne 'x86_64') {
    throw 'The evidence manifest does not describe the frozen candidate.'
}
if ($manifest.downstream_authority.vision_desktop_integration_authorized -ne $false -or
    $manifest.downstream_authority.statement -ne 'Vision Desktop integration requires separate explicit authorization.') {
    throw 'The immutable evidence package no longer retains its original downstream-authority boundary.'
}

$verifiedBytes = [int64]0
$verifiedFiles = 0
foreach ($entry in $manifest.files) {
    $path = Resolve-ContainedPath $evidence ([string]$entry.path)
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Evidence inventory file is missing: $($entry.path)"
    }
    $item = Get-Item -LiteralPath $path
    if ($item.Length -ne [int64]$entry.size_bytes) {
        throw "Evidence inventory size mismatch: $($entry.path)"
    }
    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
    if ($hash -ne [string]$entry.sha256) {
        throw "Evidence inventory hash mismatch: $($entry.path)"
    }
    $verifiedBytes += $item.Length
    $verifiedFiles += 1
}
if ($verifiedFiles -ne [int]$manifest.file_count -or
    $verifiedBytes -ne [int64]$manifest.total_size_bytes) {
    throw 'The evidence inventory totals are inconsistent.'
}

$source = Resolve-ContainedPath $evidence 'candidate/vision-core.exe'
$sourceItem = Get-Item -LiteralPath $source
$sourceHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $source).Hash.ToLowerInvariant()
if ($sourceItem.Length -ne $expectedCandidateSize -or $sourceHash -ne $expectedCandidateHash) {
    throw 'The frozen source executable does not match its accepted identity.'
}

$destinationDirectory = Join-Path $repository 'bundled\core\windows-x64'
$destination = Join-Path $destinationDirectory 'vision-core.exe'
New-Item -ItemType Directory -Path $destinationDirectory -Force | Out-Null
if (Test-Path -LiteralPath $destination -PathType Leaf) {
    $existing = Get-Item -LiteralPath $destination
    $existingHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $destination).Hash.ToLowerInvariant()
    if ($existing.Length -eq $expectedCandidateSize -and $existingHash -eq $expectedCandidateHash) {
        Write-Output "Frozen Core is already staged: $destination"
        exit 0
    }
    throw 'The staging destination exists with different bytes; it was not replaced.'
}

$temporary = Join-Path $destinationDirectory ('.vision-core.' + [Guid]::NewGuid().ToString('N') + '.staging')
try {
    Copy-Item -LiteralPath $source -Destination $temporary
    $temporaryItem = Get-Item -LiteralPath $temporary
    $temporaryHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $temporary).Hash.ToLowerInvariant()
    if ($temporaryItem.Length -ne $expectedCandidateSize -or $temporaryHash -ne $expectedCandidateHash) {
        throw 'The staging copy failed post-copy verification.'
    }
    Move-Item -LiteralPath $temporary -Destination $destination
} finally {
    if (Test-Path -LiteralPath $temporary -PathType Leaf) {
        Remove-Item -LiteralPath $temporary -Force
    }
}

$finalItem = Get-Item -LiteralPath $destination
$finalHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $destination).Hash.ToLowerInvariant()
if ($finalItem.Length -ne $expectedCandidateSize -or $finalHash -ne $expectedCandidateHash) {
    throw 'The final staged executable failed verification.'
}

Write-Output "Staged frozen Core: $destination"
Write-Output "Candidate SHA-256: $finalHash"
Write-Output "Candidate size: $($finalItem.Length)"
Write-Output "Evidence files verified: $verifiedFiles"
Write-Output "Evidence bytes verified: $verifiedBytes"
