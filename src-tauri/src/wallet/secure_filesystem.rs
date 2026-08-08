#![cfg(windows)]

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Seek, SeekFrom},
    mem::{offset_of, size_of},
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Component, Path, PathBuf, Prefix},
    ptr,
};
use windows_sys::Win32::{
    Foundation::{GENERIC_READ, GENERIC_WRITE},
    Storage::FileSystem::{
        FileRenameInfo, SetFileInformationByHandle, DELETE, FILE_ATTRIBUTE_DIRECTORY,
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_RENAME_INFO, FILE_SHARE_READ, FILE_SHARE_WRITE, READ_CONTROL, WRITE_DAC,
    },
};

/// Holds every directory component open without delete sharing so the chain cannot be
/// renamed or replaced while a custody file is opened and used.
pub(in crate::wallet) struct DirectoryChainGuard {
    _directories: Vec<File>,
}

impl DirectoryChainGuard {
    pub(in crate::wallet) fn open_existing(path: &Path) -> io::Result<Self> {
        Self::walk(path, false)
    }

    pub(in crate::wallet) fn ensure(path: &Path) -> io::Result<Self> {
        Self::walk(path, true)
    }

    fn walk(path: &Path, create_missing: bool) -> io::Result<Self> {
        validate_absolute_disk_path(path)?;
        let mut current = PathBuf::new();
        let mut directories = Vec::new();
        for component in path.components() {
            current.push(component.as_os_str());
            if matches!(component, Component::Prefix(_)) {
                continue;
            }
            let file = match open_directory(&current) {
                Ok(file) => file,
                Err(error) if create_missing && error.kind() == io::ErrorKind::NotFound => {
                    match fs::create_dir(&current) {
                        Ok(()) => {}
                        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                        Err(error) => return Err(error),
                    }
                    open_directory(&current)?
                }
                Err(error) => return Err(error),
            };
            directories.push(file);
        }
        if directories.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "secure directory chain is empty",
            ));
        }
        Ok(Self {
            _directories: directories,
        })
    }
}

pub(in crate::wallet) fn open_existing_file(path: &Path) -> io::Result<File> {
    open_existing_file_with_share(path, FILE_SHARE_READ)
}

fn open_existing_file_with_share(path: &Path, share_mode: u32) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .share_mode(share_mode)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    let file = options.open(path)?;
    validate_regular_file(&file)?;
    Ok(file)
}

pub(in crate::wallet) fn create_new_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .write(true)
        .create_new(true)
        .share_mode(0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    let file = options.open(path)?;
    validate_regular_file(&file)?;
    Ok(file)
}

/// Creates a non-overwriting vault staging file whose open handle owns every later operation,
/// including security changes, publication, and verification. The share mode deliberately denies
/// delete and write access so another process cannot move or replace the source while it is open.
pub(in crate::wallet) fn create_new_publishable_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create_new(true)
        .access_mode(GENERIC_READ | GENERIC_WRITE | DELETE | READ_CONTROL | WRITE_DAC)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    let file = options.open(path)?;
    validate_regular_file(&file)?;
    Ok(file)
}

/// Atomically gives the already-open file its final non-replacing name. The source of the rename
/// is the validated handle rather than a pathname, so a path substitution cannot redirect which
/// file is published.
pub(in crate::wallet) fn publish_open_file(file: &File, destination: &Path) -> io::Result<()> {
    rename_open_file(file, destination, false)
}

/// Atomically replaces an existing destination with the already-open, validated staging file.
/// The source identity is handle-bound and the operation never exposes a partially written final
/// file. Callers must hold the destination directory chain open for the complete operation.
pub(in crate::wallet) fn replace_with_open_file(file: &File, destination: &Path) -> io::Result<()> {
    rename_open_file(file, destination, true)
}

