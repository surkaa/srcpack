use ignore::WalkBuilder;
use std::path::{Path, PathBuf};
use anyhow::Result;

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

/// 简单的硬编码黑名单检查
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
