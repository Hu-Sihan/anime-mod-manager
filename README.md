# Anime Mod Manager

面向 Genshin Impact / GIMI 的 3dmigoto 模组管理器，基于 Rust + GTK4 + libadwaita。从 GameBanana 在线浏览、筛选、一键下载安装，到本地模组启用 / 禁用管理，全部在统一窗口内完成。

## 软件截图

| 在线浏览与筛选 | 模组详情与版本选择 |
|---|---|
| [![浏览页](assets/remote-browse.png)](assets/remote-browse.png) | [![详情抽屉](assets/detail-drawer.png)](assets/detail-drawer.png) |
| **下载队列** | **本地模组管理** |
| [![下载页](assets/download-queue.png)](assets/download-queue.png) | [![本地页](assets/local-mods.png)](assets/local-mods.png) |
| **设置** ||
| [![设置页](assets/settings.png)](assets/settings.png) | |

### 主要功能

- **在线浏览**：GameBanana 全部约 18000 个模组的本地缓存与分页浏览，支持分类 / 子类 / 年龄 / 下载状态四层筛选
- **模组详情抽屉**：封面预览图、简介、标签、文件列表、版本切换，右侧滑出不影响浏览
- **下载队列**：可配置并发数，支持暂停 / 继续 / 断点续传，下载状态持久化到 `.anime-mod.json`，重启后自动恢复
- **安装引擎**：支持 `zip` / `7z` / `rar` 格式自动解压安装
- **本地管理**：已安装模组列表，启用 / 禁用切换，批量操作
- **GIMI Runtime 管理**：内置 runtime 版本检查与更新入口
- **图片缓存**：封面图在网络请求层统一缓存，256 条目 / 300s TTL，翻页预加载相邻页

项目包含两部分：

- `anime-mod-manager`：核心库，负责 GameBanana API、模组元数据、下载调度、本地扫描与安装。
- `mod-manager-demo`：基于 GTK4 + libadwaita 的桌面前端。

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