fn rename_open_file(file: &File, destination: &Path, replace_existing: bool) -> io::Result<()> {
    validate_absolute_disk_path(destination)?;
    let destination_wide: Vec<u16> = destination.as_os_str().encode_wide().collect();
    if destination_wide.is_empty() || destination_wide.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "vault destination path is invalid",
        ));
    }
    let name_bytes = destination_wide
        .len()
        .checked_mul(size_of::<u16>())
        .and_then(|size| u32::try_from(size).ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "vault path is too long"))?;
    // `FileNameLength` excludes a terminator, but reserve and retain one UTF-16 NUL after the
    // flexible array. Some Windows paths otherwise land exactly on the allocation boundary and
    // can be published with trailing garbage despite the explicit byte count.
    let buffer_size = offset_of!(FILE_RENAME_INFO, FileName)
        .checked_add(name_bytes as usize)
        .and_then(|size| size.checked_add(size_of::<u16>()))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "vault path is too long"))?;
    let buffer_size_u32 = u32::try_from(buffer_size)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "vault path is too long"))?;
    let words = buffer_size.div_ceil(size_of::<usize>());
    let mut buffer = vec![0_usize; words];
    let information = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    // SAFETY: `buffer` is pointer-aligned and large enough for the fixed header plus every UTF-16
    // destination unit. `file` owns a valid handle with DELETE access. `replace_existing` is an
    // explicit caller policy; the Windows rename is atomic with respect to the final path.
    let renamed = unsafe {
        ptr::write(information, FILE_RENAME_INFO::default());
        (*information).Anonymous.ReplaceIfExists = replace_existing;
        (*information).RootDirectory = ptr::null_mut();
        (*information).FileNameLength = name_bytes;
        ptr::copy_nonoverlapping(
            destination_wide.as_ptr(),
            (*information).FileName.as_mut_ptr(),
            destination_wide.len(),
        );
        SetFileInformationByHandle(
            file.as_raw_handle().cast(),
            FileRenameInfo,
            information.cast(),
            buffer_size_u32,
        )
    };
    if renamed == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(in crate::wallet) fn rewind(file: &mut File) -> io::Result<()> {
    file.seek(SeekFrom::Start(0)).map(|_| ())
}

fn open_directory(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    let file = options.open(path)?;
    let attributes = file.metadata()?.file_attributes();
    if attributes & FILE_ATTRIBUTE_DIRECTORY == 0 || attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "custody directory is not a regular directory",
        ));
    }
    Ok(file)
}

fn validate_regular_file(file: &File) -> io::Result<()> {
    let attributes = file.metadata()?.file_attributes();
    if attributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "custody file is not a regular file",
        ));
    }
    Ok(())
}

