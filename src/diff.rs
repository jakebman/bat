#![cfg(feature = "git")]

use std::collections::HashMap;
use std::fs;
use std::env;
use std::path::Path;

use git2::{DiffOptions, IntoCString, Repository, Error};

#[derive(Copy, Clone, Debug)]
pub enum LineChange {
    Added,
    RemovedAbove,
    RemovedBelow,
    Modified,
}

pub type LineChanges = HashMap<u32, LineChange>;

fn discover_git_repo(filename: &Path) -> Result<Repository, Error> {
    if env::var("GIT_DIR").is_ok() {
      return Repository::open_from_env();
    }
    return Repository::discover(filename);
}

pub fn get_git_diff(filename: &Path) -> Option<LineChanges> {
    let env_work_tree= env::var("GIT_WORK_TREE"); // bad name
    env::remove_var("GIT_WORK_TREE");

    let jake_result = discover_git_repo(filename);
    let repo = jake_result.ok()?;
    let repo_work_tree = repo.workdir();
    if repo_work_tree.is_some() {
        return continue_with(&repo, filename, repo_work_tree?);
    } else {
        return continue_with(&repo, filename, env_work_tree.as_ref().map(|x| Path::new(x)).ok()?);
    };
}
fn continue_with(repo:&Repository, filename: &Path, work_tree: &Path) -> Option<LineChanges> {
    let repo_path_absolute = fs::canonicalize(work_tree).ok()?;

    let filepath_absolute = fs::canonicalize(filename).ok()?;
    let filepath_relative_to_repo = filepath_absolute.strip_prefix(&repo_path_absolute).ok()?;

    let mut diff_options = DiffOptions::new();
    let pathspec = filepath_relative_to_repo.into_c_string().ok()?;
    diff_options.pathspec(pathspec);
    diff_options.context_lines(0);

    let index_to_work_dir = repo.diff_index_to_workdir(None, Some(&mut diff_options));
    let diff = index_to_work_dir
        .ok()?;

    let mut line_changes: LineChanges = HashMap::new();

    let mark_section =
        |line_changes: &mut LineChanges, start: u32, end: i32, change: LineChange| {
            for line in start..=end as u32 {
                line_changes.insert(line, change);
            }
        };

    let _ = diff.foreach(
        &mut |_, _| true,
        None,
        Some(&mut |delta, hunk| {
            let path = delta.new_file().path().unwrap_or_else(|| Path::new(""));

            if filepath_relative_to_repo != path {
                return false;
            }

            let old_lines = hunk.old_lines();
            let new_start = hunk.new_start();
            let new_lines = hunk.new_lines();
            let new_end = (new_start + new_lines) as i32 - 1;

            if old_lines == 0 && new_lines > 0 {
                mark_section(&mut line_changes, new_start, new_end, LineChange::Added);
            } else if new_lines == 0 && old_lines > 0 {
                if new_start == 0 {
                    mark_section(&mut line_changes, 1, 1, LineChange::RemovedAbove);
                } else {
                    mark_section(
                        &mut line_changes,
                        new_start,
                        new_start as i32,
                        LineChange::RemovedBelow,
                    );
                }
            } else {
                mark_section(&mut line_changes, new_start, new_end, LineChange::Modified);
            }

            true
        }),
        None,
    );

    Some(line_changes)
}
