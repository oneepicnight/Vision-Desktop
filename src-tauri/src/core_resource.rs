#![cfg(windows)]

use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom},
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::{AsRawHandle, RawHandle},
    },
    path::{Component, Path, PathBuf, Prefix},
};
use windows_sys::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::{
        FileIdInfo, GetDriveTypeW, GetFileInformationByHandle, GetFileInformationByHandleEx,
        BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO, FILE_SHARE_READ,
        FILE_SHARE_WRITE,
    },
    System::{Threading::QueryFullProcessImageNameW, WindowsProgramming::DRIVE_FIXED},
};

const MAX_PROCESS_IMAGE_PATH_UNITS: usize = 32_768;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct CoreFileIdentity {
    volume_serial_number: u64,
    file_id: [u8; 16],
    size_bytes: u64,
}

impl CoreFileIdentity {
    pub(crate) fn stable_bytes(self) -> [u8; 32] {
        let mut bytes = [0_u8; 32];
        bytes[..8].copy_from_slice(&self.volume_serial_number.to_le_bytes());
        bytes[8..24].copy_from_slice(&self.file_id);
        bytes[24..].copy_from_slice(&self.size_bytes.to_le_bytes());
        bytes
    }
}

pub(crate) struct GuardedCoreFile {
    _directories: CoreDirectoryChainGuard,
    file: File,
    path: PathBuf,
    identity: CoreFileIdentity,
}

struct CoreDirectoryChainGuard {
    _directories: Vec<File>,
}

impl CoreDirectoryChainGuard {
    fn open_existing(path: &Path) -> io::Result<Self> {
        validate_absolute_fixed_disk_path(path)?;
        let mut current = PathBuf::new();
        let mut directories = Vec::new();
        for component in path.components() {
            current.push(component.as_os_str());
            if matches!(component, Component::Prefix(_)) {
                continue;
            }
            directories.push(open_directory(&current)?);
        }
        if directories.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Core resource directory chain is empty",
            ));
        }
        Ok(Self {
            _directories: directories,
        })
    }
}

impl GuardedCoreFile {
    pub(crate) fn open(path: &Path) -> io::Result<Self> {
        let parent = path.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Core resource path has no parent",
            )
        })?;
        let directories = CoreDirectoryChainGuard::open_existing(parent)?;
        let mut options = OpenOptions::new();
        options
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        let file = options.open(path)?;
        validate_regular_single_link_file(&file)?;
        let identity = file_identity(&file)?;
        Ok(Self {
            _directories: directories,
            file,
            path: path.to_path_buf(),
            identity,
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn identity(&self) -> CoreFileIdentity {
        self.identity
    }

    pub(crate) fn size_bytes(&self) -> u64 {
        self.identity.size_bytes
    }

    pub(crate) fn read_bounded(&mut self, maximum: usize) -> io::Result<Vec<u8>> {
        self.file.seek(SeekFrom::Start(0))?;
        let limit = u64::try_from(maximum)
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid read bound"))?;
        let mut bytes = Vec::new();
        self.file.by_ref().take(limit).read_to_end(&mut bytes)?;
        self.file.seek(SeekFrom::Start(0))?;
        if bytes.len() > maximum {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Core resource exceeds its size bound",
            ));
        }
        Ok(bytes)
    }

    pub(crate) fn sha256_lower(&mut self) -> io::Result<String> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut hasher = Sha256::new();
        io::copy(&mut self.file, &mut hasher)?;
        self.file.seek(SeekFrom::Start(0))?;
        Ok(hex::encode(hasher.finalize()))
    }

    pub(crate) fn revalidate(
        &mut self,
        expected_size: u64,
        expected_sha256: &str,
    ) -> io::Result<()> {
        validate_regular_single_link_file(&self.file)?;
        let current = file_identity(&self.file)?;
        if current != self.identity || current.size_bytes != expected_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Core resource identity changed",
            ));
        }
        if self.sha256_lower()? != expected_sha256 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Core resource digest changed",
            ));
        }
        Ok(())
    }
}

pub(crate) fn running_process_image_identity(raw: RawHandle) -> io::Result<CoreFileIdentity> {
    let mut capacity = 512_usize;
    loop {
        if capacity > MAX_PROCESS_IMAGE_PATH_UNITS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Core process image path is too long",
            ));
        }
        let mut buffer = vec![0_u16; capacity];
        let mut length = u32::try_from(buffer.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid path buffer"))?;
        let succeeded = unsafe {
            QueryFullProcessImageNameW(raw as HANDLE, 0, buffer.as_mut_ptr(), &mut length)
        };
        if succeeded != 0 {
            buffer.truncate(length as usize);
            let path = PathBuf::from(String::from_utf16(&buffer).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid process image path")
            })?);
            let image = GuardedCoreFile::open(&path)?;
            return Ok(image.identity());
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(122) {
            capacity = capacity.saturating_mul(2);
            continue;
        }
        return Err(error);
    }
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
            "Core resource directory is not a regular directory",
        ));
    }
    Ok(file)
}

fn validate_regular_single_link_file(file: &File) -> io::Result<()> {
    let attributes = file.metadata()?.file_attributes();
    if attributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Core resource is not a regular file",
        ));
    }
    let information = by_handle_information(file)?;
    if information.nNumberOfLinks != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Core resource must have exactly one hard link",
        ));
    }
    Ok(())
}

fn by_handle_information(file: &File) -> io::Result<BY_HANDLE_FILE_INFORMATION> {
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    let succeeded =
        unsafe { GetFileInformationByHandle(file.as_raw_handle() as HANDLE, &mut information) };
    if succeeded == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(information)
    }
}

