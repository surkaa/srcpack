use clap::Parser;
use std::path::PathBuf;
use anyhow::Result;
use srcpack::{ScanConfig, scan_files};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// 要扫描的根目录，默认为当前目录
    #[arg(default_value = ".")]
    path: PathBuf,

    /// 预演模式：只打印文件列表，不进行压缩
    #[arg(long, short = 'd')]
    dry_run: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    println!("正在扫描目录: {:?}", args.path);

    let config = ScanConfig::new(&args.path);
    let files = scan_files(&config)?;

    println!("扫描完成，共找到 {} 个文件。", files.len());

    if args.dry_run {
        println!("--- 文件列表 (Dry Run) ---");
        for file in files {
            println!("{}", file.display());
        }
    } else {
        println!("准备压缩... (功能开发中)");
    }

    Ok(())
}