use clap::Parser;
use std::path::PathBuf;
use anyhow::{Context, Result};
use srcpack::{ScanConfig, scan_files, pack_files};

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

    println!("正在扫描目录: {:?}", root_path);

    let config = ScanConfig::new(&root_path);
    let files = scan_files(&config)?;

    println!("扫描完成，共找到 {} 个文件。", files.len());

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
                let dir_name = root_path.file_name()
                    .unwrap_or_else(|| std::ffi::OsStr::new("archive"))
                    .to_string_lossy();
                PathBuf::from(format!("{}.zip", dir_name))
            }
        };

        println!("正在压缩到: {:?}", output_path);

        // 调用核心压缩逻辑
        pack_files(&files, &root_path, &output_path)?;

        println!("成功！已创建压缩包: {}", output_path.display());
    }

    Ok(())
}