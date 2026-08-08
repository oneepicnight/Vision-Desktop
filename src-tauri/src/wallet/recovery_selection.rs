use super::{
    recovery::MAX_RECOVERY_JSON_BYTES,
    runtime::{
        RecoveryPathPurpose, RecoveryPathToken, RecoverySelectionPermit, WalletRuntimeError,
        WalletRuntimeState,
    },
};
use std::{
    fs,
    os::windows::{ffi::OsStrExt, fs::MetadataExt},
    panic::{catch_unwind, AssertUnwindSafe},
    path::{Component, Path, PathBuf, Prefix},
    sync::Arc,
};
use tauri::{Runtime, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, FilePath};
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

const RECOVERY_SUFFIX: &str = ".vision-recovery.json";
const RECOVERY_FILTER_EXTENSIONS: &[&str] = &["json"];
const DEFAULT_RECOVERY_FILE_NAME: &str = "vision-wallet.vision-recovery.json";
const MAX_WINDOWS_PATH_UNITS: usize = 32_767;
const MAX_WINDOWS_FILE_NAME_UNITS: usize = 255;

struct SelectionFailClosedGuard {
    runtime: Arc<WalletRuntimeState>,
    armed: bool,
}

impl SelectionFailClosedGuard {
    fn arm(runtime: Arc<WalletRuntimeState>) -> Self {
        Self {
            runtime,
            armed: true,
        }
    }

    fn commit(&mut self) {
        self.armed = false;
    }

    fn invalidate_or_terminate(&mut self) {
        if !self.armed {
            return;
        }
        match catch_unwind(AssertUnwindSafe(|| self.runtime.invalidate_all())) {
            Ok(Ok(())) => self.armed = false,
            Ok(Err(_)) | Err(_) => std::process::abort(),
        }
    }
}

impl Drop for SelectionFailClosedGuard {
    fn drop(&mut self) {
        self.invalidate_or_terminate();
    }
}

pub(in crate::wallet) fn select_recovery_destination<R, F>(
    window: &WebviewWindow<R>,
    runtime: Arc<WalletRuntimeState>,
    completion: F,
) -> Result<(), WalletRuntimeError>
where
    R: Runtime,
    F: FnOnce(Result<RecoveryPathToken, WalletRuntimeError>) + Send + 'static,
{
    let mut boundary = SelectionFailClosedGuard::arm(Arc::clone(&runtime));
    let result = catch_unwind(AssertUnwindSafe(|| {
        let owner_window = window.label().to_string();
        let permit = runtime
            .begin_recovery_path_selection(&owner_window, RecoveryPathPurpose::Destination)?;
        let callback_runtime = Arc::clone(&runtime);
        window
            .dialog()
            .file()
            .set_parent(window)
            .set_title("Choose encrypted Vision recovery destination")
            .set_file_name(DEFAULT_RECOVERY_FILE_NAME)
            .add_filter("Vision encrypted recovery", RECOVERY_FILTER_EXTENSIONS)
            .save_file(move |selected| {
                run_selection_callback(callback_runtime, || {
                    let result = finish_destination_selection(&runtime, permit, selected);
                    completion(result);
                });
            });
        Ok(())
    }));
    match result {
        Ok(Ok(())) => {
            boundary.commit();
            Ok(())
        }
        Ok(Err(error)) => {
            boundary.invalidate_or_terminate();
            Err(error)
        }
        Err(_) => {
            boundary.invalidate_or_terminate();
            Err(WalletRuntimeError::RuntimeUnavailable)
        }
    }
}

pub(in crate::wallet) fn select_recovery_source<R, F>(
    window: &WebviewWindow<R>,
    runtime: Arc<WalletRuntimeState>,
    completion: F,
) -> Result<(), WalletRuntimeError>
where
    R: Runtime,
    F: FnOnce(Result<RecoveryPathToken, WalletRuntimeError>) + Send + 'static,
{
    let mut boundary = SelectionFailClosedGuard::arm(Arc::clone(&runtime));
    let result = catch_unwind(AssertUnwindSafe(|| {
        let owner_window = window.label().to_string();
        let permit =
            runtime.begin_recovery_path_selection(&owner_window, RecoveryPathPurpose::Source)?;
        let callback_runtime = Arc::clone(&runtime);
        window
            .dialog()
            .file()
            .set_parent(window)
            .set_title("Choose encrypted Vision recovery source")
            .add_filter("Vision encrypted recovery", RECOVERY_FILTER_EXTENSIONS)
            .pick_file(move |selected| {
                run_selection_callback(callback_runtime, || {
                    let result = finish_source_selection(&runtime, permit, selected);
                    completion(result);
                });
            });
        Ok(())
    }));
    match result {
        Ok(Ok(())) => {
            boundary.commit();
            Ok(())
        }
        Ok(Err(error)) => {
            boundary.invalidate_or_terminate();
            Err(error)
        }
        Err(_) => {
            boundary.invalidate_or_terminate();
            Err(WalletRuntimeError::RuntimeUnavailable)
        }
    }
}

