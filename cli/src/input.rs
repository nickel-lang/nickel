use std::path::PathBuf;

use nickel_lang_core::{
    ast::InputFormat,
    eval::cache::CacheImpl,
    files::Files,
    program::{Program, ProgramBuilder},
};

#[cfg(feature = "package-experimental")]
use nickel_lang_package::{ManifestFile, config::Config as PackageConfig};

use crate::{customize::Customize, global::GlobalContext};

#[derive(clap::Parser, Debug)]
pub struct InputOptions<Customize: clap::Args, InputFormatOptions: clap::Args> {
    /// Input files. Omit to read from stdin. If multiple files are provided, the corresponding
    /// Nickel expressions are merged (combined with `&`) to produce the result.
    pub files: Vec<PathBuf>,

    /// Validates the final configuration against a contract specified as a Nickel file. If this
    /// argument is used multiple times, all specified contracts will be applied sequentially.
    #[arg(long)]
    apply_contract: Vec<PathBuf>,

    /// Skips the standard library import. For debugging only
    #[cfg(debug_assertions)]
    #[arg(long, global = true)]
    pub nostdlib: bool,

    /// Options for setting the format of input from stdin
    #[command(flatten)]
    pub format_options: InputFormatOptions,

    /// Adds a directory to the list of paths to search for imports in.
    ///
    /// When importing a file, Nickel searches for it relative to the file doing the
    /// import. If not found, it searches in the paths specified by `--import-path`.
    /// If not found there, it searches in the (colon-separated) list of paths contained
    /// in the environment variable `NICKEL_IMPORT_PATH`.
    #[arg(long, short = 'I', global = true)]
    pub import_path: Vec<PathBuf>,

    #[command(flatten)]
    pub customize_mode: Customize,

    /// Path to a package lock file.
    ///
    /// This is required for evaluations or imports that import packages.
    /// (Future versions may auto-detect or auto-create a lock file, in which
    /// case this argument will become optional.)
    #[arg(long, global = true)]
    pub manifest_path: Option<PathBuf>,

    /// Filesystem location for caching fetched packages.
    ///
    /// Defaults to an appropriate platform-dependent value, like
    /// `$XDG_CACHE_HOME/nickel` on linux.
    #[arg(long, global = true)]
    pub package_cache_dir: Option<PathBuf>,

    /// Enable incremental evaluation (experimental)
    #[arg(long, global = true)]
    pub incremental: bool,
}

pub enum PrepareError {
    /// Not a real error.
    ///
    /// Input preparation sometimes wants to print some info and then exit,
    /// without proceeding to program evaluation. This error variant is used to
    /// trigger the early return, but we don't print an error message or exit
    /// with an error status.
    EarlyReturn,
    /// A real error.
    Error(crate::error::Error),
}

impl<E: Into<crate::error::Error>> From<E> for PrepareError {
    fn from(e: E) -> Self {
        PrepareError::Error(e.into())
    }
}

pub type PrepareResult<T> = Result<T, PrepareError>;

pub trait Prepare {
    fn prepare(&self, ctx: &mut GlobalContext) -> PrepareResult<Program<CacheImpl>>;
}

impl<C: clap::Args + Customize, F: clap::Args + InputFormatOptions> Prepare for InputOptions<C, F> {
    fn prepare(&self, ctx: &mut GlobalContext) -> PrepareResult<Program<CacheImpl>> {
        // Resolve the package map first — it's independent of program construction and may itself
        // fail (manifest open / lock / package fetch).
        #[cfg(feature = "package-experimental")]
        let package_map = {
            let manifest_path = self.manifest_path.clone().or_else(|| {
                // If the manifest path isn't given, where should we start
                // looking for it? If there's only one file, we start with the
                // directory containing it. If there are no files, we take the
                // current directory.
                //
                // If there are multiple files, finding a good heuristic
                // is harder. For now, we take the parent directory if it's
                // unique and otherwise we require an explicit manifest
                // path.
                let mut parents = self.files.iter().map(|p| p.parent()).collect::<Vec<_>>();
                parents.sort();
                parents.dedup();
                let dir = match parents.as_slice() {
                    [] => std::env::current_dir().ok(),
                    [p] => p.map(std::path::Path::to_owned),
                    _ => None,
                };
                dir.and_then(|dir| crate::package::find_manifest(&dir).ok())
            });

            if let Some(manifest_path) = manifest_path {
                let manifest = ManifestFile::from_path(&manifest_path)?;
                let mut config = PackageConfig::new()?;
                if let Some(cache_dir) = self.package_cache_dir.as_ref() {
                    config = config.with_cache_dir(cache_dir.to_owned());
                };

                let (_lock, resolution) = manifest.lock(config.clone())?;
                nickel_lang_package::index::ensure_index_packages_downloaded(&resolution)?;
                Some(resolution.package_map(&manifest)?)
            } else {
                None
            }
        };

        let mut builder = ProgramBuilder::new()
            .with_trace(std::io::stderr())
            .with_reporter(ctx.reporter.clone());

        builder = if self.files.is_empty() {
            builder.add_stdin(self.format_options.stdin_format())
        } else {
            builder.add_paths(self.files.iter().cloned())
        };

        builder = builder
            .add_contract_paths(self.apply_contract.iter().cloned())
            .add_import_paths(self.import_path.iter().cloned());

        if let Ok(nickel_path) = std::env::var("NICKEL_IMPORT_PATH") {
            builder = builder.add_import_paths(nickel_path.split(':').map(PathBuf::from));
        }

        #[cfg(feature = "package-experimental")]
        if let Some(map) = package_map {
            builder = builder.with_package_map(map);
        }

        // The builder defers all I/O to `build()`, so any failure here is from opening an input
        // file, registering an in-memory source, or loading a contract path. All such errors are
        // `IOError`, whose rendering doesn't need the source database — `Files::empty()` is
        // sufficient.
        // `mut` is only needed in debug builds (for `set_skip_stdlib` below).
        #[cfg_attr(not(debug_assertions), allow(unused_mut))]
        let mut program = builder.build().map_err(|e| {
            PrepareError::Error(crate::error::Error::Program {
                files: Files::empty(),
                error: e.into(),
            })
        })?;

        #[cfg(debug_assertions)]
        if self.nostdlib {
            program.set_skip_stdlib();
        }

        self.customize_mode.customize(program)
    }
}

/// Trait to set the format when input to the cli is passed
/// in from stdin. This is to allow the --stdin-format flag to
/// override the default Nickel format for certain subcommands while
/// other commands can exclude it.
trait InputFormatOptions {
    fn stdin_format(&self) -> InputFormat;
}

/// Specifies that input from stdin should be treated as Nickel,
/// and cannot be overridden. The command will not have the --stdin-format flag.
#[derive(clap::Args, Debug)]
pub struct NickelOnly;

impl InputFormatOptions for NickelOnly {
    fn stdin_format(&self) -> InputFormat {
        InputFormat::Nickel
    }
}

/// Adds the --stdin-format flag to the subcommand to allow the format
/// of stdin to be specified.
#[derive(clap::Args, Debug)]
pub struct StdinFormat {
    /// Specify the format of the input from stdin
    #[arg(long, value_enum, default_value_t, conflicts_with = "files")]
    pub stdin_format: InputFormat,
}

impl InputFormatOptions for StdinFormat {
    fn stdin_format(&self) -> InputFormat {
        self.stdin_format
    }
}
