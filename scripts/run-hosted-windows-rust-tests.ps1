$ErrorActionPreference = "Stop"

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $repositoryRoot "src-tauri\Cargo.toml"

# GitHub-hosted Windows runners are Windows Server. Vision Wallet deliberately rejects Server
# before custody state exists, and its protected-storage tests require the reviewed Windows 11
# Client filesystem and host boundary. This allowlist exercises Rust logic that is valid on the
# hosted runner without weakening or bypassing the production host policy. The complete serialized
# suite remains mandatory on a supported Windows 11 Client workstation.
$libraryFilters = @(
    "api::tests",
    "config::tests",
    "core_manifest::tests",
    "paths::tests",
    "reports::tests",
    "supervisor::tests",
    "wallet::account::tests",
    "wallet::activation::tests",
    "wallet::amount::tests",
    "wallet::contract::tests",
    "wallet::core_client::tests",
    "wallet::device_protection::ownership_tests",
    "wallet::device_protection::tests",
    "wallet::kdf::tests",
    "wallet::native_secret_buffer::tests",
    "wallet::panic_policy::tests",
    "wallet::public_request::tests",
    "wallet::receipt::tests",
    "wallet::recovery::tests",
    "wallet::secret_input::tests",
    "wallet::secrets::tests",
    "wallet::session::tests",
    "wallet::submission::tests",
    "wallet::transaction::tests"
)

foreach ($filter in $libraryFilters) {
    Write-Host "Running hosted-safe Rust filter: $filter"
    & cargo test --locked --manifest-path $manifestPath --lib $filter -- --test-threads=1
    if ($LASTEXITCODE -ne 0) {
        throw "Hosted-safe Rust filter failed: $filter"
    }
}

foreach ($integrationTest in @("tauri_acl", "webview_security")) {
    Write-Host "Running hosted-safe integration test: $integrationTest"
    & cargo test --locked --manifest-path $manifestPath --test $integrationTest -- --test-threads=1
    if ($LASTEXITCODE -ne 0) {
        throw "Hosted-safe integration test failed: $integrationTest"
    }
}
