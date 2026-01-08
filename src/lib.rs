use anyhow::{Context, Result};
use ignore::WalkBuilder;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use zip::write::FileOptions;

/// Configuration for the file scanning process.
pub struct ScanConfig {
    /// The root directory from which the scan will start.
    pub root_path: PathBuf,
}

impl ScanConfig {
    /// Creates a new `ScanConfig` with the specified root path.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            root_path: path.into(),
        }
    }
}

/// Scans the directory specified in the configuration and returns a list of files to include.
///
/// This function utilizes the `ignore` crate to respect `.gitignore` rules.
/// It also performs additional filtering to exclude common build artifacts
/// (such as `node_modules`, `target`, `.git`, etc.) regardless of gitignore settings.
///
/// # Arguments
///
/// * `config` - The configuration object containing the root path.
///
/// # Returns
///
/// * `Result<Vec<PathBuf>>` - A vector containing absolute paths to the valid files found.
/// # Example
///
/// ```no_run
/// use srcpack::{ScanConfig, scan_files};
///
/// let config = ScanConfig::new(".");
/// match scan_files(&config) {
///     Ok(files) => println!("Found {} files respecting .gitignore", files.len()),
///     Err(e) => eprintln!("Error scanning directory: {}", e),
/// }
/// ```
pub fn scan_files(config: &ScanConfig) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    // WalkBuilder is the core builder from the ignore crate
    let walker = WalkBuilder::new(&config.root_path)
        .standard_filters(true) // Automatically read .gitignore, .git/info/exclude, etc.
        .require_git(false) // Do not require a git repository to work
        .hidden(false) // Include hidden files (like .env), though specific ones are filtered later
        .build();

    for result in walker {
        match result {
            Ok(entry) => {
                let path = entry.path();

                // Filter out directories; we only collect files
                if path.is_file() {
                    // Apply hardcoded blacklist for common heavy directories
                    if is_build_artifact(path) {
                        continue;
                    }

                    files.push(path.to_path_buf());
                }
            }
            Err(err) => {
                eprintln!("Scan warning: {}", err);
            }
        }
    }

    Ok(files)
}

/// Compresses the provided list of files into a ZIP archive.
///
/// This function supports **ZIP64** extensions, allowing it to handle files larger than 4GB.
/// It uses stream-based copying (`std::io::copy`) to keep memory usage low.
///
/// # Arguments
///
/// * `files` - A slice of file paths to be compressed.
/// * `root_path` - The base path used to calculate relative paths inside the ZIP archive.
/// * `output_path` - The destination path for the generated ZIP file.
/// * `on_progress` - A closure called after each file is processed.
///     * Arguments: `(path: &PathBuf, current_file_size: u64, total_processed_size: u64)`
///
/// # Returns
///
/// * `Result<()>` - Returns Ok if the operation completes successfully.
///
/// # Example
///
/// ```no_run
/// use srcpack::{pack_files, ScanConfig, scan_files};
/// use std::path::Path;
///
/// let root = Path::new(".");
/// let config = ScanConfig::new(root);
/// let files = scan_files(&config).unwrap(); // Get list of files first
/// let output = Path::new("backup.zip");
///
/// // Pack the files with a simple progress closure
/// pack_files(&files, root, output, |path, size, total| {
///     println!("Packed {:?} ({} bytes)", path, size);
/// }).expect("Failed to pack files");
/// ```
pub fn pack_files<F>(
    files: &[PathBuf],
    root_path: &Path,
    output_path: &Path,
    mut on_progress: F,
) -> Result<()>
where
    F: FnMut(&PathBuf, u64, u64) -> (),
{
    let file = File::create(output_path)
        .with_context(|| format!("Failed to create output file: {:?}", output_path))?;

    // Use a buffered writer to improve file I/O performance
    let buf_writer = BufWriter::with_capacity(1024 * 1024, file);
    let mut zip = zip::ZipWriter::new(buf_writer);

    // Set compression options: Default to Deflated (standard compression)
    let options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755) // Set generic permissions to avoid read-only issues on unzip
        .large_file(true); // Enable ZIP64 for large files

    let mut total_processed_size: u64 = 0;

    for path in files {
        // 1. Calculate relative path (e.g., "src/main.rs")
        // If calculation fails (edge case), fallback to the full path
        let relative_path = path.strip_prefix(root_path).unwrap_or(path);

        // 2. Normalize path separators (Windows "\" -> Zip "/")
        // Crucial for cross-platform compatibility
        let path_str = relative_path.to_string_lossy().replace('\\', "/");

        // 3. Start a new file in the Zip archive
        zip.start_file(path_str, options)?;

        // 4. Read file content and stream it into the Zip
        let mut f = File::open(path)?;
        let metadata = f.metadata()?;
        let current_file_size = metadata.len();

        // Stream copy: reads from file and writes to zip buffer directly
        std::io::copy(&mut f, &mut zip)?;

        total_processed_size += current_file_size;
        on_progress(path, current_file_size, total_processed_size);
    }

    // Finalize the zip file structure
    zip.finish()?;

    Ok(())
}

/// Checks if a path belongs to a common build artifact or dependency directory.
///
/// This serves as a secondary hard-coded filter to ensure folders like `node_modules`
/// or `target` are never included, even if .gitignore is missing.
fn is_build_artifact(path: &Path) -> bool {
    // Check if the path component contains common build directory names
    for component in path.components() {
        if let Some(s) = component.as_os_str().to_str() {
            if s == "node_modules" || s == "target" || s == "build" || s == "dist" || s == ".git" || s == ".idea" || s == ".vscode" {
                return true;
            }
        }
    }
    false
}