fn validate_absolute_disk_path(path: &Path) -> io::Result<()> {
    let mut components = path.components();
    if !matches!(
        components.next(),
        Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_))
    ) || !matches!(components.next(), Some(Component::RootDir))
        || components.any(|component| match component {
            Component::Normal(value) => value.encode_wide().any(|unit| unit == b':' as u16),
            _ => true,
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "custody path is not an absolute disk path",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn guarded_chain_and_same_handle_file_io_work_for_regular_paths() {
        let directory = tempfile::tempdir().unwrap();
        let nested = directory.path().join("one").join("two");
        let _guard = DirectoryChainGuard::ensure(&nested).unwrap();
        let path = nested.join("encrypted.bin");
        let mut created = create_new_file(&path).unwrap();
        created.write_all(b"encrypted").unwrap();
        created.sync_all().unwrap();
        drop(created);

        let _guard = DirectoryChainGuard::open_existing(&nested).unwrap();
        let mut opened = open_existing_file(&path).unwrap();
        let mut bytes = Vec::new();
        opened.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"encrypted");
    }

    #[test]
    fn publish_uses_the_open_source_handle_and_never_replaces_a_destination() {
        let directory = tempfile::tempdir().unwrap();
        let guarded = directory.path().join("guarded");
        let _guard = DirectoryChainGuard::ensure(&guarded).unwrap();
        let staged_path = guarded.join("staged.json");
        let final_path = guarded.join("vault.json");
        let moved_path = guarded.join("attacker-moved.json");
        let mut staged = create_new_publishable_file(&staged_path).unwrap();
        staged.write_all(b"encrypted-vault").unwrap();
        staged.sync_all().unwrap();

        // The staging handle denies delete sharing. A competing pathname rename cannot move the
        // source out from under handle-bound publication.
        assert!(fs::rename(&staged_path, &moved_path).is_err());
        publish_open_file(&staged, &final_path).unwrap();
        assert!(!staged_path.exists());
        assert_eq!(fs::read(&final_path).unwrap(), b"encrypted-vault");

        let second_stage = guarded.join("second-stage.json");
        let second = create_new_publishable_file(&second_stage).unwrap();
        assert!(publish_open_file(&second, &final_path).is_err());
        assert_eq!(fs::read(&final_path).unwrap(), b"encrypted-vault");
    }

    #[test]
    fn publication_preserves_the_exact_utf16_filename_at_allocation_boundaries() {
        let directory = tempfile::tempdir().unwrap();
        let guarded = directory.path().join("guarded");
        let _guard = DirectoryChainGuard::ensure(&guarded).unwrap();
        let staged_path = guarded.join("stage.bin");
        let destination = guarded.join("wallet.submission-reconciliation.json");
        let mut staged = create_new_publishable_file(&staged_path).unwrap();
        staged.write_all(b"authenticated-record").unwrap();
        staged.sync_all().unwrap();
        publish_open_file(&staged, &destination).unwrap();

        let entries = fs::read_dir(&guarded)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            entries,
            vec!["wallet.submission-reconciliation.json".to_string()]
        );
        assert_eq!(fs::read(destination).unwrap(), b"authenticated-record");
    }

    #[test]
    fn replacement_is_handle_bound_atomic_and_blocked_by_a_held_destination() {
        let directory = tempfile::tempdir().unwrap();
        let guarded = directory.path().join("guarded");
        let _guard = DirectoryChainGuard::ensure(&guarded).unwrap();
        let final_path = guarded.join("activity.jsonl");
        fs::write(&final_path, b"old-complete-journal\n").unwrap();

        let staged_path = guarded.join("staged.jsonl");
        let mut staged = create_new_publishable_file(&staged_path).unwrap();
        staged.write_all(b"new-complete-journal\n").unwrap();
        staged.sync_all().unwrap();

        let held_destination = open_existing_file(&final_path).unwrap();
        assert!(replace_with_open_file(&staged, &final_path).is_err());
        assert_eq!(fs::read(&final_path).unwrap(), b"old-complete-journal\n");
        drop(held_destination);

        replace_with_open_file(&staged, &final_path).unwrap();
        assert!(!staged_path.exists());
        assert_eq!(fs::read(&final_path).unwrap(), b"new-complete-journal\n");
    }

    #[test]
    fn replacement_removes_a_destination_reparse_point_without_following_it() {
        use std::os::windows::fs::symlink_file;

        let directory = tempfile::tempdir().unwrap();
        let guarded = directory.path().join("guarded");
        let _guard = DirectoryChainGuard::ensure(&guarded).unwrap();
        let victim = guarded.join("victim.jsonl");
        fs::write(&victim, b"victim-must-not-change\n").unwrap();
        let destination = guarded.join("activity.jsonl");

        // Symlink creation can be disabled by Windows policy. If it is available, atomic
        // replacement must replace the directory entry rather than following its target.
        if symlink_file(&victim, &destination).is_ok() {
            let staging_path = guarded.join("staged-for-reparse-test.jsonl");
            let mut staged = create_new_publishable_file(&staging_path).unwrap();
            staged.write_all(b"new-complete-journal\n").unwrap();
            staged.sync_all().unwrap();
            replace_with_open_file(&staged, &destination).unwrap();

            assert_eq!(fs::read(&victim).unwrap(), b"victim-must-not-change\n");
            assert!(!fs::symlink_metadata(&destination)
                .unwrap()
                .file_type()
                .is_symlink());
            assert_eq!(fs::read(&destination).unwrap(), b"new-complete-journal\n");
        }
    }

    #[test]
    fn held_directory_and_file_handles_block_path_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let guarded = directory.path().join("guarded");
        fs::create_dir(&guarded).unwrap();
        let moved_directory = directory.path().join("moved");
        let directory_guard = DirectoryChainGuard::open_existing(&guarded).unwrap();
        assert!(fs::rename(&guarded, &moved_directory).is_err());
        drop(directory_guard);
        fs::rename(&guarded, &moved_directory).unwrap();

        let file_path = moved_directory.join("vault.json");
        fs::write(&file_path, b"encrypted").unwrap();
        let moved_file = moved_directory.join("moved-vault.json");
        let file = open_existing_file(&file_path).unwrap();
        assert!(fs::rename(&file_path, &moved_file).is_err());
        drop(file);
        fs::rename(&file_path, &moved_file).unwrap();
    }

    #[test]
    fn secure_file_helpers_reject_non_disk_paths_and_directories_as_files() {
        assert!(DirectoryChainGuard::open_existing(Path::new("relative")).is_err());
        assert!(DirectoryChainGuard::open_existing(Path::new(r"\\server\share")).is_err());
        assert!(
            validate_absolute_disk_path(Path::new(r"C:\wallet\activity.jsonl:stream")).is_err()
        );
        let directory = tempfile::tempdir().unwrap();
        assert!(open_existing_file(directory.path()).is_err());
    }
}