fn run_selection_callback(runtime: Arc<WalletRuntimeState>, callback: impl FnOnce()) {
    let mut boundary = SelectionFailClosedGuard::arm(runtime);
    if catch_unwind(AssertUnwindSafe(callback)).is_ok() {
        boundary.commit();
    } else {
        boundary.invalidate_or_terminate();
    }
}

fn finish_destination_selection(
    runtime: &WalletRuntimeState,
    permit: RecoverySelectionPermit,
    selected: Option<FilePath>,
) -> Result<RecoveryPathToken, WalletRuntimeError> {
    let Some(FilePath::Path(path)) = selected else {
        return Err(if selected.is_none() {
            WalletRuntimeError::RecoverySelectionCancelled
        } else {
            WalletRuntimeError::RecoveryDestinationInvalid
        });
    };
    let path = validate_destination(path)?;
    runtime.complete_recovery_path_selection(permit, path)
}

fn finish_source_selection(
    runtime: &WalletRuntimeState,
    permit: RecoverySelectionPermit,
    selected: Option<FilePath>,
) -> Result<RecoveryPathToken, WalletRuntimeError> {
    let Some(FilePath::Path(path)) = selected else {
        return Err(if selected.is_none() {
            WalletRuntimeError::RecoverySelectionCancelled
        } else {
            WalletRuntimeError::RecoverySourceInvalid
        });
    };
    let path = validate_source(path)?;
    runtime.complete_recovery_path_selection(permit, path)
}

fn validate_destination(path: PathBuf) -> Result<PathBuf, WalletRuntimeError> {
    validate_local_path_shape(&path, WalletRuntimeError::RecoveryDestinationInvalid)?;
    let path = normalize_destination_suffix(path)?;
    validate_local_path_shape(&path, WalletRuntimeError::RecoveryDestinationInvalid)?;
    let parent = path
        .parent()
        .ok_or(WalletRuntimeError::RecoveryDestinationInvalid)?;
    validate_directory_chain(parent, WalletRuntimeError::RecoveryDestinationInvalid)?;

    match fs::symlink_metadata(&path) {
        Ok(_) => Err(WalletRuntimeError::RecoveryDestinationExists),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(path),
        Err(_) => Err(WalletRuntimeError::RecoveryDestinationInvalid),
    }
}

fn validate_source(path: PathBuf) -> Result<PathBuf, WalletRuntimeError> {
    validate_local_path_shape(&path, WalletRuntimeError::RecoverySourceInvalid)?;
    validate_exact_suffix(&path, WalletRuntimeError::RecoverySourceInvalid)?;
    let parent = path
        .parent()
        .ok_or(WalletRuntimeError::RecoverySourceInvalid)?;
    validate_directory_chain(parent, WalletRuntimeError::RecoverySourceInvalid)?;
    let metadata =
        fs::symlink_metadata(&path).map_err(|_| WalletRuntimeError::RecoverySourceInvalid)?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_RECOVERY_JSON_BYTES as u64
    {
        return Err(WalletRuntimeError::RecoverySourceInvalid);
    }
    Ok(path)
}

fn normalize_destination_suffix(mut path: PathBuf) -> Result<PathBuf, WalletRuntimeError> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(WalletRuntimeError::RecoveryDestinationInvalid)?;
    let lower = file_name.to_ascii_lowercase();
    let base = if lower.ends_with(RECOVERY_SUFFIX) {
        &file_name[..file_name.len() - RECOVERY_SUFFIX.len()]
    } else if lower.ends_with(".json") {
        &file_name[..file_name.len() - ".json".len()]
    } else {
        file_name
    };
    if base.is_empty() || base == "." || base == ".." || base.contains(':') {
        return Err(WalletRuntimeError::RecoveryDestinationInvalid);
    }
    let normalized = format!("{base}{RECOVERY_SUFFIX}");
    if normalized.encode_utf16().count() > MAX_WINDOWS_FILE_NAME_UNITS {
        return Err(WalletRuntimeError::RecoveryDestinationInvalid);
    }
    path.set_file_name(normalized);
    Ok(path)
}

