use anyhow::{Context, Result};
use ignore::WalkBuilder;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use zip::write::FileOptions;

/// 扫描配置结构体
///
/// 用于构建扫描器时的参数配置，目前主要包含根目录路径。
pub struct ScanConfig {
    /// 要扫描的根目录路径
    pub root_path: PathBuf,
}

impl ScanConfig {
    /// 创建一个新的扫描配置
    ///
    /// # Arguments
    ///
    /// * `path` - 能够转换为 PathBuf 的路径类型
    ///
    /// # Examples
    ///
    /// ```
    /// use srcpack::ScanConfig;
    /// let config = ScanConfig::new(".");
    ///
    /// assert_eq!(config.root_path, std::path::PathBuf::from("."));
    /// ```
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            root_path: path.into(),
        }
    }
}

/// 执行文件扫描
///
/// 递归扫描指定目录，自动遵循 `.gitignore`、`.git/info/exclude` 等规则，
/// 并排除常见的构建产物（如 `node_modules`, `target` 等）。
///
/// # Arguments
///
/// * `config` - 包含根路径的配置对象
///
/// # Returns
///
/// * `Result<Vec<PathBuf>>` - 成功时返回符合条件的文件绝对路径列表
///
/// # Examples
///
/// ```no_run
/// use srcpack::{ScanConfig, scan_files};
///
/// let config = ScanConfig::new("./my_project");
/// let files = scan_files(&config).unwrap();
/// println!("Found {} files", files.len());
/// ```
pub fn scan_files(config: &ScanConfig) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    // WalkBuilder 是 ignore crate 提供的核心构建器
    let walker = WalkBuilder::new(&config.root_path)
        .standard_filters(true) // 自动读取 .gitignore
        .require_git(false)     // 不强制要求在 git 仓库内
        .hidden(false)          // 包含隐藏文件
        .build();

    for result in walker {
        match result {
            Ok(entry) => {
                let path = entry.path();

                // 过滤掉目录本身，只收集文件
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

/// 执行文件压缩打包
///
/// 将给定的文件列表打包成 Zip 文件。支持 Zip64（大文件），并使用流式读写以降低内存占用。
///
/// # Arguments
///
/// * `files` - 需要压缩的文件路径切片
/// * `root_path` - 项目根路径，用于计算 Zip 内的相对路径
/// * `output_path` - 输出 Zip 文件的完整路径
/// * `on_progress` - 进度回调闭包，参数为 `(当前文件路径, 当前文件大小, 已处理总大小)`
///
/// # Returns
///
/// * `Result<()>` - 操作成功返回 Ok(())
///
/// # Examples
///
/// ```no_run
/// use srcpack::pack_files;
/// use std::path::{Path, PathBuf};
///
/// let files = vec![PathBuf::from("src/main.rs")];
/// let root = Path::new(".");
/// let output = Path::new("backup.zip");
///
/// pack_files(&files, root, output, |path, curr, total| {
///     println!("Processing {:?} (Size: {})", path, curr);
/// }).unwrap();
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
        .with_context(|| format!("无法创建输出文件: {:?}", output_path))?;

    // 使用带缓冲区的写入器，提高写入性能
    let buf_writer = BufWriter::with_capacity(1024 * 1024, file);

    let mut zip = zip::ZipWriter::new(buf_writer);

    // 设置压缩选项：默认使用 Deflated (标准压缩算法)
    // 开启 large_file (Zip64) 以支持超过 4GB 的文件
    let options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755)
        .large_file(true);

    let mut total_processed_size: u64 = 0;

    for path in files {
        // 1. 计算相对路径
        let relative_path = path.strip_prefix(root_path).unwrap_or(path);

        // 2. 规范化路径分隔符 (Windows "\" -> Zip "/")
        let path_str = relative_path.to_string_lossy().replace('\\', "/");

        // 3. 在 Zip 中开始一个新文件
        zip.start_file(path_str, options)?;

        // 4. 读取原文件内容并流式写入 Zip
        let mut f = File::open(path)?;

        // 获取元数据以计算大小和进度
        let metadata = f.metadata()?;
        let current_file_size = metadata.len();

        // 使用 std::io::copy 进行流式传输，避免将大文件一次性读入内存
        std::io::copy(&mut f, &mut zip)?;

        total_processed_size += current_file_size;

        on_progress(path, current_file_size, total_processed_size);
    }

    // 完成写入
    zip.finish()?;

    Ok(())
}

/// 辅助函数：判断是否为构建产物或不需要的目录
fn is_build_artifact(path: &Path) -> bool {
    // 检查路径组件中是否包含常见的构建目录名称
    for component in path.components() {
        if let Some(s) = component.as_os_str().to_str() {
            if s == "node_modules" || s == "target" || s == "build" || s == "dist" || s == ".git" || s == ".idea" || s == ".vscode" {
                return true;
            }
        }
    }
    false
}
