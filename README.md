# srcpack

**srcpack** is a blazing fast CLI tool to pack source code into a ZIP file, automatically respecting `.gitignore` rules.

It helps you back up or share your code without manually excluding `node_modules`, `target`, or `.git` folders.

## Installation

```bash
cargo install srcpack
```

## Usage

Run inside your project directory:

```bash
srcpack
```

### Options

```bash
# Pack a specific directory
srcpack path/to/project

# Specify output filename
srcpack --output my-backup.zip

# Analyze mode: Dry run to list files without zipping
srcpack --dry-run

# Analyze mode + Top files: Find the largest space consumers
srcpack --dry-run --top 20
```
