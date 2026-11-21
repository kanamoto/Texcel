use super::*;

/// Writes a temporary file and publishes it only after the content is complete.
pub(super) fn atomic_save(
    path: &Path,
    bytes: &[u8],
    expected: &SaveExpectation,
    job_id: u64,
    cancelled: &AtomicBool,
) -> io::Result<FileIdentity> {
    let expected = resolve_expectation(path, expected, cancelled)?;
    verify_expectation(path, &expected, cancelled)?;
    let inherited_mode = existing_file_mode(path, &expected)?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temp_path = create_temp_file(parent, job_id, bytes, cancelled, inherited_mode)?;
    let replacement = (|| {
        // A failed reread after replacement would report a completed replacement as failed.
        // Build the file identity from the same temporary file first, then return it after replacement.
        let identity = FileIdentity::read_cancellable(&temp_path, Some(cancelled))?;
        if cancelled.load(Ordering::Acquire) {
            return Err(cancelled_error());
        }
        verify_expectation(path, &expected, cancelled)?;
        check_not_cancelled_before_publish(cancelled)?;
        match expected {
            SaveExpectation::Absent => {
                // `hard_link` publishes the completed temporary file without replacing an existing destination.
                fs::hard_link(&temp_path, path)?;
                let _ = fs::remove_file(&temp_path);
            }
            SaveExpectation::Matches(_) => replace_file_atomic(&temp_path, path)?,
            SaveExpectation::KnownPath { .. } => {
                return Err(user_io_error(
                    io::ErrorKind::InvalidInput,
                    Message::new("status-save-destination-unverified"),
                ));
            }
            SaveExpectation::CurrentAtStart => {
                return Err(user_io_error(
                    io::ErrorKind::InvalidInput,
                    Message::new("status-save-destination-unverified"),
                ));
            }
        }
        Ok(identity)
    })();
    if temp_path.exists() {
        let _ = fs::remove_file(&temp_path);
    }
    replacement
}

/// Keeps cancellation effective until the final publish step begins.
pub(super) fn check_not_cancelled_before_publish(cancelled: &AtomicBool) -> io::Result<()> {
    if cancelled.load(Ordering::Acquire) {
        Err(cancelled_error())
    } else {
        Ok(())
    }
}

/// Chooses the destination identity rule for this save request.
pub(super) fn resolve_expectation(
    path: &Path,
    expected: &SaveExpectation,
    cancelled: &AtomicBool,
) -> io::Result<SaveExpectation> {
    match expected {
        SaveExpectation::Absent | SaveExpectation::Matches(_) => return Ok(expected.clone()),
        SaveExpectation::KnownPath {
            source_path,
            identity,
        } => {
            if same_normalized_path(path, source_path) {
                return match fs::symlink_metadata(path) {
                    Ok(metadata) if metadata.file_type().is_symlink() => Err(user_io_error(
                        io::ErrorKind::InvalidInput,
                        Message::new("status-file-symlink-unsupported"),
                    )),
                    Ok(_) => Ok(SaveExpectation::Matches(identity.clone())),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        Ok(SaveExpectation::Absent)
                    }
                    Err(error) => Err(error),
                };
            }
            return resolve_expectation(path, &SaveExpectation::CurrentAtStart, cancelled);
        }
        SaveExpectation::CurrentAtStart => (),
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(user_io_error(
            io::ErrorKind::InvalidInput,
            Message::new("status-file-symlink-unsupported"),
        )),
        Ok(_) => Ok(SaveExpectation::Matches(FileIdentity::read_cancellable(
            path,
            Some(cancelled),
        )?)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(SaveExpectation::Absent),
        Err(error) => Err(error),
    }
}

/// Checks whether two paths name the same normalized destination.
pub(super) fn same_normalized_path(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => lexical_absolute_path(left)
            .zip(lexical_absolute_path(right))
            .is_some_and(|(left, right)| left == right),
    }
}

/// Makes a path absolute and removes safe lexical dot segments.
pub(super) fn lexical_absolute_path(path: &Path) -> Option<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::CurDir => (),
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    Some(normalized)
}

#[cfg(unix)]
/// Reads the permission bits of an existing destination on Unix.
pub(super) fn existing_file_mode(
    path: &Path,
    expected: &SaveExpectation,
) -> io::Result<Option<u32>> {
    use std::os::unix::fs::PermissionsExt;

    if matches!(expected, SaveExpectation::Matches(_)) {
        let metadata = fs::metadata(path)?;
        return Ok(Some(metadata.permissions().mode() & 0o7777));
    }
    Ok(None)
}

#[cfg(not(unix))]
/// Reads the permission bits of an existing destination on Unix.
pub(super) fn existing_file_mode(
    _path: &Path,
    _expected: &SaveExpectation,
) -> io::Result<Option<u32>> {
    Ok(None)
}

