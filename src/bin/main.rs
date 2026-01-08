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

    /// 结束后显示最大的 N 个文件
    #[arg(long, default_value_t = 10)]
    top: usize,
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
        return Ok(());
    }

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

    println!("准备压缩到: {:?}", output_path.file_name().unwrap());

    // 设置压缩时的进度条
    let bar = ProgressBar::new(files.len() as u64);
    bar.set_style(
        ProgressStyle::with_template(
            // [耗时] [进度条] 进度/总数 百分比 (预计剩余时间) 当前文件
            "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {percent}% (ETA: {eta}) {msg}",
        )?
            .progress_chars("##-"),
    );

    // 内存中保存 Top N 最大文件 (大小, 相对路径字符串)
    // 预分配容量稍微大一点避免频繁扩容
    let mut top_files: Vec<(u64, String)> = Vec::with_capacity(args.top + 1);

    pack_files(
        &files,
        &root_path,
        &output_path,
        |path_buf, current_size, total_size| {
            let relative_path = path_buf.strip_prefix(&root_path).unwrap_or(path_buf);
            let relative_path_str = relative_path.to_string_lossy().to_string();

            if args.top > 0 {
                top_files.push((current_size, relative_path_str.clone()));
                // 降序排序：大文件在前
                top_files.sort_by(|a, b| b.0.cmp(&a.0));
                // 保持只有 Top N
                if top_files.len() > args.top {
                    top_files.truncate(args.top);
                }
            }

            bar.set_message(format!(
                "{} | 总计: {}",
                relative_path_str,
                format_size(total_size)
            ));

            bar.inc(1);
        },
    )?;

    bar.finish_with_message("压缩完成！");

    println!("\n✨ 成功！文件已保存至: {}", output_path.display());

    if !top_files.is_empty() {
        println!("\n📊 占用空间最大的 {} 个文件 (建议检查是否需要加入 .gitignore):", top_files.len());
        println!("{:-<60}", ""); // 分割线
        println!("{:<10} | {}", "大小", "文件路径");
        println!("{:-<60}", "");

        for (size, path) in top_files {
            println!("{:<12} | {}", format_size(size), path);
        }
        println!("{:-<60}", "");
    }

    Ok(())
}

// 简单的辅助函数：格式化字节大小
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
