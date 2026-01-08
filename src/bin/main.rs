use anyhow::{Context, Result};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use srcpack::{ScanConfig, pack_files, scan_files};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(
    name = "srcpack",
    author = "SurKaa",
    version,
    about = "A fast CLI tool to pack source code respecting .gitignore",
    long_about = "srcpack is a utility to compress source code directories into zip files. \
                  It automatically reads .gitignore files to exclude build artifacts like target/, node_modules/, etc."
)]
struct Args {
    /// Root directory to scan
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Output zip file path
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Dry run: Scan and analyze files without creating a zip
    #[arg(long, short = 'd')]
    dry_run: bool,

    /// Show the top N the largest files (only works in dry-run mode or after compression)
    ///
    /// This option will list the largest files found to help you identify what is taking up space.
    #[arg(long, default_value_t = 0, requires = "dry_run")]
    top: usize,

    /// Manually exclude patterns (e.g. "*.mp4", "secrets/")
    #[arg(long, short = 'x')]
    exclude: Vec<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let root_path = std::fs::canonicalize(&args.path)
        .with_context(|| format!("Cannot access directory: {:?}", args.path))?;

    // --- Scanning ---
    let scan_spinner = ProgressBar::new_spinner();
    scan_spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")?
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
    );
    scan_spinner.set_message(format!(
        "Scanning: {:?}",
        root_path.file_name().unwrap_or_default()
    ));
    scan_spinner.enable_steady_tick(Duration::from_millis(100));

    let config = ScanConfig::new(&root_path, args.exclude);
    let files = scan_files(&config)?;

    scan_spinner.finish_with_message(format!("Found {} files.", files.len()));

    // --- Dry Run / Analysis Mode ---
    if args.dry_run {
        println!("\n--- Dry Run Mode (No Zip Created) ---");

        let mut file_stats = Vec::with_capacity(files.len());
        let mut total_size: u64 = 0;

        // Calculate sizes quickly
        for file in &files {
            let size = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
            total_size += size;
            file_stats.push((size, file));
        }

        // Print all files (standard behavior)
        // User can pipe this to 'more' or 'less'
        if args.top == 0 {
            for (_, file) in &file_stats {
                let display_path = file.strip_prefix(&root_path).unwrap_or(file);
                println!("{}", display_path.display());
            }
        }

        println!("\nTotal size: {}", format_size(total_size));

        // If top is specified, show the analysis
        if args.top > 0 {
            print_top_files(&mut file_stats, args.top, &root_path);
        } else {
            println!("Tip: Use '--top 10' with '--dry-run' to see the largest files.");
        }

        return Ok(());
    }

    // --- Compression Mode ---
    let output_path = match args.output {
        Some(p) => p,
        None => {
            let dir_name = root_path
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("archive"))
                .to_string_lossy();
            PathBuf::from(format!("{}.zip", dir_name))
        }
    };

    println!("Compressing to: {:?}", output_path.file_name().unwrap());

    let bar = ProgressBar::new(files.len() as u64);
    bar.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {percent}% (ETA: {eta}) {msg}",
        )?
        .progress_chars("##-"),
    );

    pack_files(
        &files,
        &root_path,
        &output_path,
        |path_buf, _, total_size| {
            let relative_path = path_buf.strip_prefix(&root_path).unwrap_or(path_buf);
            let relative_path_str = relative_path.to_string_lossy().to_string();

            let display_name = truncate(&relative_path_str, 35);

            bar.set_message(format!(
                "{} | Total: {}",
                display_name,
                format_size(total_size)
            ));

            bar.inc(1);
        },
    )?;

    bar.finish_with_message("Done!");
    println!("\n✨ Success! Saved to: {}", output_path.display());

    Ok(())
}

fn print_top_files(files: &mut Vec<(u64, &PathBuf)>, n: usize, root: &PathBuf) {
    // Sort descending by size
    files.sort_by(|a, b| b.0.cmp(&a.0));

    let count = n.min(files.len());

    println!("\n📊 Largest {} files (Analysis):", count);
    println!("{:-<60}", "");
    println!("{:<12} | {}", "Size", "File Path");
    println!("{:-<60}", "");

    for i in 0..count {
        let (size, path) = files[i];
        let relative_path = path.strip_prefix(root).unwrap_or(path);
        println!("{:<12} | {}", format_size(size), relative_path.display());
    }
    println!("{:-<60}", "");
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * 1024 * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    // Get the total number of characters (not bytes)
    let char_count = s.chars().count();

    if char_count <= max_chars {
        return s.to_string();
    }

    // Calculate the starting character position that needs to be retained
    let chars_to_keep = max_chars.saturating_sub(3);
    let skip_count = char_count.saturating_sub(chars_to_keep);

    // Collect the remaining characters
    let kept_str: String = s.chars().skip(skip_count).collect();

    format!("...{}", kept_str)
}