fn file_identity(file: &File) -> io::Result<CoreFileIdentity> {
    let mut information = FILE_ID_INFO::default();
    let size = u32::try_from(std::mem::size_of::<FILE_ID_INFO>())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid identity size"))?;
    let succeeded = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle() as HANDLE,
            FileIdInfo,
            (&mut information as *mut FILE_ID_INFO).cast(),
            size,
        )
    };
    if succeeded == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(CoreFileIdentity {
        volume_serial_number: information.VolumeSerialNumber,
        file_id: information.FileId.Identifier,
        size_bytes: file.metadata()?.len(),
    })
}

fn validate_absolute_fixed_disk_path(path: &Path) -> io::Result<()> {
    let mut components = path.components();
    let drive = match components.next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(letter) => letter,
            _ => return Err(invalid_core_path()),
        },
        _ => return Err(invalid_core_path()),
    };
    if !matches!(components.next(), Some(Component::RootDir))
        || components.any(|component| match component {
            Component::Normal(value) => value.encode_wide().any(|unit| unit == b':' as u16),
            _ => true,
        })
    {
        return Err(invalid_core_path());
    }
    let root = [u16::from(drive), b':' as u16, b'\\' as u16, 0];
    if unsafe { GetDriveTypeW(root.as_ptr()) } != DRIVE_FIXED {
        return Err(invalid_core_path());
    }
    Ok(())
}

fn invalid_core_path() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "Core resource path is not on a fixed local disk",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, process::Command, thread};

    #[test]
    fn guarded_file_rejects_hard_links_and_blocks_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("vision-core.exe");
        let alias = directory.path().join("vision-core-alias.exe");
        File::create(&original)
            .unwrap()
            .write_all(b"candidate")
            .unwrap();
        std::fs::hard_link(&original, &alias).unwrap();
        assert!(GuardedCoreFile::open(&original).is_err());
        std::fs::remove_file(&alias).unwrap();

        let mut guarded = GuardedCoreFile::open(&original).unwrap();
        let digest = guarded.sha256_lower().unwrap();
        assert!(std::fs::rename(&original, directory.path().join("moved.exe")).is_err());
        guarded.revalidate(9, &digest).unwrap();
    }

    #[test]
    fn guarded_file_rejects_relative_and_unc_paths() {
        assert!(GuardedCoreFile::open(Path::new("relative.exe")).is_err());
        assert!(GuardedCoreFile::open(Path::new(r"\\server\share\vision-core.exe")).is_err());
    }

    #[test]
    fn guarded_file_rejects_reparse_ancestors_and_preexisting_writers() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("target");
        let junction = directory.path().join("junction");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("manifest.json"), b"manifest").unwrap();
        let status = Command::new("cmd.exe")
            .args([
                "/d",
                "/s",
                "/c",
                "mklink",
                "/J",
                junction.to_str().unwrap(),
                target.to_str().unwrap(),
            ])
            .status()
            .unwrap();
        assert!(status.success());
        assert!(GuardedCoreFile::open(&junction.join("manifest.json")).is_err());

        let regular = directory.path().join("vision-core.exe");
        std::fs::write(&regular, b"candidate").unwrap();
        let writer = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(&regular)
            .unwrap();
        assert!(GuardedCoreFile::open(&regular).is_err());
        drop(writer);
        GuardedCoreFile::open(&regular).unwrap();
    }

    #[test]
    fn path_swaps_and_post_hash_replacement_never_preserve_an_approved_digest() {
        let directory = tempfile::tempdir().unwrap();
        let resource = directory.path().join("manifest.json");
        let displaced = directory.path().join("manifest.original.json");
        std::fs::write(&resource, b"approved").unwrap();
        let approved_digest = hex::encode(Sha256::digest(b"approved"));

        std::fs::rename(&resource, &displaced).unwrap();
        std::fs::write(&resource, b"replaced").unwrap();
        let mut swapped = GuardedCoreFile::open(&resource).unwrap();
        assert!(swapped.revalidate(8, &approved_digest).is_err());
        drop(swapped);

        std::fs::remove_file(&resource).unwrap();
        std::fs::rename(&displaced, &resource).unwrap();
        let mut guarded = GuardedCoreFile::open(&resource).unwrap();
        assert_eq!(guarded.sha256_lower().unwrap(), approved_digest);

        let rename_from = resource.clone();
        let rename_to = directory.path().join("raced.json");
        let rename = thread::spawn(move || std::fs::rename(rename_from, rename_to));
        let delete_path = resource.clone();
        let delete = thread::spawn(move || std::fs::remove_file(delete_path));
        assert!(rename.join().unwrap().is_err());
        assert!(delete.join().unwrap().is_err());
        assert!(OpenOptions::new().write(true).open(&resource).is_err());
        guarded.revalidate(8, &approved_digest).unwrap();
    }

    #[test]
    fn revalidation_fails_for_changed_size_digest_and_unavailable_process_image() {
        let directory = tempfile::tempdir().unwrap();
        let resource = directory.path().join("manifest.json");
        std::fs::write(&resource, b"approved").unwrap();
        let mut guarded = GuardedCoreFile::open(&resource).unwrap();
        let digest = guarded.sha256_lower().unwrap();
        assert!(guarded.revalidate(7, &digest).is_err());
        assert!(guarded.revalidate(8, &"0".repeat(64)).is_err());
        assert!(running_process_image_identity(std::ptr::null_mut()).is_err());
    }
}
