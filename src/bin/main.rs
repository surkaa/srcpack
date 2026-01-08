use anyhow::{Context, Result};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use srcpack::{pack_files, scan_files, ScanConfig};
use std::path::{PathBuf};
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// 要扫描的根目录，默认为当前目录
    #[arg(default_value = ".")]
    path: PathBuf,

    /// 指定输出文件名 (可选)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// 预演模式：只打印文件列表，不进行压缩
    #[arg(long, short = 'd')]
    dry_run: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // 获取绝对路径，方便后续处理
    let root_path = std::fs::canonicalize(&args.path)
        .with_context(|| format!("无法访问目录: {:?}", args.path))?;

    // 1. 设置扫描时的 Spinner (转圈圈)
    // 这是一个未定长度的进度条，适合扫描过程
    let scan_spinner = ProgressBar::new_spinner();
    scan_spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")?
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
    );
    scan_spinner.set_message(format!(
        "正在扫描: {:?}",
        root_path.file_name().unwrap_or_default()
    ));
    scan_spinner.enable_steady_tick(Duration::from_millis(100)); // 让它动起来

    // 执行扫描
    let config = ScanConfig::new(&root_path);
    let files = scan_files(&config)?;

    // 扫描完成，结束 Spinner
    scan_spinner.finish_with_message(format!("扫描完成，发现 {} 个文件", files.len()));

    if args.dry_run {
        println!("--- 文件列表 (Dry Run) ---");
        for file in files {
            // 这里为了显示好看，我们可以把绝对路径转回相对路径显示
            let display_path = file.strip_prefix(&root_path).unwrap_or(&file);
            println!("{}", display_path.display());
        }
    } else {
        // 决定输出文件名
        let output_path = match args.output {
            Some(p) => p,
            None => {
                // 如果没有指定输出文件名，使用目录名 + .zip
                let dir_name = root_path
                    .file_name()
                    .unwrap_or_else(|| std::ffi::OsStr::new("archive"))
                    .to_string_lossy();
                PathBuf::from(format!("{}.zip", dir_name))
            }
        };

        println!("准备压缩到: {:?}", output_path.file_name().unwrap());

        // 2. 设置压缩时的进度条
        let bar = ProgressBar::new(files.len() as u64);
        bar.set_style(
            ProgressStyle::with_template(
                // 优化了模板：把 msg 放到了最后，防止文件名过长破坏对齐
                "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {percent}% (ETA: {eta}) {msg}",
            )?
            .progress_chars("##-"),
        );

        let mut max_path: Option<String> = None;
        let mut max_size: usize = 0;

        pack_files(
            &files,
            &root_path,
            &output_path,
            |path_buf, current_size, total_size| {
                if current_size > max_size {
                    max_size = current_size;
                    max_path = Some(
                        path_buf.clone()
                            .strip_prefix(&root_path)
                            .unwrap_or(path_buf)
                            .to_string_lossy()
                            .to_string(),
                    );
                }

                // 格式化一下大小
                let size_str = format_size(total_size);

                bar.set_message(format!(
                    "- (总计: {}) 最大文件: {:?} ({})",
                    size_str,
                    max_path,
                    format_size(max_size)
                ));

                bar.inc(1);
            },
        )?;

        bar.finish();

        println!("成功！文件已保存至: {}", output_path.display());
    }

    Ok(())
}

// 简单的辅助函数：格式化字节大小
fn format_size(bytes: usize) -> String {
    const KB: usize = 1024;
    const MB: usize = 1024 * 1024;
    const GB: usize = 1024 * 1024 * 1024;

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
