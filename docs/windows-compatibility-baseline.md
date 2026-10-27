# Windows 兼容基线（CP-01）

基准：2026-10-27 改造前代码；断言必须随迁移保留，不因新接口调整预期。
测试中的 Editor 是故障注入器，不是真实 TSF。实机历史证据见
[2026-10-22 验收](temporary-input-validation.md)，新制品实机验证留在 CP-10。

| 矩阵 | 可复现证据（Cargo 测试文件/模块） |
| --- | --- |
| W01 | core_engine/tests/state_machine_flow：uppercase、symbol、space_with_no_candidate；state_machine 单元测试 |
| W02 | state_machine_flow：wildcard、candidate_words_are_unique、prefix_candidates；pinyin_search |
| W03 | state_machine_flow：auto_commit_unique、overflow_code、overflow_exact_code |
| W04 | state_machine_flow：space/enter/backspace/esc、passthrough；input_session：preflight_matches_execution |
| W05 | state_machine_flow：digit_wildcard、select_out_of_range；temporary_input：invalid_candidate_number |
| W06 | key_filter：selecting_state_maps_quick_select_hotkeys、temporary_space_rule；temporary_input：page |
| W07 | key_filter：classify_shifted、ascii_punctuation；state_machine_flow：punctuation_after、uppercase |
| W08 | lifecycle：Shift 跟踪；input_session：preflight_and_execution_agree_on_idle_and_modified_keys |
| W09 | input_session：overflow_then_switch_to_english；历史实机语言栏/撤销证据 |
| W10 | temporary_input：unavailable_modes、empty_modes；temporary_bundle；table_options 校验 |
| W11 | temporary_input：english_punctuation、commits_once；历史实机英文原文/词组 |
| W12 | pinyin_search 全部用例；temporary_input：apostrophes、functional_configuration |
| W13 | temporary_input：empty_modes、backspace_to_empty |
| W14 | reverse_lookup；input_session：temporary_menu_never_enters_document |
| W15 | input_session：temporary_focus_switch_reload、external_termination、reload_and_focus_loss |
| W16 | input_session：failed_temporary_commit、failed_cleanup、failed_temporary_cancellation、partial_edit_failure |
| W17 | reload：failed_snapshot_keeps_old_revision、native_watcher；input_session：unchanged_runtime |
| W18 | user_dictionary 单元测试、im_engine 合并测试；table_session：invalid_table、config_write_failure |
| W19 | config/config_path/table_options 单元测试；temporary_loading；table_session：new_fields_survive |
| W20 | candidate_data、screen_geometry、tip_manager 测试；历史实机 GDI/设置/安装；浏览器、AppContainer、多显示器仍属未知项 |
| W21 | text_service 保持 prepare_context → sync_runtime → Shift/recover → test_key；TF_ES_SYNC 请求与 HRESULT 检查由代码审核固定；input_session 顶字只产出一个 CommitThenUpdate |
| W22 | input_session：nested_input_sees_advanced_state、switch_and_reload_advance_before_edit、external_termination_during_edit |
| W23 | input_session：none_preserves_view、candidate_and_spelling_views、missing_precise_anchor；Converted/Input 分支断言 |
| W24 | Config::from_str 与 validate_for_save 分开；TemporaryTables::load 入口关闭提前返回；table_session::validate_references 校验非空引用；temporary_loading 空引用 |

轨迹断言覆盖事件/consumed、模式/缓冲、提交与预编辑操作、候选/提示/页、失败清理、generation 和候选发布。
嵌套输入必须看见先行状态，旧调用即使失败也不能覆盖嵌套结果；开关与运行时在编辑前采用，失败不回滚。
None 保留整个旧视图与 anchor，纯预判不推进 generation。宿主正文不包含菜单和模式标签。

已知限制：commit_mode、punctuation_mode、enable_exact_match、enable_fuzzy、toggle_simplify 保留磁盘值但本系列不激活。
Windows fallback replace_len 使用 Rust scalar 数而非 UTF-16；非 BMP 范围风险单列，不能把它用于 Android 删除协议。
既有 LNK4104 与工作区严格 Clippy 问题不属于新增回归。测试临时目录曾仅用时钟命名，Windows 并发同 tick 可碰撞，CP-01 增加进程内序号隔离。
