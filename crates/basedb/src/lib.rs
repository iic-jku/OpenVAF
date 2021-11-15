#[cfg(test)]
mod tests;

pub mod diagnostics;
pub mod line_index;
pub mod lints;

use std::str::from_utf8;
use std::{intrinsics::transmute, sync::Arc};

use line_index::{Line, LineIndex};
use lints::{Lint, LintData, LintLevel, LintRegistry, LintResolver, ErasedItemTreeId};
use parking_lot::RwLock;
use salsa::Durability;
use syntax::{
    sourcemap::SourceMap, FileReadError, Parse, Preprocess, SourceFile, SourceProvider, TextRange,
    TextSize,
};

use typed_index_collections::{TiSlice, TiVec};
pub use vfs::{FileId, Vfs, VfsPath};

pub trait VfsStorage {
    fn vfs(&self) -> &RwLock<Vfs>;
}

#[salsa::query_group(BaseDatabase)]
pub trait BaseDB: LintResolver + VfsStorage + salsa::Database {
    #[salsa::input]
    fn plugin_lints(&self) -> &'static [LintData];
    #[salsa::input]
    fn global_lint_overwrites(&self, root_file: FileId) -> Arc<TiSlice<Lint, Option<LintLevel>>>;

    #[salsa::input]
    fn include_dirs(&self, root_file: FileId) -> Arc<[VfsPath]>;
    #[salsa::input]
    fn macro_flags(&self, file_root: FileId) -> Arc<[Arc<str>]>;

    fn parse(&self, root_file: FileId) -> Parse<SourceFile>;
    fn preprocess(&self, root_file: FileId) -> Preprocess;
    #[salsa::transparent]
    fn sourcemap(&self, root_file: FileId) -> Arc<SourceMap>;

    /// Returns the line index of a file
    fn line_index(&self, file_id: FileId) -> Arc<LineIndex>;

    // #[salsa::transparent]
    // fn line_col(&self, span: FileSpan) -> LineCol;

    #[salsa::transparent]
    fn line(&self, pos: TextSize, file: FileId) -> Line;

    #[salsa::transparent]
    fn line_range(&self, line: Line, file: FileId) -> TextRange;

    fn file_text(&self, file: FileId) -> Result<Arc<str>, FileReadError>;

    #[salsa::transparent]
    fn file_path(&self, file: FileId) -> VfsPath;

    #[salsa::transparent]
    fn file_id(&self, path: VfsPath) -> FileId;

    #[salsa::invoke(LintRegistry::new)]
    fn lint_registry(&self) -> Arc<LintRegistry>;

    #[salsa::transparent]
    fn lint(&self, name: &str) -> Option<Lint>;

    #[salsa::transparent]
    fn lint_data(&self, lint: Lint) -> LintData;



    #[salsa::transparent]
    fn lint_lvl(&self, lint: Lint, root_file: FileId, sctx: Option<ErasedItemTreeId>)
        -> (LintLevel, bool);

    #[salsa::transparent]
    fn empty_global_lint_overwrites(&self) -> TiVec<Lint, Option<LintLevel>>;
}

fn lint(db: &dyn BaseDB, name: &str) -> Option<Lint> {
    db.lint_registry().lint_from_name(name)
}

fn lint_data(db: &dyn BaseDB, lint: Lint) -> LintData {
    db.lint_registry().lint_data(lint)
}

fn lint_lvl(
    db: &dyn BaseDB,
    lint: Lint,
    root_file: FileId,
    sctx: Option<ErasedItemTreeId>,
) -> (LintLevel, bool) {
    if let Some(sctx) = sctx {
        if let Some(lvl) = db.lint_overwrite(lint, sctx, root_file) {
            return (lvl, false);
        }
    }

    if let Some(lvl) = db.global_lint_overwrites(root_file)[lint] {
        return (lvl, false);
    }

    (db.lint_data(lint).default_lvl, true)
}

#[inline]
fn line_index(db: &dyn BaseDB, file_id: FileId) -> Arc<LineIndex> {
    let text = db.file_text(file_id);
    Arc::new(LineIndex::new(text.as_ref().map_or("", |t| &*t)))
}

#[inline]
fn line(db: &dyn BaseDB, pos: TextSize, file: FileId) -> Line {
    db.line_index(file).line(pos)
}

#[inline]
fn line_range(db: &dyn BaseDB, line: Line, file: FileId) -> TextRange {
    db.line_index(file).line_range(line)
}

