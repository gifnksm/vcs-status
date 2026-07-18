use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use crate::{
    VcsStatusError,
    vcs::{self, VcsRepository},
};

/// A version control repository that can report status relevant to
/// `--allow-*` style checks.
#[derive(Debug)]
pub struct Repository {
    inner: Box<dyn VcsRepository>,
}

impl Repository {
    /// Discovers the repository containing `path`.
    ///
    /// This searches `path` and its parent directories for a repository
    /// supported by one of the enabled backends.
    ///
    /// Returns `Ok(Some(_))` if a supported repository worktree is found, or
    /// `Ok(None)` if no supported repository is found.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    ///
    /// - a backend fails while probing `path`
    /// - the discovered repository does not provide a worktree for file
    ///   status checks
    #[inline]
    pub fn discover<P>(path: P) -> Result<Option<Self>, VcsStatusError>
    where
        P: AsRef<Path>,
    {
        let Some(inner) = vcs::discover(path.as_ref())? else {
            return Ok(None);
        };
        Ok(Some(Self { inner }))
    }

    /// Opens the repository at `path`.
    ///
    /// Unlike [`Self::discover`], this does not search parent directories.
    /// `path` must identify a repository directly according to the enabled
    /// backend.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    ///
    /// - `path` does not refer to a supported repository worktree
    /// - the backend fails to open it
    #[inline]
    pub fn open<P>(path: P) -> Result<Self, VcsStatusError>
    where
        P: AsRef<Path>,
    {
        let inner = vcs::open(path.as_ref())?;
        Ok(Self { inner })
    }

    /// Returns the root directory of the repository worktree.
    #[inline]
    #[must_use]
    pub fn workdir(&self) -> &Path {
        self.inner.worktree()
    }

    /// Returns the aggregate status of the repository worktree.
    ///
    /// Paths in the returned status are relative to [`Self::workdir`].
    ///
    /// # Errors
    ///
    /// Returns an error if the backend fails to query the repository status.
    #[inline]
    pub fn status(&self) -> Result<RepositoryStatus, VcsStatusError> {
        self.inner.status()
    }

    /// Returns the status of a single file `path` within the repository.
    ///
    /// `path` must resolve to an existing file within [`Self::workdir`].
    /// Symlinks are followed, and the returned [`FileStatus`] describes the
    /// resolved file.
    ///
    /// A file may be both staged and modified at the same time if it has
    /// staged changes and additional unstaged changes.
    ///
    /// This operation is intended for file paths. It does not perform rename
    /// detection.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    ///
    /// - `path` does not resolve to a path within [`Self::workdir`]
    /// - `path` does not resolve to an existing file in the worktree
    /// - `path` could not be resolved to a canonical path for any other reason
    /// - the backend fails to query file status for any other reason
    #[inline]
    pub fn file_status<P>(&self, path: P) -> Result<FileStatus, VcsStatusError>
    where
        P: AsRef<Path>,
    {
        self.inner.file_status(path.as_ref())
    }
}

/// A summary of repository state relevant to `--allow-*` checks.
///
/// These path sets describe repository status entries and may include tracked
/// paths that are no longer present in the worktree.
#[derive(Debug, Clone)]
pub struct RepositoryStatus {
    pub(crate) modified: BTreeSet<PathBuf>,
    pub(crate) staged: BTreeSet<PathBuf>,
    pub(crate) untracked: BTreeSet<PathBuf>,
}

impl RepositoryStatus {
    /// Returns whether the repository has tracked worktree changes.
    ///
    /// This does not include staged changes or untracked files.
    #[inline]
    #[must_use]
    pub fn has_worktree_changes(&self) -> bool {
        !self.modified.is_empty()
    }

    /// Returns the set of tracked paths with worktree changes.
    ///
    /// The returned paths are relative to [`Repository::workdir`].
    #[inline]
    #[must_use]
    pub fn modified_files(&self) -> &BTreeSet<PathBuf> {
        &self.modified
    }

    /// Returns whether the repository has staged changes in the index.
    #[inline]
    #[must_use]
    pub fn has_staged_changes(&self) -> bool {
        !self.staged.is_empty()
    }

    /// Returns the set of tracked paths with staged changes.
    ///
    /// The returned paths are relative to [`Repository::workdir`].
    #[inline]
    #[must_use]
    pub fn staged_files(&self) -> &BTreeSet<PathBuf> {
        &self.staged
    }

    /// Returns whether the repository has untracked files.
    #[inline]
    #[must_use]
    pub fn has_untracked_files(&self) -> bool {
        !self.untracked.is_empty()
    }

    /// Returns the set of untracked paths.
    ///
    /// The returned paths are relative to [`Repository::workdir`].
    #[inline]
    #[must_use]
    pub fn untracked_files(&self) -> &BTreeSet<PathBuf> {
        &self.untracked
    }

    /// Returns whether the repository has any worktree, staged, or untracked
    /// changes.
    #[inline]
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.has_worktree_changes() || self.has_staged_changes() || self.has_untracked_files()
    }
}

/// The status of a single file within a repository.
///
/// The stored path is the canonical path of the resolved file, relative to
/// [`Repository::workdir`].
///
/// More than one predicate may return `true` for the same file. For example,
/// a file may have staged changes and additional unstaged modifications.
#[derive(Debug, Clone)]
pub struct FileStatus {
    pub(crate) path: PathBuf,
    pub(crate) modified: bool,
    pub(crate) staged: bool,
    pub(crate) untracked: bool,
}

impl FileStatus {
    /// Returns the canonical repository-relative path of the resolved file.
    ///
    /// This may differ from the path passed to [`Repository::file_status`]
    /// when that path reaches the same file through symlinks or other
    /// equivalent non-canonical forms.
    #[inline]
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns whether the file has tracked worktree changes.
    ///
    /// This does not include staged changes or untracked files.
    #[inline]
    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// Returns whether the file has staged changes in the index.
    #[inline]
    #[must_use]
    pub fn is_staged(&self) -> bool {
        self.staged
    }

    /// Returns whether the file is untracked.
    #[inline]
    #[must_use]
    pub fn is_untracked(&self) -> bool {
        self.untracked
    }

    /// Returns whether the file is modified, staged, or untracked.
    #[inline]
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.is_modified() || self.is_staged() || self.is_untracked()
    }
}
