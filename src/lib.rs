use ignore::WalkBuilder;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use zip::write::FileOptions;

/// 扫描配置
pub struct ScanConfig {
    pub root_path: PathBuf,
}

impl ScanConfig {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            root_path: path.into(),
        }
    }
}

/// 执行扫描，返回一个包含所有“合法”文件路径的列表
///
/// # Arguments
/// * `config` - 扫描配置
///
/// # Returns
/// * `Result<Vec<PathBuf>>` - 成功时返回文件路径列表，失败时返回错误
pub fn scan_files(config: &ScanConfig) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    // WalkBuilder 是 ignore crate 提供的核心构建器
    let walker = WalkBuilder::new(&config.root_path)
        .standard_filters(true) // 自动读取 .gitignore, .git/info/exclude 等
        .add_custom_ignore_filename(".ignore") // 允许用户自定义 .ignore 文件
        .build();

    for result in walker {
        match result {
            Ok(entry) => {
                let path = entry.path();

                // 过滤掉目录本身，我们只收集文件
                if path.is_file() {
                    if is_build_artifact(path) {
                        continue;
                    }

                    files.push(path.to_path_buf());
                }
            }
            Err(err) => {
                eprintln!("扫描出错: {}", err);
            }
        }
    }

    Ok(files)
}

/// 执行压缩
///
/// # Arguments
/// * `files` - 需要压缩的文件路径列表
/// * `root_path` - 根路径，用于计算相对路径
/// * `output_path` - 输出的 Zip 文件路径
/// * `on_progress` - 进度回调函数，每处理一个文件调用一次
/// # Returns
/// * `Result<()>` - 成功时返回空，失败时返回错误
pub fn pack_files<F>(
    files: &[PathBuf],
    root_path: &Path,
    output_path: &Path,
    on_progress: F
) -> Result<()>
where
    F: Fn()
{
    let file = File::create(output_path)
        .with_context(|| format!("无法创建输出文件: {:?}", output_path))?;

    let mut zip = zip::ZipWriter::new(file);
    // 设置压缩选项：默认使用 Deflated (标准压缩算法)
    let options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755); // 设置通用的权限，防止解压后只读

    // 用于读取文件内容的缓冲区
    let mut buffer = Vec::new();

    for path in files {
        // 1. 计算相对路径 (例如: "src/main.rs")
        // 如果无法计算相对路径（极端情况），就使用文件名
        let relative_path = path.strip_prefix(root_path).unwrap_or(path);

        // 2. 规范化路径分隔符 (Windows "\" -> Zip "/")
        // 这一步对于跨平台解压非常重要
        let path_str = relative_path.to_string_lossy().replace('\\', "/");

        // 3. 在 Zip 中开始一个新文件
        zip.start_file(path_str, options)?;

        // 4. 读取原文件内容并写入 Zip
        let mut f = File::open(path)?;
        buffer.clear();
        f.read_to_end(&mut buffer)?;
        zip.write_all(&buffer)?;

        on_progress();
    }

    // 完成写入
    zip.finish()?;

    Ok(())
}

fn is_build_artifact(path: &Path) -> bool {
    // 检查路径组件中是否包含常见的构建目录名称
    for component in path.components() {
        if let Some(s) = component.as_os_str().to_str() {
            if s == "node_modules" || s == "target" || s == "build" || s == "dist" {
                return true;
            }
        }
    }
    false
}
