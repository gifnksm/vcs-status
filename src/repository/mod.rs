//! Repository status query APIs.
//!
//! This module provides the [`Repository`] type for discovering and querying
//! VCS repositories, along with status result types such as
//! [`RepositoryStatus`] and [`FileStatus`].
//!
//! Paths returned by status queries are relative to the repository worktree.
//!
//! Most users will start with [`Repository::discover`] or [`Repository::open`],
//! then query status with [`Repository::repository_status`] or
//! [`Repository::file_status`].

use std::{
    path::{Path, PathBuf},
    slice,
};

use crate::{
    VcsStatusError,
    vcs::{self, VcsRepository},
};

#[cfg(test)]
mod tests;

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
    /// The returned status may include files ignored by the VCS. These are
    /// represented as clean [`FileStatus`] values; see [`FileStatus`] for
    /// details.
    ///
    /// Paths in the returned status are relative to [`Self::workdir`].
    ///
    /// # Errors
    ///
    /// Returns an error if the backend fails to query the repository status.
    #[inline]
    pub fn repository_status(&self) -> Result<RepositoryStatus, VcsStatusError> {
        self.inner.repository_status()
    }

    /// Returns the status of a single file `path` within the repository.
    ///
    /// `path` must resolve to an existing file within [`Self::workdir`].
    /// Symlinks are followed, and the returned [`FileStatus`] describes the
    /// resolved file.
    ///
    /// The returned [`FileStatus::path`] may differ from the path passed to
    /// this method when the input reaches the same file through symlinks or
    /// other equivalent non-canonical forms.
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

/// A set of file statuses produced by a repository status query.
///
/// File statuses are ordered by ascending worktree-relative path. Entries may
/// include files ignored by the VCS and tracked paths that are no longer
/// present in the worktree.
///
/// Ignored files are represented as clean [`FileStatus`] values because
/// `--allow-*` style checks do not treat them as a blocking state.
#[derive(Debug, Clone)]
pub struct RepositoryStatus {
    files: Vec<FileStatus>,
    num_modified_files: usize,
    num_staged_files: usize,
    num_untracked_files: usize,
}

impl RepositoryStatus {
    #[cfg(any(test, vcs_backend_enabled))]
    pub(crate) fn new<I>(files: I) -> Self
    where
        I: IntoIterator<Item = FileStatus>,
    {
        let mut files = files.into_iter().collect::<Vec<_>>();
        files.sort_by(|a, b| a.path().cmp(b.path()));
        assert!(
            files.array_windows().all(|[a, b]| a.path() != b.path()),
            "repository status entries must be unique by path",
        );

        let mut num_modified_files = 0;
        let mut num_staged_files = 0;
        let mut num_untracked_files = 0;

        for file in &files {
            num_modified_files += usize::from(file.is_modified());
            num_staged_files += usize::from(file.is_staged());
            num_untracked_files += usize::from(file.is_untracked());
        }

        Self {
            files,
            num_modified_files,
            num_staged_files,
            num_untracked_files,
        }
    }

    /// Returns an iterator over all file statuses in this status set.
    ///
    /// This includes files ignored by the VCS, which are represented as clean
    /// file statuses.
    ///
    /// Files are yielded in ascending worktree-relative path order.
    #[inline]
    #[must_use]
    pub fn files(&self) -> Files<'_> {
        Files {
            iter: self.files.iter(),
        }
    }

    /// Returns an iterator over files with unstaged changes in this status
    /// set.
    ///
    /// Files are yielded in ascending worktree-relative path order.
    #[inline]
    #[must_use]
    pub fn modified_files(&self) -> ModifiedFiles<'_> {
        ModifiedFiles {
            iter: self.files.iter(),
            len: self.num_modified_files,
        }
    }

    /// Returns an iterator over files with staged changes in this status set.
    ///
    /// Files are yielded in ascending worktree-relative path order.
    #[inline]
    #[must_use]
    pub fn staged_files(&self) -> StagedFiles<'_> {
        StagedFiles {
            iter: self.files.iter(),
            len: self.num_staged_files,
        }
    }

    /// Returns an iterator over untracked files in this status set.
    ///
    /// Files are yielded in ascending worktree-relative path order.
    #[inline]
    #[must_use]
    pub fn untracked_files(&self) -> UntrackedFiles<'_> {
        UntrackedFiles {
            iter: self.files.iter(),
            len: self.num_untracked_files,
        }
    }

    /// Returns whether this status set contains any files with unstaged
    /// changes.
    ///
    /// This does not include staged changes or untracked files.
    #[inline]
    #[must_use]
    pub fn has_modified_files(&self) -> bool {
        self.num_modified_files > 0
    }

    /// Returns whether this status set contains any files with staged changes.
    #[inline]
    #[must_use]
    pub fn has_staged_files(&self) -> bool {
        self.num_staged_files > 0
    }

    /// Returns whether this status set contains any untracked files.
    #[inline]
    #[must_use]
    pub fn has_untracked_files(&self) -> bool {
        self.num_untracked_files > 0
    }

    /// Returns whether this status set contains any modified, staged, or
    /// untracked files.
    ///
    /// Clean file statuses, including ignored files, do not make the status
    /// set dirty.
    #[inline]
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.has_modified_files() || self.has_staged_files() || self.has_untracked_files()
    }
}