/// Checks the destination state immediately before replacement.
pub(super) fn verify_expectation(
    path: &Path,
    expected: &SaveExpectation,
    cancelled: &AtomicBool,
) -> io::Result<()> {
    match expected {
        SaveExpectation::Absent => match fs::symlink_metadata(path) {
            Ok(_) => Err(user_io_error(
                io::ErrorKind::AlreadyExists,
                Message::new("status-save-destination-appeared"),
            )),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        },
        SaveExpectation::Matches(expected) => {
            let current = FileIdentity::read_cancellable(path, Some(cancelled))?;
            if &current == expected {
                Ok(())
            } else {
                Err(user_io_error(
                    io::ErrorKind::Other,
                    Message::new("status-save-destination-changed"),
                ))
            }
        }
        SaveExpectation::CurrentAtStart => Err(user_io_error(
            io::ErrorKind::InvalidInput,
            Message::new("status-save-destination-unverified"),
        )),
        SaveExpectation::KnownPath { .. } => Err(user_io_error(
            io::ErrorKind::InvalidInput,
            Message::new("status-save-destination-unverified"),
        )),
    }
}

/// Creates a temporary file beside the destination.
pub(super) fn create_temp_file(
    parent: &Path,
    job_id: u64,
    bytes: &[u8],
    cancelled: &AtomicBool,
    inherited_mode: Option<u32>,
) -> io::Result<PathBuf> {
    // Only Unix has permission bits to carry over to the new file.
    #[cfg(not(unix))]
    let _ = inherited_mode;
    let mut last_error = None;
    for attempt in 0..16_u32 {
        let mut name = OsString::from(".texcel-");
        name.push(std::process::id().to_string());
        name.push("-");
        name.push(job_id.to_string());
        name.push("-");
        name.push(attempt.to_string());
        name.push(".tmp");
        let path = parent.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                let write_result = (|| {
                    #[cfg(unix)]
                    if let Some(mode) = inherited_mode {
                        use std::os::unix::fs::PermissionsExt;
                        file.set_permissions(fs::Permissions::from_mode(mode))?;
                    }
                    write_snapshot(&mut file, bytes, cancelled)?;
                    file.flush()?;
                    file.sync_all()
                })();
                if let Err(error) = write_result {
                    let _ = fs::remove_file(&path);
                    return Err(error);
                }
                return Ok(path);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                last_error = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        user_io_error(
            io::ErrorKind::AlreadyExists,
            Message::new("status-save-temp-name-unavailable"),
        )
    }))
}

/// Writes and syncs all bytes before returning the temporary file.
pub(super) fn write_snapshot(
    file: &mut File,
    bytes: &[u8],
    cancelled: &AtomicBool,
) -> io::Result<()> {
    for chunk in bytes.chunks(IO_CHUNK_SIZE) {
        if cancelled.load(Ordering::Acquire) {
            return Err(cancelled_error());
        }
        file.write_all(chunk)?;
    }
    if cancelled.load(Ordering::Acquire) {
        return Err(cancelled_error());
    }
    Ok(())
}

/// Builds the standard cancellation error.
pub(super) fn cancelled_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::Interrupted,
        Message::new("status-save-cancelled"),
    )
}

/// Builds the error used when a source exceeds the size limit.
pub(super) fn source_size_error() -> io::Error {
    user_io_error(
        io::ErrorKind::InvalidData,
        Message::new("status-file-size-limit"),
    )
}

/// Publishes the completed temporary file using an atomic replacement.
pub(crate) fn replace_file_atomic(source: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        replace_file_windows(source, destination)
    }
    #[cfg(not(windows))]
    {
        fs::rename(source, destination)
    }
}

#[cfg(windows)]
/// Replaces a Windows destination with the completed temporary file.
pub(super) fn replace_file_windows(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        /// Calls the Windows API to replace or move a file.
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }

    let mut source_wide = source.as_os_str().encode_wide().collect::<Vec<_>>();
    let mut destination_wide = destination.as_os_str().encode_wide().collect::<Vec<_>>();
    if source_wide.contains(&0) || destination_wide.contains(&0) {
        return Err(user_io_error(
            io::ErrorKind::InvalidInput,
            Message::new("status-file-path-invalid"),
        ));
    }
    source_wide.push(0);
    destination_wide.push(0);
    // SAFETY: Both UTF-16 paths end in NUL and stay unchanged during this call.
    let succeeded = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if succeeded == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Returns the modification time as Unix nanoseconds.
pub(super) fn modified_unix_nanos(metadata: &fs::Metadata) -> Option<u128> {
    metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_nanos())
}