fn empty_global_lint_overwrites(db: &dyn BaseDB) -> TiVec<Lint, Option<LintLevel>> {
    vec![None; db.plugin_lints().len() + lints::builtin::ALL.len()].into()
}
// #[inline]
// fn line_col(db: &dyn BaseDB, span: FileSpan) -> LineCol {
//     db.line_index(span.file).line_col(span.range.start())
// }

fn parse(db: &dyn BaseDB, root_file: FileId) -> Parse<SourceFile> {
    SourceFile::parse(&db.as_src_provider(), root_file)
}

fn preprocess(db: &dyn BaseDB, root_file: FileId) -> Preprocess {
    syntax::preprocess(&db.as_src_provider(), root_file)
}

// Update source files with
// ReadQuery.in_db_mut(self).invalidate(path);

#[inline]
fn file_text(db: &dyn BaseDB, file: FileId) -> Result<Arc<str>, FileReadError> {
    db.salsa_runtime().report_synthetic_read(Durability::LOW);
    let vfs = db.vfs().read();
    let contents = vfs.file_contents(file).ok_or(FileReadError::NotFound)?;
    let contents = from_utf8(contents).map_err(|_| FileReadError::InvalidTextFormat)?;
    Ok(Arc::from(contents))
}

fn file_path(db: &dyn BaseDB, file: FileId) -> VfsPath {
    db.vfs().read().file_path(file)
}

fn file_id(db: &dyn BaseDB, path: VfsPath) -> FileId {
    db.vfs().write().ensure_file_id(path)
}

#[inline]
fn sourcemap(db: &dyn BaseDB, root_file: FileId) -> Arc<SourceMap> {
    db.preprocess(root_file).sm
}

pub const STANDARD_FLAGS: [&str; 3] = [" __OPENVAF__", "__VAMS__", "__VAMS_COMPACT_MODELING__"];

pub trait Upcast<T: ?Sized> {
    fn upcast(&self) -> &T;
}

impl<'a> dyn BaseDB + 'a {
    pub fn as_src_provider(&self) -> impl SourceProvider + '_ {
        SourceProviderDelegate(self)
    }
}

struct SourceProviderDelegate<'a>(&'a dyn BaseDB);

impl<'a> SourceProvider for SourceProviderDelegate<'_> {
    #[inline(always)]
    fn include_dirs(&self, root_file: FileId) -> Arc<[VfsPath]> {
        self.0.include_dirs(root_file)
    }

    #[inline(always)]
    fn macro_flags(&self, root_file: FileId) -> Arc<[Arc<str>]> {
        self.0.macro_flags(root_file)
    }

    #[inline(always)]
    fn file_text(&self, file: FileId) -> Result<Arc<str>, FileReadError> {
        self.0.file_text(file)
    }

    #[inline(always)]
    fn file_path(&self, file: FileId) -> VfsPath {
        self.0.file_path(file)
    }

    #[inline(always)]
    fn file_id(&self, path: VfsPath) -> FileId {
        self.0.file_id(path)
    }
}

#[macro_export]
macro_rules! impl_intern_key {
    ($name:ident) => {
        impl ::salsa::InternKey for $name {
            fn from_intern_id(v: ::salsa::InternId) -> Self {
                $name(v)
            }
            fn as_intern_id(&self) -> ::salsa::InternId {
                self.0
            }
        }
    };
}

impl dyn BaseDB {
    pub fn setup_test_db(
        &mut self,
        root_file_name: &str,
        root_file: &str,
        vfs: &mut Vfs,
    ) -> FileId {
        let root_file = vfs.add_virt_file(root_file_name, root_file);
        vfs.insert_std_lib();

        let include_dirs = Arc::from(vec![VfsPath::new_virtual_path("/std".to_owned())]);
        self.set_include_dirs(root_file, include_dirs);

        let macro_flags: Vec<_> = STANDARD_FLAGS.iter().map(|x| Arc::from(*x)).collect();
        self.set_macro_flags(root_file, Arc::from(macro_flags));

        self.set_plugin_lints(&[]);
        let overwrites: Arc<[_]> = Arc::from(self.empty_global_lint_overwrites().as_ref());
        let overwrites = unsafe {
            transmute::<Arc<[Option<LintLevel>]>, Arc<TiSlice<Lint, Option<LintLevel>>>>(overwrites)
        };

        self.set_global_lint_overwrites(root_file, overwrites);

        root_file
    }

    pub fn apply_vfs_changes(&mut self) {
        let changes = self.vfs().write().take_changes();
        for change in changes {
            self.invalidate_file(change.file_id);
        }
    }

    pub fn invalidate_file(&mut self, file: FileId) {
        FileTextQuery.in_db_mut(self).invalidate(&file)
    }
}
