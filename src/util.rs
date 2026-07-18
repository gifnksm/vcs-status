#![allow(
    dead_code,
    reason = "shared utility helpers may be unused in some backend feature sets, and handling that with finer-grained cfgs would add unnecessary complexity"
)]

use std::{
    fs::Metadata,
    io,
    path::{Path, PathBuf},
};

use snafu::{IntoError as _, OptionExt as _, ensure};

use crate::{VcsStatusError, error};

pub(crate) fn read_path_metadata(path: &Path) -> Result<Metadata, VcsStatusError> {
    path.metadata().map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            error::PathNotFoundSnafu { path }.build()
        } else {
            error::InaccessiblePathSnafu { path }.into_error(source)
        }
    })
}

pub(crate) fn ensure_path_exists(path: &Path) -> Result<(), VcsStatusError> {
    let _metadata = read_path_metadata(path)?;
    Ok(())
}

pub(crate) fn ensure_path_is_directory(path: &Path) -> Result<(), VcsStatusError> {
    let metadata = read_path_metadata(path)?;
    ensure!(metadata.is_dir(), error::PathNotADirectorySnafu { path });
    Ok(())
}

pub(crate) fn ensure_path_is_file(path: &Path) -> Result<(), VcsStatusError> {
    let metadata = read_path_metadata(path)?;
    ensure!(metadata.is_file(), error::PathNotAFileSnafu { path });
    Ok(())
}

fn canonicalize_path<P>(path: P) -> Result<PathBuf, VcsStatusError>
where
    P: AsRef<Path>,
{
    let path = path.as_ref();
    dunce::canonicalize(path).map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            error::PathNotFoundSnafu { path }.build()
        } else {
            error::CanonicalizePathSnafu { path }.into_error(source)
        }
    })
}

pub(crate) fn canonicalize_to_worktree_path(
    worktree_path: &Path,
    path: &Path,
) -> Result<PathBuf, VcsStatusError> {
    let worktree_path = canonicalize_path(worktree_path)?;
    let entry_path = canonicalize_path(worktree_path.join(path))?;
    let canonicalized = entry_path
        .strip_prefix(&worktree_path)
        .ok()
        .context(error::InvalidWorktreeRelativePathSnafu { path })?;
    ensure!(
        !canonicalized.as_os_str().is_empty(),
        error::InvalidWorktreeRelativePathSnafu { path }
    );
    Ok(canonicalized.to_path_buf())
}
