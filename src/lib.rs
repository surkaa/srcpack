use anyhow::{Context, Result};
use ignore::WalkBuilder;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use zip::write::FileOptions;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use ignore::overrides::OverrideBuilder;
use zip::CompressionMethod;

/// Configuration for the file scanning process.
pub struct ScanConfig {
    /// The root directory from which the scan will start.
    pub root_path: PathBuf,
    /// Optional patterns to exclude from the scan.
    pub exclude_patterns: Vec<String>,
}

impl ScanConfig {
    /// Creates a new `ScanConfig` with the specified root path.
    pub fn new(path: impl Into<PathBuf>, excludes: Vec<String>) -> Self {
        Self {
            root_path: path.into(),
            exclude_patterns: excludes,
        }
    }
}

pub struct PackConfig {
    pub root_path: PathBuf,
    pub output_path: PathBuf,
    pub compression_method: CompressionMethod,
    // None Use the default, some(0-9) to specify the level
    pub compression_level: Option<i32>,
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
/// let config = ScanConfig::new(".", vec![String::from("*.mp4")]);
/// match scan_files(&config) {
///     Ok(files) => println!("Found {} files respecting .gitignore", files.len()),
///     Err(e) => eprintln!("Error scanning directory: {}", e),
/// }
/// ```
pub fn scan_files(config: &ScanConfig) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    let mut overrides = OverrideBuilder::new(&config.root_path);
    for pattern in &config.exclude_patterns {
        // ignore crate 的规则是：!pattern 表示忽略
        // 所以如果用户输入 "*.mp4"，我们需要转为 "!*.mp4" 告诉 builder 这是一个负面规则
        // 或者直接使用 builder.add("!*.mp4")
        let glob = format!("!{}", pattern);
        overrides.add(&glob).context("Invalid exclude pattern")?;
    }
    let override_matched = overrides.build()?;

