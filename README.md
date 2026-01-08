# srcpack

**srcpack** 一个极速的命令行工具，用于将源代码打包成 ZIP 文件，同时遵循 `.gitignore` 规则。

可以帮你彻底告别手动排除 `node_modules`、`target` 或 `.git` 目录的烦恼，让代码备份和分享变得轻而易举。

## 安装

```bash
cargo install srcpack
```

## 使用方法

在项目根目录下直接运行：

```bash
srcpack
```

### 常用选项

```bash
# 打包指定目录
srcpack path/to/project

# 仅预览将被打包的文件列表（不进行压缩）
srcpack --dry-run

# 指定输出文件名
srcpack --output my-backup.zip

# 打包后列出体积最大的 20 个文件（辅助优化 .gitignore）
srcpack --top 20
```