/// The status of a single file within a repository.
///
/// The stored path is the worktree-relative path associated with this status
/// in the VCS. It may refer to a tracked path that is no longer present in the
/// worktree.
///
/// Clean statuses are those for which [`Self::is_modified`],
/// [`Self::is_staged`], and [`Self::is_untracked`] all return `false`.
/// Files ignored by the VCS are represented as clean statuses because
/// `--allow-*` style checks do not treat them as a blocking state.
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
    /// Returns the worktree-relative path associated with this file status in
    /// the VCS.
    ///
    /// When this status is returned by [`Repository::file_status`], the
    /// returned path may differ from the path passed to that method when the
    /// input reaches the same file through symlinks or other equivalent
    /// non-canonical forms.
    #[inline]
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns whether the file has unstaged changes.
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
    ///
    /// This returns `false` for clean files, including files ignored by the
    /// VCS.
    #[inline]
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.is_modified() || self.is_staged() || self.is_untracked()
    }
}

/// An iterator over all file statuses in a [`RepositoryStatus`].
///
/// This struct is created by the [`RepositoryStatus::files`] method.
/// Files are yielded in ascending worktree-relative path order.
#[derive(Debug, Clone)]
pub struct Files<'a> {
    iter: slice::Iter<'a, FileStatus>,
}

impl<'a> Iterator for Files<'a> {
    type Item = &'a FileStatus;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl DoubleEndedIterator for Files<'_> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back()
    }
}

impl ExactSizeIterator for Files<'_> {
    #[inline]
    fn len(&self) -> usize {
        self.iter.len()
    }
}

/// An iterator over files with unstaged changes in a [`RepositoryStatus`].
///
/// This struct is created by the [`RepositoryStatus::modified_files`] method.
/// Files are yielded in ascending worktree-relative path order.
#[derive(Debug, Clone)]
pub struct ModifiedFiles<'a> {
    iter: slice::Iter<'a, FileStatus>,
    len: usize,
}

impl<'a> Iterator for ModifiedFiles<'a> {
    type Item = &'a FileStatus;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let file = self.iter.find(|file| file.is_modified())?;
        self.len -= 1;
        Some(file)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl DoubleEndedIterator for ModifiedFiles<'_> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        let file = self.iter.rfind(|file| file.is_modified())?;
        self.len -= 1;
        Some(file)
    }
}

impl ExactSizeIterator for ModifiedFiles<'_> {
    #[inline]
    fn len(&self) -> usize {
        self.len
    }
}

/// An iterator over files with staged changes in a [`RepositoryStatus`].
///
/// This struct is created by the [`RepositoryStatus::staged_files`] method.
/// Files are yielded in ascending worktree-relative path order.
#[derive(Debug, Clone)]
pub struct StagedFiles<'a> {
    iter: slice::Iter<'a, FileStatus>,
    len: usize,
}

impl<'a> Iterator for StagedFiles<'a> {
    type Item = &'a FileStatus;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let file = self.iter.find(|file| file.is_staged())?;
        self.len -= 1;
        Some(file)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl DoubleEndedIterator for StagedFiles<'_> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        let file = self.iter.rfind(|file| file.is_staged())?;
        self.len -= 1;
        Some(file)
    }
}

impl ExactSizeIterator for StagedFiles<'_> {
    #[inline]
    fn len(&self) -> usize {
        self.len
    }
}

/// An iterator over untracked files in a [`RepositoryStatus`].
///
/// This struct is created by the [`RepositoryStatus::untracked_files`] method.
/// Files are yielded in ascending worktree-relative path order.
#[derive(Debug, Clone)]
pub struct UntrackedFiles<'a> {
    iter: slice::Iter<'a, FileStatus>,
    len: usize,
}

impl<'a> Iterator for UntrackedFiles<'a> {
    type Item = &'a FileStatus;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let file = self.iter.find(|file| file.is_untracked())?;
        self.len -= 1;
        Some(file)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl DoubleEndedIterator for UntrackedFiles<'_> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        let file = self.iter.rfind(|file| file.is_untracked())?;
        self.len -= 1;
        Some(file)
    }
}

impl ExactSizeIterator for UntrackedFiles<'_> {
    #[inline]
    fn len(&self) -> usize {
        self.len
    }
}