fn validate_exact_suffix(path: &Path, error: WalletRuntimeError) -> Result<(), WalletRuntimeError> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(error)?;
    let lower = file_name.to_ascii_lowercase();
    let base = lower.strip_suffix(RECOVERY_SUFFIX).ok_or(error)?;
    if base.is_empty() || base == "." || base == ".." || base.contains(':') {
        return Err(error);
    }
    Ok(())
}

fn validate_local_path_shape(
    path: &Path,
    error: WalletRuntimeError,
) -> Result<(), WalletRuntimeError> {
    if path.as_os_str().encode_wide().count() > MAX_WINDOWS_PATH_UNITS {
        return Err(error);
    }
    let mut components = path.components();
    if !matches!(
        components.next(),
        Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_))
    ) || !matches!(components.next(), Some(Component::RootDir))
    {
        return Err(error);
    }
    for component in components {
        let Component::Normal(value) = component else {
            return Err(error);
        };
        if value.encode_wide().any(|unit| unit == b':' as u16) {
            return Err(error);
        }
    }
    Ok(())
}

fn validate_directory_chain(
    directory: &Path,
    error: WalletRuntimeError,
) -> Result<(), WalletRuntimeError> {
    let mut current = PathBuf::new();
    for component in directory.components() {
        current.push(component.as_os_str());
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        let metadata = fs::symlink_metadata(&current).map_err(|_| error)?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 || !metadata.is_dir() {
            return Err(error);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn destination_is_local_new_and_normalized_to_exact_suffix() {
        let directory = tempfile::tempdir().unwrap();
        let selected = directory.path().join("offline-backup.json");
        let normalized = validate_destination(selected).unwrap();
        assert_eq!(
            normalized.file_name().unwrap(),
            "offline-backup.vision-recovery.json"
        );

        fs::write(&normalized, b"already exists").unwrap();
        assert_eq!(
            validate_destination(normalized).unwrap_err(),
            WalletRuntimeError::RecoveryDestinationExists
        );
    }

    #[test]
    fn destination_rejects_relative_paths_and_alternate_data_streams() {
        assert_eq!(
            validate_destination(PathBuf::from("relative.json")).unwrap_err(),
            WalletRuntimeError::RecoveryDestinationInvalid
        );
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(
            validate_destination(directory.path().join("wallet:stream.json")).unwrap_err(),
            WalletRuntimeError::RecoveryDestinationInvalid
        );
        for unsafe_path in [
            PathBuf::from(r"\\server\share\backup.json"),
            PathBuf::from(r"\\?\C:\backup.json"),
            PathBuf::from(r"C:\safe\..\backup.json"),
        ] {
            assert_eq!(
                validate_destination(unsafe_path).unwrap_err(),
                WalletRuntimeError::RecoveryDestinationInvalid
            );
        }
    }

    #[test]
    fn source_requires_an_exact_bounded_regular_recovery_file() {
        let directory = tempfile::tempdir().unwrap();
        let valid = directory.path().join("backup.vision-recovery.json");
        fs::write(&valid, b"encrypted").unwrap();
        assert_eq!(validate_source(valid.clone()).unwrap(), valid);

        let wrong_suffix = directory.path().join("backup.json");
        fs::write(&wrong_suffix, b"encrypted").unwrap();
        assert_eq!(
            validate_source(wrong_suffix).unwrap_err(),
            WalletRuntimeError::RecoverySourceInvalid
        );

        let empty = directory.path().join("empty.vision-recovery.json");
        fs::write(&empty, []).unwrap();
        assert_eq!(
            validate_source(empty).unwrap_err(),
            WalletRuntimeError::RecoverySourceInvalid
        );

        let oversized = directory.path().join("large.vision-recovery.json");
        let mut file = fs::File::create(&oversized).unwrap();
        file.write_all(&vec![0_u8; MAX_RECOVERY_JSON_BYTES + 1])
            .unwrap();
        assert_eq!(
            validate_source(oversized).unwrap_err(),
            WalletRuntimeError::RecoverySourceInvalid
        );
    }

    #[test]
    fn cancellation_and_invalid_selection_revoke_pending_authority() {
        let runtime = Arc::new(WalletRuntimeState::for_test());
        let permit = runtime
            .begin_recovery_path_selection("main", RecoveryPathPurpose::Destination)
            .unwrap();
        assert_eq!(
            finish_destination_selection(&runtime, permit, None).err(),
            Some(WalletRuntimeError::RecoverySelectionCancelled)
        );
        runtime
            .begin_operation("main", super::super::runtime::WalletOperationKind::Create)
            .unwrap();

        let runtime = Arc::new(WalletRuntimeState::for_test());
        let permit = runtime
            .begin_recovery_path_selection("main", RecoveryPathPurpose::Source)
            .unwrap();
        assert_eq!(
            finish_source_selection(
                &runtime,
                permit,
                Some(FilePath::Path(PathBuf::from("relative.json"))),
            )
            .err(),
            Some(WalletRuntimeError::RecoverySourceInvalid)
        );
        runtime
            .begin_operation("main", super::super::runtime::WalletOperationKind::Restore)
            .unwrap();

        let runtime = Arc::new(WalletRuntimeState::for_test());
        let permit = runtime
            .begin_recovery_path_selection("main", RecoveryPathPurpose::Source)
            .unwrap();
        assert_eq!(
            finish_source_selection(
                &runtime,
                permit,
                Some(FilePath::Url(
                    "file:///C:/backup.vision-recovery.json".parse().unwrap(),
                )),
            )
            .err(),
            Some(WalletRuntimeError::RecoverySourceInvalid)
        );
    }

    #[test]
    fn validated_selection_returns_only_a_consumable_runtime_token() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = Arc::new(WalletRuntimeState::for_test());
        let permit = runtime
            .begin_recovery_path_selection("main", RecoveryPathPurpose::Destination)
            .unwrap();
        let token = finish_destination_selection(
            &runtime,
            permit,
            Some(FilePath::Path(directory.path().join("backup.json"))),
        )
        .unwrap();
        assert_eq!(token.as_str().len(), 64);
        let selected = runtime
            .consume_recovery_path("main", RecoveryPathPurpose::Destination, token.as_str())
            .unwrap();
        assert_eq!(selected.file_name().unwrap(), "backup.vision-recovery.json");
    }

    #[test]
    fn uncommitted_selection_permit_drop_fully_invalidates_authority() {
        let runtime = Arc::new(WalletRuntimeState::for_test());
        let permit = runtime
            .begin_recovery_path_selection("main", RecoveryPathPurpose::Destination)
            .unwrap();
        drop(permit);

        runtime
            .begin_operation("main", super::super::runtime::WalletOperationKind::Create)
            .unwrap();
    }

    #[test]
    fn callback_panic_revokes_completed_path_authority() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = Arc::new(WalletRuntimeState::for_test());
        let permit = runtime
            .begin_recovery_path_selection("main", RecoveryPathPurpose::Destination)
            .unwrap();
        let callback_runtime = Arc::clone(&runtime);
        let completion_runtime = Arc::clone(&runtime);
        run_selection_callback(callback_runtime, move || {
            completion_runtime
                .complete_recovery_path_selection(
                    permit,
                    directory.path().join("panic.vision-recovery.json"),
                )
                .unwrap();
            panic!("injected recovery selection callback panic");
        });

        assert_eq!(
            runtime
                .consume_recovery_path(
                    "main",
                    RecoveryPathPurpose::Destination,
                    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                )
                .unwrap_err(),
            WalletRuntimeError::PathAuthorizationInvalid,
        );
        runtime
            .begin_operation("main", super::super::runtime::WalletOperationKind::Create)
            .unwrap();
    }

    #[test]
    fn callback_panics_around_path_validation_revoke_pending_authority() {
        for panic_after_validation in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let runtime = Arc::new(WalletRuntimeState::for_test());
            let permit = runtime
                .begin_recovery_path_selection("main", RecoveryPathPurpose::Destination)
                .unwrap();
            let callback_runtime = Arc::clone(&runtime);
            run_selection_callback(callback_runtime, move || {
                if panic_after_validation {
                    validate_destination(directory.path().join("panic.json")).unwrap();
                }
                let _permit_must_drop_during_unwind = permit;
                panic!("injected recovery selection validation panic");
            });

            runtime
                .begin_operation("main", super::super::runtime::WalletOperationKind::Create)
                .unwrap();
        }
    }
}