    // WalkBuilder is the core builder from the ignore crate
    let walker = WalkBuilder::new(&config.root_path)
        .standard_filters(true) // Automatically read .gitignore, .git/info/exclude, etc.
        .overrides(override_matched) // Apply user-defined exclude patterns
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
/// use srcpack::{pack_files, ScanConfig, scan_files, PackConfig};
/// use std::path::Path;
///
/// let root = Path::new(".");
/// let config = ScanConfig::new(root, vec![]);
/// let files = scan_files(&config).unwrap(); // Get list of files first
/// let output = Path::new("backup.zip");
/// let pack_config = PackConfig {
///    root_path: root.to_path_buf(),
///    output_path: output.to_path_buf(),
///    compression_method: zip::CompressionMethod::Deflated,
///    compression_level: None,
/// };
///
/// // Pack the files with a simple progress closure
/// pack_files(&files, &pack_config, |path, size, total| {
///     println!("Packed {:?} ({} bytes)", path, size);
/// }).expect("Failed to pack files");
/// ```
pub fn pack_files<F>(
    files: &[PathBuf],
    config: &PackConfig,
    mut on_progress: F,
) -> Result<()>
where
    F: FnMut(&PathBuf, u64, u64) -> (),
{
    let file = File::create(&config.output_path)
        .with_context(|| format!("Failed to create output file: {:?}", &config.output_path))?;

    // Use a buffered writer to improve file I/O performance
    let buf_writer = BufWriter::with_capacity(1024 * 1024, file);
    let mut zip = zip::ZipWriter::new(buf_writer);

    // Set compression options: Default to Deflated (standard compression)
    let options = FileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(config.compression_level)
        .large_file(true); // Enable ZIP64 for large files

    let mut total_processed_size: u64 = 0;

    for path in files {
        // Calculate relative path (e.g., "src/main.rs")
        // If calculation fails (edge case), fallback to the full path
        let relative_path = path.strip_prefix(&config.root_path).unwrap_or(path);

        // Normalize path separators (Windows "\" -> Zip "/")
        // Crucial for cross-platform compatibility
        let path_str = relative_path.to_string_lossy().replace('\\', "/");

        // Read file content and stream it into the Zip
        let mut f = File::open(path)?;
        let metadata = f.metadata()?;

        // Preserve original file permissions if possible
        let permissions = if cfg!(unix) {
            #[cfg(unix)]
            {
                metadata.permissions().mode()
            }
            #[cfg(not(unix))]
            {
                0o644 // Windows/Other fallback
            }
        } else {
            0o644
        };

        // Start a new file in the Zip archive
        zip.start_file(path_str, options.clone().unix_permissions(permissions))?;

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
            if s == "node_modules"
                || s == "target"
                || s == "build"
                || s == "dist"
                || s == ".git"
                || s == ".idea"
                || s == ".vscode"
            {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::{Read, Write};
    use tempfile::tempdir;
    use zip::ZipArchive;

    /// Helper function to create a file with specific content
    fn create_test_file(dir: &Path, name: &str, content: &[u8]) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = File::create(path).unwrap();
        f.write_all(content).unwrap();
    }

    #[test]
    fn test_scan_filtering_logic() {
        // 1. Setup a temporary environment
        let temp_dir = tempdir().unwrap();
        let root = temp_dir.path();

        // 2. Create a mixed file structure (valid source code vs artifacts)

        // Valid files
        create_test_file(root, "src/main.rs", b"fn main() {}");
        create_test_file(root, "README.md", b"# Hello");
        // Hidden file that should be kept (unless ignored by gitignore)
        create_test_file(root, ".env", b"SECRET=123");

        // Hardcoded artifacts (should be ignored by is_build_artifact)
        create_test_file(root, "target/debug/app.exe", b"binary");
        create_test_file(root, "node_modules/react/index.js", b"module");
        create_test_file(root, ".git/HEAD", b"ref: refs/heads/main");
        create_test_file(root, ".vscode/settings.json", b"{}");

        // Gitignore logic
        create_test_file(root, ".gitignore", b"*.log\n/temp/");
        create_test_file(root, "error.log", b"error content"); // Should be ignored by *.log
        create_test_file(root, "temp/cache.bin", b"cache"); // Should be ignored by /temp/

        // 3. Execute Scan
        let config = ScanConfig::new(root, vec![]);
        let files = scan_files(&config).expect("Scan failed");

        // 4. Verification
        // Convert paths to relative strings for easier assertion
        let relative_paths: Vec<String> = files
            .iter()
            .map(|p| {
                p.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();

        // Assertions:
        // SHOULD contain:
        assert!(
            relative_paths.contains(&"src/main.rs".to_string()),
            "Missing src/main.rs"
        );
        assert!(
            relative_paths.contains(&"README.md".to_string()),
            "Missing README.md"
        );
        assert!(relative_paths.contains(&".env".to_string()), "Missing .env");
        assert!(
            relative_paths.contains(&".gitignore".to_string()),
            "Missing .gitignore"
        ); // We allowed hidden files, so .gitignore itself should be packed

        // SHOULD NOT contain (Hardcoded filters):
        assert!(
            !relative_paths.iter().any(|p| p.contains("target")),
            "Should exclude target"
        );
        assert!(
            !relative_paths.iter().any(|p| p.contains("node_modules")),
            "Should exclude node_modules"
        );
        assert!(
            !relative_paths.iter().any(|p| p.contains(".git/")),
            "Should exclude .git"
        );
        assert!(
            !relative_paths.iter().any(|p| p.contains(".vscode")),
            "Should exclude .vscode"
        );

        // SHOULD NOT contain (Gitignore filters):
        assert!(
            !relative_paths.contains(&"error.log".to_string()),
            "Should respect *.log in gitignore"
        );
        assert!(
            !relative_paths.contains(&"temp/cache.bin".to_string()),
            "Should respect /temp/ in gitignore"
        );
    }

    #[test]
    fn test_pack_integrity_and_round_trip() {
        // 1. Setup
        let temp_dir = tempdir().unwrap();
        let root = temp_dir.path();
        let output_zip_path = temp_dir.path().join("test_archive.zip");

        // Create some files with distinct content
        let file1_content = "Rust is awesome!";
        let file2_content = vec![0u8; 1024 * 10]; // 10KB dummy binary data

        create_test_file(root, "src/lib.rs", file1_content.as_bytes());
        create_test_file(root, "assets/data.bin", &file2_content);

        // Create a deep directory structure
        create_test_file(root, "a/b/c/d/deep.txt", b"Deep file");

        // 2. Scan
        let config = ScanConfig::new(root, vec![]);
        let files = scan_files(&config).unwrap();
        assert_eq!(files.len(), 3);

        // 3. Pack (Test the pack_files function)
        pack_files(
            &files,
            &PackConfig {
                root_path: root.to_path_buf(),
                output_path: output_zip_path.clone(),
                compression_method: CompressionMethod::Deflated,
                compression_level: None,
            },
            |_, _, _| {}, // Empty progress callback
        )
        .expect("Packing failed");

        assert!(output_zip_path.exists(), "Zip file was not created");

        // 4. Verify Integrity (Unzip and Compare)
        let zip_file = File::open(&output_zip_path).unwrap();
        let mut archive = ZipArchive::new(zip_file).unwrap();

        // Check if correct number of files are in zip
        assert_eq!(archive.len(), 3);

        // Check file 1: Content match
        let mut f1 = archive
            .by_name("src/lib.rs")
            .expect("src/lib.rs missing in zip");
        let mut buffer = String::new();
        f1.read_to_string(&mut buffer).unwrap();
        assert_eq!(buffer, file1_content, "Content mismatch for src/lib.rs");
        drop(f1); // Release borrow

        // Check file 2: Binary size match
        let f2 = archive
            .by_name("assets/data.bin")
            .expect("assets/data.bin missing");
        assert_eq!(
            f2.size(),
            file2_content.len() as u64,
            "Size mismatch for binary file"
        );
        drop(f2);

        // Check file 3: Path normalization (Windows backslash handling)
        // zip crate standardizes to forward slash, ensure our code did that
        let filenames: Vec<_> = archive.file_names().collect();
        assert!(
            filenames.contains(&"a/b/c/d/deep.txt"),
            "Deep path not preserved or normalized incorrectly"
        );

        // 5. Cleanup
        // The `temp_dir` object (from tempfile crate) automatically deletes
        // the directory and all contents when it goes out of scope here.
        // No manual deletion needed.
    }

    #[test]
    fn test_manual_exclude_patterns() {
        // 1. Setup
        let temp_dir = tempdir().unwrap();
        let root = temp_dir.path();

        // 2. Create a mixed environment
        // Files that should REMAIN
        create_test_file(root, "src/main.rs", b"code");
        create_test_file(root, "assets/logo.png", b"image");
        create_test_file(root, "docs/readme.txt", b"docs");

        // Files that should be EXCLUDED
        create_test_file(root, "assets/demo.mp4", b"heavy video"); // Exclude by extension
        create_test_file(root, "secrets/api_key.txt", b"super secret"); // Exclude by directory
        create_test_file(root, "secrets/nested/config.yaml", b"nested secret"); // Exclude by directory (deep)
        create_test_file(root, "backup.log", b"log file"); // Exclude by exact name

        // 3. Configure with Excludes
        // logic: user passes "pattern", code converts to "!pattern" for ignore crate
        let excludes = vec![
            "*.mp4".to_string(),   // Pattern 1: Glob extension
            "secrets".to_string(), // Pattern 2: Directory name
            "*.log".to_string(),   // Pattern 3: Glob extension
        ];

        let config = ScanConfig::new(root, excludes);

        // 4. Execute Scan
        let files = scan_files(&config).expect("Scan failed");

        // 5. Verify results
        let relative_paths: Vec<String> = files
            .iter()
            .map(|p| {
                p.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();

        // --- Positive Assertions (What should be there) ---
        assert!(relative_paths.contains(&"src/main.rs".to_string()), "Standard file should be present");
        assert!(relative_paths.contains(&"assets/logo.png".to_string()), "Non-excluded asset should be present");
        assert!(relative_paths.contains(&"docs/readme.txt".to_string()), "Docs should be present");

        // --- Negative Assertions (What should be gone) ---
        // Verify *.mp4 is gone
        assert!(
            !relative_paths.iter().any(|p| p.ends_with(".mp4")),
            "Failed to exclude .mp4 files"
        );

        // Verify secrets directory is gone (including nested files)
        assert!(
            !relative_paths.iter().any(|p| p.starts_with("secrets/")),
            "Failed to exclude secrets directory"
        );

        // Verify *.log is gone
        assert!(
            !relative_paths.iter().any(|p| p.ends_with(".log")),
            "Failed to exclude .log files"
        );
    }
}
