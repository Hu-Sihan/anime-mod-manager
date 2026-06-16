# Anime Mod Manager

一个基于 Rust 的 3dmigoto 模组管理器项目，当前包含两部分：

- `anime-mod-manager`：核心库，负责 GameBanana 数据拉取、模组元数据管理、下载状态持久化、本地模组扫描与安装。
- `mod-manager-demo`：基于 GTK4 + libadwaita 的桌面前端，用来验证浏览、下载、详情抽屉、本地管理、设置页等交互。

项目当前主要面向 Genshin Impact / GIMI 一类工作流，但核心层已经尽量按“浏览页 / 下载模块 / Meta 管理 / 本地模组管理”拆分。

## 当前特性

- 浏览 GameBanana 模组列表与详情
- 本地缓存封面图与图片资源
- 模组详情侧抽屉、版本文件列表、下载入口
- 下载队列、任务并发控制、暂停 / 继续 / 失败重试
- 下载状态落盘到 `.anime-mod.json`，支持重启后恢复任务
- 本地模组页筛选、搜索、启用 / 禁用、批量操作
- 安装 `zip` / `7z` / `rar` 压缩包
- 内置 GIMI runtime 管理入口和版本检查配置

## 项目结构

```text
.
├── src/                  # 核心库
│   ├── gamebanana.rs     # GameBanana API
│   ├── manager.rs        # 本地模组安装 / 启用 / 禁用
│   ├── meta_manager.rs   # .anime-mod.json 索引与按需读取
│   ├── mod_file_downloader.rs
│   └── models.rs         # 核心数据模型
├── demo/
│   ├── src/config.rs     # 前端配置读写（config.json）
│   ├── src/style.css     # GTK 样式
│   └── src/ui/           # 浏览页 / 下载页 / 设置页 / 详情抽屉
└── Cargo.toml            # workspace 根
```

## 运行要求

### Rust

- Rust stable
- Cargo

### 系统依赖

桌面前端依赖 GTK4 和 libadwaita。以 Debian / Ubuntu 系为例：

```bash
sudo apt install libgtk-4-dev libadwaita-1-dev
```

### 解压依赖

为了安装 `7z` / `rar` 模组压缩包，运行环境需要至少提供下面的工具之一：

- `7z`
- `bsdtar`

项目会优先使用可用工具进行解压。

## 启动方式

```bash
cargo run -p mod-manager-demo
```

## 配置文件

前端配置不再使用 YAML，当前统一使用运行目录下的 `config.json`。

默认配置结构如下：

```json
{
  "core": {
    "gimi_runtime": {
      "importer_directory": "/path/to/gimi",
      "managed_version": "v8.7.8",
      "github_repo_owner": "SilentNightSound",
      "github_repo_name": "GIMI-Package"
    }
  },
  "ui": {
    "language": "zh-CN",
    "night_mode": false
  },
  "network": {
    "concurrent_downloads": 3
  }
}
```

其中：

- `core.gimi_runtime.importer_directory`：GIMI runtime 目录
- `ui.language`：界面语言
- `ui.night_mode`：界面夜间模式开关
- `network.concurrent_downloads`：下载任务并发数

## 元数据设计

本地模组和下载任务通过每个模组目录下的 `.anime-mod.json` 持久化。

- `MetaManager` 负责扫描目录并建立 `uuid -> 模板种类` 索引
- `ModManager` 基于模组模板管理已安装 / 已禁用模组
- 下载模块基于下载模板恢复任务状态、重建队列和进度

当前设计目标是：

- 内存中只维护索引，不把全部 meta 一次性读入
- UI、下载调度器、模组管理器都通过 meta 交互
- 任务重启、失败恢复、封面复用尽量依赖同一份目录级数据

## 开发说明

常用检查命令：

```bash
cargo fmt --all
cargo check -p mod-manager-demo
```

## 许可证

MIT
