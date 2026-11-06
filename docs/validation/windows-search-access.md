# 用户目录安装后的 Windows 搜索输入修复

日期：2026-11-06。基线：`152ea50` / 已发布的 0.7.2。验证环境为已有 Windows 11 Sandbox，未重建、未下载发布资产或模型。

## 现象与对照

在任务栏搜索中选择 MyWubi 后仍出现微软拼音候选。`tasklist /m im_engine.dll` 显示 Explorer 已加载 DLL，SearchHost 未加载。注册路径为 `%LOCALAPPDATA%\MyWubi\current\im_engine.dll`。

配置和活动码表已有应用容器只读授权，但 DLL 及其所在目录只有当前用户、SYSTEM、Administrators 的权限。只给 DLL 授权仍未恢复；同时给其目录和 DLL 添加非继承的读取/执行权限后，同一 SearchHost 加载成功，`a` + 空格上屏“工”。没有修改 DLL 内容、注册类别或 TSF 状态机来完成这组对照。

旧修复 `c102560` 的共享配置路径、数据文件 ACL、窗口归属和重新激活处理仍在。此次缺口是用户私有安装目录中的 **DLL 加载权限**，不是旧修复被删除。Windows 对应用容器访问资源同时检查用户与容器权限，参见 [Microsoft 的 AppContainer 文档](https://learn.microsoft.com/en-us/windows/win32/secauthz/implementing-an-appcontainer)。

## 实现

- 将已有的数据 ACL 实现移到 `tip_manager::app_container`，设置程序继续复用原来的只读授权行为。
- `prepare_tip` 对明确的 DLL 文件及其直接父目录授予 `S-1-15-2-1`、`S-1-15-2-2` 的 RX 权限，不增加写权限，不继承给兄弟文件，不修改用户目录的整体权限。
- TIP 注册前调用该步骤，错误时不继续写注册表。
- 设置程序普通启动及 Velopack 安装后、更新后回调重新准备权限，覆盖更新替换文件导致 ACL 丢失的情形。回调不进行 TIP 注册、不要求提权、不访问网络。

## 验证

1. 新增 DLL/目录权限回归测试先失败：DLL 只读权限值 `1179785`，所需 RX 为 `1179817`。修复后通过。
2. `cargo test -p settings -p tip_manager --lib --locked --offline`：31 项通过。包括无继承/无写授权、保留现有权限、DLL 替换后重新授权、无效路径不修改目录权限。
3. `cargo test -p im_engine --locked --offline same_com_service_can_activate_after_deactivation -- --nocapture`：通过。
4. `cargo clippy -p tip_manager --all-targets --locked --offline -- -D warnings`：通过。
5. 包含 settings 的严格 Clippy 被已有 7 条告警阻断，位于 `app.rs`、`model_catalog.rs`、`panels/dictionary.rs`、`panels/appearance.rs`；这些文件本次未修改。
6. `cargo build -p settings -p im_engine --release --locked --offline`：通过；保留已有的 COM 导出 LNK4104 链接告警。
7. 在沙盒独立的用户私有目录运行实际构建产物的 `--veloapp-install 0.7.2` 和 `--veloapp-updated 0.7.2`：均退出 0。每次从缺少容器权限的状态开始，结果均为目录/DLL 有 RX，兄弟 `settings.exe` 没有获得授权。
8. 将本地构建的 settings/DLL 复制到沙盒现有安装目录，保留旧二进制备份，运行更新回调，重启 SearchHost。确认 SearchHost 和新启动的 Notepad 均加载该目录的 DLL。
9. 完整码表实测：Notepad、搜索栏均以 `w` + 空格上屏“人”；搜索栏切到微软拼音再切回 MyWubi，以 `a` + 空格继续上屏“工”。截图：[记事本](windows-search-access/notepad.png)、[搜索栏切换后](windows-search-access/search-reactivated.png)。

原沙盒仍引用前次验收的两词 `wubi86.dict` 测试表。已备份原配置及测试表，并将沙盒改为使用本地已有的完整 `wubi06.dict`；该表从本地仓库复制，未下载。发布包自带的 `wubi86.dict` 本身是开发示例，未把它误记为完整码表。宿主电脑的配置、注册和输入法未修改。

本次验证范围是基础五笔输入、候选显示/上屏、宿主切换及安装更新授权。候选框在搜索宿主中的精确光标定位、模型预测在 AppContainer 中的功能不在本次修复范围内。

## 沙盒中的本地修复产物

保持版本号 0.7.2，本地修复构建，尚未发布新 Release。只对本地复制的两个修复文件核对 SHA-256，没有回下载已发布资产：

| 文件 | SHA-256 |
| --- | --- |
| settings.exe | `E0B91280CE3BAAD960FA8824E20C9DFE88A96B9E1081D8D1ACA4946C42DD85F1` |
| im_engine.dll | `CA72D8FB1988CB80A3C7576147AC59BF844BFC030F61C68118D1C2CD44408CBC` |

安装/更新回调记录在 `target/mi36-39-sandbox/output/search-lifecycle.log`；旧文件备份为沙盒安装目录内的 `settings.pre-search-fix.exe` 和 `im_engine.pre-search-fix.dll`。测试使用现有沙盒和本地缓存，结束后关闭本次启动的设置程序、记事本和浏览器。
