# NTD 前端功能清单

## 一、任务管理 (Todo Management)

### 1.1 任务列表 (TodoList)
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 任务展示 | 列表形式展示所有任务，显示标题、描述、执行器、标签 | TodoList.tsx |
| 标签筛选 | 按标签筛选任务，支持多标签筛选 | TodoList.tsx |
| 状态筛选 | 筛选 pending/running/completed/failed 状态任务 | TodoList.tsx |
| 搜索功能 | 支持按任务标题搜索 | KanbanBoard.tsx |
| 快速执行按钮 | 在列表中快速执行任务 | TodoList.tsx |
| 主题切换 | 支持亮色/暗色主题切换 | TodoList.tsx |
| 新建任务 | 通过右侧抽屉创建新任务 | TodoList.tsx |
| 智能创建 | 通过 AI 智能解析创建任务 | TodoList.tsx |

### 1.2 任务详情 (TodoDetail)
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 任务基本信息展示 | 显示标题、Prompt、执行器、标签 | TodoDetail.tsx |
| 执行历史 | 显示任务的所有执行记录 | TodoDetail.tsx |
| 执行统计 | 显示 Token 消耗、成本、成功率等统计 | TodoDetail.tsx |
| 直接执行 | 立即执行任务 | TodoDetail.tsx |
| 带参执行 | 传入额外参数执行任务 | TodoDetail.tsx |
| 继续对话 | 从上次中断处继续执行 | TodoDetail.tsx |
| 停止执行 | 强制停止正在执行的任务 | TodoDetail.tsx |
| 任务编辑 | 编辑任务标题、Prompt、标签等 | TodoDetail.tsx |
| 任务删除 | 删除任务 | TodoDetail.tsx |
| 状态变更 | 更改任务状态 | TodoDetail.tsx |
| 日志视图 | 以日志形式查看执行过程 | TodoDetail.tsx |
| 对话视图 | 以对话形式查看执行过程 | TodoDetail.tsx |
| YAML 导出 | 导出会话为 YAML 格式 | TodoDetail.tsx |
| 分页加载 | 执行记录分页加载 | TodoDetail.tsx |
| 状态过滤 | 按执行状态过滤历史记录 | TodoDetail.tsx |

### 1.3 任务卡片 (TodoCard)
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 任务信息展示 | 显示 ID、标题、执行器、时间、模型 | TodoCard.tsx |
| Prompt 展开/收起 | 可展开查看完整 Prompt | TodoCard.tsx |
| 结果展示 | 显示执行结论 | TodoCard.tsx |
| 标签展示 | 显示任务关联的标签 | TodoCard.tsx |
| 使用统计 | 显示耗时、Token 消耗、成本 | TodoCard.tsx |
| 触发类型 | 显示触发方式（Cron/手动） | TodoCard.tsx |
| 运行历史切换 | 切换查看不同执行记录 | TodoCard.tsx |
| 复制功能 | 复制 Prompt 或结论 | TodoCard.tsx |

### 1.4 任务抽屉 (TodoDrawer)
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 创建任务 | 创建新任务 | TodoDrawer.tsx |
| 编辑任务 | 编辑现有任务 | TodoDrawer.tsx |
| 执行器选择 | 选择执行器（Claude Code/Gemini 等） | TodoDrawer.tsx |
| Prompt 编辑 | Markdown 编辑器编辑 Prompt | TodoDrawer.tsx |
| 模板选择 | 从预设模板选择 | TodoDrawer.tsx |
| Skills 插入 | 插入 Skill 引用到 Prompt | TodoDrawer.tsx |
| 标签选择 | 选择任务标签 | TodoDrawer.tsx |
| 工作目录 | 设置执行工作目录 | TodoDrawer.tsx |
| Git Worktree | 启用 Git Worktree 模式 | TodoDrawer.tsx |
| 定时调度 | 设置 Cron 定时执行 | TodoDrawer.tsx |
| 定时预设 | 快速选择常用定时配置 | TodoDrawer.tsx |
| Prompt 参数 | 支持 {{content}}、{{message}} 等参数 | TodoDrawer.tsx |

### 1.5 智能创建 (SmartCreateModal)
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 自然语言输入 | 输入自然语言描述需求 | SmartCreateModal.tsx |
| AI 智能解析 | AI 自动处理并创建任务 | SmartCreateModal.tsx |
| 配置引导 | 未配置时引导用户设置 | SmartCreateModal.tsx |
| 快捷键支持 | Ctrl+Enter 提交 | SmartCreateModal.tsx |

---

## 二、看板视图 (Kanban Board)

### 2.1 看板布局
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 四列布局 | pending/running/completed/failed 四列 | KanbanBoard.tsx |
| 卡片拖拽 | 拖拽卡片跨列移动 | KanbanBoard.tsx |
| 拖拽反馈 | 拖拽时显示目标列高亮 | KanbanBoard.tsx |
| 状态自动更新 | 拖拽后自动更新任务状态 | KanbanBoard.tsx |
| 任务计数 | 每列显示任务数量 | KanbanBoard.tsx |

### 2.2 看板筛选
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 时间筛选 | 按 6h/12h/24h/3d/7d 筛选 | KanbanBoard.tsx |
| 关键词搜索 | 按标题/Prompt 搜索 | KanbanBoard.tsx |
| 移动端适配 | 移动端 Tab 切换模式 | KanbanBoard.tsx |
| 滑动手势 | 移动端左右滑动切换列 | KanbanBoard.tsx |

### 2.3 看板统计
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 实时统计 | 显示各状态任务数量 | KanbanBoard.tsx |
| 统计汇总 | 显示任务总数 | KanbanBoard.tsx |

---

## 三、仪表盘 (Dashboard)

### 3.1 数据概览
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 活跃任务 | 显示正在执行的任务 | Dashboard.tsx |
| 关键指标 | 今日执行、总执行、成功率、花费 | Dashboard.tsx |
| 亮点数据 | 单日峰值、最高产模型、活跃天数 | Dashboard.tsx |
| 任务概览 | 总任务、运行中、已完成、失败数 | Dashboard.tsx |
| 执行概览 | 标签数、定时任务、总执行、总花费 | Dashboard.tsx |

### 3.2 图表分析
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 任务状态分布 | 饼图展示各状态占比 | Dashboard.tsx |
| 触发来源分析 | 手动/定时/Cron/命令占比 | Dashboard.tsx |
| 执行器分布 | 各执行器执行次数和成功率 | Dashboard.tsx |
| 执行器耗时 | 各执行器平均执行时间 | Dashboard.tsx |
| 标签分布 | 各标签任务数量和执行情况 | Dashboard.tsx |
| Token 消耗 | 输入/输出/缓存读写占比 | Dashboard.tsx |
| 执行趋势 | 每日执行次数折线图 | Dashboard.tsx |
| 活动热力图 | GitHub 风格贡献热力图 | Dashboard.tsx |
| 模型任务分布 | 各模型执行次数 | Dashboard.tsx |
| 模型推理统计 | 各模型 Token 消耗和成本 | Dashboard.tsx |
| 缓存效率 | 各模型缓存命中率 | Dashboard.tsx |
| Token 趋势 | 每日 Token 消耗趋势 | Dashboard.tsx |
| 推理统计 | 推理输入/输出/成本/输出率 | Dashboard.tsx |
| 消息记录分析 | 飞书消息处理统计 | Dashboard.tsx |

### 3.3 时间范围
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 预设范围 | 5小时/7天/14天/30天 | Dashboard.tsx |
| 自定义范围 | 选择自定义日期范围 | Dashboard.tsx |

### 3.4 排行榜
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 模型排行榜 | 按执行次数排名的模型列表 | Dashboard.tsx |

### 3.5 分享卡片
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 分享卡片 | 生成统计数据分享图片 | Dashboard.tsx |

### 3.6 最近执行
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 执行记录表 | 显示最近执行的任务 | Dashboard.tsx |

---

## 四、纪念碑视图 (MemorialBoard)

### 4.1 结论视图
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 瀑布流展示 | 多列瀑布流展示已完成任务 | MemorialBoard.tsx |
| 时间筛选 | 按时间范围筛选 | MemorialBoard.tsx |
| 搜索功能 | 按标题/Prompt 搜索 | MemorialBoard.tsx |
| 结果展开 | 点击展开查看执行结论 | MemorialBoard.tsx |
| 成功/失败统计 | 显示成功和失败数量 | MemorialBoard.tsx |

### 4.2 看板视图
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 看板模式 | 切换到看板视图 | MemorialBoard.tsx |

---

## 五、执行面板 (ExecutionPanel)

### 5.1 任务显示
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 多任务标签 | 显示所有运行中的任务 | ExecutionPanel.tsx |
| 任务切换 | 切换查看不同任务日志 | ExecutionPanel.tsx |
| 实时日志 | 实时显示执行日志 | ExecutionPanel.tsx |
| 日志时间戳 | 显示每条日志的时间 | ExecutionPanel.tsx |
| 日志类型颜色 | 不同类型日志不同颜色 | ExecutionPanel.tsx |

### 5.2 任务控制
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 停止任务 | 停止正在执行的任务 | ExecutionPanel.tsx |
| 展开/收起 | 展开或收起面板 | ExecutionPanel.tsx |
| 全屏模式 | 全屏查看日志 | ExecutionPanel.tsx |
| 任务详情 | 查看任务详细信息 | ExecutionPanel.tsx |

### 5.3 自动清理
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 自动移除 | 执行完成后 5 秒自动移除 | ExecutionPanel.tsx |

---

## 六、对话视图 (ChatView)

### 6.1 消息展示
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 用户消息 | 显示用户输入 | ChatView.tsx |
| AI 响应 | 显示 AI 回复 | ChatView.tsx |
| 思考过程 | 显示 AI 思考过程（可折叠） | ChatView.tsx |
| 工具调用 | 显示工具调用详情（可折叠） | ChatView.tsx |
| 工具结果 | 显示工具执行结果 | ChatView.tsx |
| 系统消息 | 显示系统消息 | ChatView.tsx |
| 执行结论 | 显示最终执行结论 | ChatView.tsx |

### 6.2 交互功能
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 展开/折叠 | 消息块可展开/折叠 | ChatView.tsx |
| 实时打字指示 | 执行中显示打字指示器 | ChatView.tsx |
| Markdown 渲染 | 支持 Markdown 格式 | ChatView.tsx |
| 代码高亮 | 代码块语法高亮 | ChatView.tsx |

---

## 七、设置页面 (SettingsPage)

### 7.1 系统设置
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 服务端口 | 设置服务端口 | SettingsPage.tsx |
| 服务地址 | 设置服务地址 | SettingsPage.tsx |
| 数据库路径 | 设置数据库路径 | SettingsPage.tsx |
| 日志级别 | 选择 DEBUG/INFO/WARN/ERROR | SettingsPage.tsx |
| 默认时区 | 设置定时任务默认时区 | SettingsPage.tsx |

### 7.2 执行器管理
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 执行器列表 | 显示所有执行器 | SettingsPage.tsx |
| 启用/禁用 | 开关控制执行器 | SettingsPage.tsx |
| 路径配置 | 设置执行器二进制路径 | SettingsPage.tsx |
| Session 目录 | 设置 Session 目录 | SettingsPage.tsx |
| 批量检测 | 批量检测执行器可用性 | SettingsPage.tsx |
| 单个检测 | 检测单个执行器 | SettingsPage.tsx |
| 执行测试 | 测试执行器 | SettingsPage.tsx |

### 7.3 标签管理
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 创建标签 | 输入名称和颜色创建标签 | SettingsPage.tsx |
| 标签列表 | 显示所有标签 | SettingsPage.tsx |
| 删除标签 | 删除标签 | SettingsPage.tsx |
| 颜色选择 | 选择标签颜色 | SettingsPage.tsx |

### 7.4 项目目录
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 添加目录 | 添加项目目录路径 | SettingsPage.tsx |
| 目录命名 | 为目录设置名称 | SettingsPage.tsx |
| 编辑目录名 | 修改目录名称 | SettingsPage.tsx |
| 删除目录 | 删除目录 | SettingsPage.tsx |
| 自动完成 | 从历史目录自动补全 | SettingsPage.tsx |

### 7.5 备份与恢复
#### 7.5.1 Todo 备份
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 导出全部 | 导出所有 Todo 为 YAML | SettingsPage.tsx |
| 选择性导出 | 选择性导出部分 Todo | SettingsPage.tsx |
| 导入预览 | 预览导入内容 | SettingsPage.tsx |
| 选择性导入 | 选择性导入部分 Todo | SettingsPage.tsx |
| 自动备份 | 定时自动备份 Todo | SettingsPage.tsx |
| 备份文件管理 | 查看/下载/删除备份文件 | SettingsPage.tsx |

#### 7.5.2 Skill 备份
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 立即备份 | 手动触发 Skill 备份 | SettingsPage.tsx |
| 自动备份 | 定时自动备份 Skills | SettingsPage.tsx |
| 备份概览 | 显示各执行器 Skill 数量 | SettingsPage.tsx |
| 备份文件管理 | 查看/下载/删除备份文件 | SettingsPage.tsx |

#### 7.5.3 数据库备份
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 下载数据库 | 下载 SQLite 数据库文件 | SettingsPage.tsx |
| 服务器备份 | 备份到服务器 | SettingsPage.tsx |
| 数据库优化 | 压缩优化数据库 | SettingsPage.tsx |
| 自动备份 | 定时自动备份数据库 | SettingsPage.tsx |
| 备份文件管理 | 查看/下载/删除备份文件 | SettingsPage.tsx |
| 日志清理 | 清理旧日志记录 | SettingsPage.tsx |

### 7.6 Skills 管理
| 功能点 | 说明 | 文件 |
|--------|------|------|
| Skills 总览 | 显示所有 Skills | SkillsPanel.tsx |
| 搜索功能 | 搜索 Skills | SkillsPanel.tsx |
| 目录/扁平视图 | 切换显示模式 | SkillsPanel.tsx |
| Skill 详情 | 查看 Skill 详细内容 | SkillsPanel.tsx |
| 导出 Skill | 导出为 ZIP | SkillsPanel.tsx |
| 导入 Skill | 从 ZIP 导入 | SkillsPanel.tsx |
| 对比分析 | 对比不同执行器的 Skills | SkillsPanel.tsx |
| 同步管理 | 在执行器间同步 Skills | SkillsPanel.tsx |
| 调用追踪 | 查看 Skills 调用记录 | SkillsPanel.tsx |

### 7.7 运行管理
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 最大并发数 | 设置最大并发任务数 | SettingsPage.tsx |
| 超时时间 | 设置任务超时时间 | SettingsPage.tsx |
| 运行中任务 | 显示运行中的任务 | SettingsPage.tsx |
| 批量停止 | 批量停止选中的任务 | SettingsPage.tsx |
| 单个停止 | 停止单个任务 | SettingsPage.tsx |

### 7.8 消息配置
#### 7.8.1 飞书绑定
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 绑定飞书 | 通过二维码绑定飞书 Bot | SettingsPage.tsx |
| Bot 列表 | 显示已绑定的 Bots | SettingsPage.tsx |
| Bot 配置 | 配置 Bot 的各项功能 | SettingsPage.tsx |
| 删除 Bot | 删除已绑定的 Bot | SettingsPage.tsx |

#### 7.8.2 消息推送
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 单聊配置 | 配置单聊消息推送 | SettingsPage.tsx |
| 群聊配置 | 配置群聊消息推送 | SettingsPage.tsx |
| 合并策略 | 设置消息合并策略 | SettingsPage.tsx |
| 推送目标 | 设置消息推送目标 | SettingsPage.tsx |

#### 7.8.3 历史消息
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 历史消息 | 查看飞书历史消息 | SettingsPage.tsx |
| 消息筛选 | 按聊天/发送者筛选 | SettingsPage.tsx |
| 消息搜索 | 搜索消息内容 | SettingsPage.tsx |
| 群聊配置 | 配置历史群聊 | SettingsPage.tsx |
| 发送者管理 | 管理消息发送者 | SettingsPage.tsx |

### 7.9 消息规则
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 斜杠命令 | 配置斜杠命令规则 | SettingsPage.tsx |
| 默认响应 | 配置默认响应 Todo | SettingsPage.tsx |
| 规则启用 | 开关控制规则 | SettingsPage.tsx |
| 规则排序 | 规则优先级排序 | SettingsPage.tsx |

### 7.10 版本信息
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 版本显示 | 显示当前版本 | SettingsPage.tsx |
| Git 信息 | 显示 Git 提交信息 | SettingsPage.tsx |

---

## 八、Skills 管理 (SkillsPanel)

### 8.1 Skills 总览
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 统计卡片 | 显示 Skill 总数、执行器数等 | SkillsPanel.tsx |
| 树形列表 | 按执行器分组显示 Skills | SkillsPanel.tsx |
| 搜索功能 | 搜索 Skill 名称和描述 | SkillsPanel.tsx |
| 分类视图 | 显示目录结构 | SkillsPanel.tsx |
| 扁平视图 | 扁平显示所有 Skills | SkillsPanel.tsx |
| Skill 详情 | 查看 Skill 元信息和内容 | SkillsPanel.tsx |
| 导出功能 | 导出 Skill 为 ZIP | SkillsPanel.tsx |
| 导入功能 | 从 ZIP 导入 Skill | SkillsPanel.tsx |

### 8.2 对比分析
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 跨执行器对比 | 对比不同执行器的 Skills | SkillsPanel.tsx |
| 共享 Skills | 显示多个执行器共有的 Skills | SkillsPanel.tsx |
| 独有 Skills | 显示单个执行器独有的 Skills | SkillsPanel.tsx |
| 搜索过滤 | 搜索特定 Skill | SkillsPanel.tsx |

### 8.3 同步管理
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 源选择 | 选择源执行器 | SkillsPanel.tsx |
| Skill 选择 | 选择要同步的 Skill | SkillsPanel.tsx |
| 目标选择 | 选择目标执行器 | SkillsPanel.tsx |
| 批量同步 | 同步到多个目标 | SkillsPanel.tsx |

### 8.4 调用追踪
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 调用记录 | 显示 Skill 调用历史 | SkillsPanel.tsx |
| 统计概览 | 显示各 Skill 调用次数 | SkillsPanel.tsx |
| 按 Skill 筛选 | 按 Skill 名称筛选 | SkillsPanel.tsx |
| 按执行器筛选 | 按执行器筛选 | SkillsPanel.tsx |
| 分页加载 | 分页加载调用记录 | SkillsPanel.tsx |

---

## 九、模板管理

### 9.1 模板列表
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 模板展示 | 网格展示所有模板 | TodoDrawer.tsx |
| 分类导航 | 侧边栏显示分类 | TodoDrawer.tsx |
| 模板搜索 | 按标题/内容搜索 | TodoDrawer.tsx |
| 系统标识 | 区分系统/自定义模板 | TodoDrawer.tsx |

### 9.2 模板操作
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 选择模板 | 使用模板内容 | TodoDrawer.tsx |
| 插入模板 | 在光标位置插入模板 | TodoDrawer.tsx |
| 创建模板 | 创建新模板 | SettingsPage.tsx |
| 编辑模板 | 编辑现有模板 | SettingsPage.tsx |
| 删除模板 | 删除模板 | SettingsPage.tsx |
| 复制模板 | 复制模板 | SettingsPage.tsx |

### 9.3 自定义模板
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 远程订阅 | 订阅远程模板 URL | SettingsPage.tsx |
| 自动同步 | 定时自动同步模板 | SettingsPage.tsx |
| 取消订阅 | 取消远程订阅 | SettingsPage.tsx |
| 手动同步 | 手动触发同步 | SettingsPage.tsx |

---

## 十、主题与国际化

### 10.1 主题系统
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 亮色主题 | 亮色模式 | themes/index.ts |
| 暗色主题 | 暗色模式 | themes/index.ts |
| 主题切换 | 一键切换主题 | useTheme.tsx |
| 本地存储 | 记住主题选择 | useTheme.tsx |

### 10.2 国际化
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 中文支持 | 中文界面 | main.tsx |

---

## 十一、响应式设计

### 11.1 桌面端
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 双栏布局 | 列表+详情双栏 | App.tsx |
| 侧边抽屉 | 右侧滑出编辑面板 | App.tsx |
| 完整功能 | 显示所有功能 | 全局 |

### 11.2 移动端
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 单栏布局 | 列表/详情切换 | App.tsx |
| FAB 浮动按钮 | 浮动操作按钮 | App.tsx |
| 底部抽屉 | 底部滑出创建面板 | App.tsx |
| 移动端看板 | Tab 切换模式 | KanbanBoard.tsx |
| 返回按钮 | 返回列表按钮 | 各组件 |
| 手势支持 | 滑动手势 | KanbanBoard.tsx |

---

## 十二、状态管理

### 12.1 全局状态
| 功能点 | 说明 | 文件 |
|--------|------|------|
| Todo 状态 | 任务列表、选中状态 | useApp.tsx |
| 执行状态 | 执行记录、运行中任务 | useApp.tsx |
| UI 状态 | 加载状态 | useApp.tsx |
| 标签状态 | 标签列表 | useApp.tsx |

### 12.2 实时更新
| 功能点 | 说明 | 文件 |
|--------|------|------|
| SSE 事件 | 实时接收执行状态 | useExecutionEvents.ts |
| 任务启动 | 新任务启动时更新 | useExecutionEvents.ts |
| 日志追加 | 实时追加日志 | useExecutionEvents.ts |
| 任务完成 | 任务完成时更新状态 | useExecutionEvents.ts |
| 任务失败 | 任务失败时更新状态 | useExecutionEvents.ts |

---

## 十三、工具函数

### 13.1 数据库操作
| 功能点 | 说明 | 文件 |
|--------|------|------|
| CRUD 操作 | 任务的增删改查 | database.ts |
| 执行记录 | 执行记录查询 | database.ts |
| 统计查询 | Dashboard 统计数据 | database.ts |
| 备份导出 | 数据备份导出 | database.ts |

### 13.2 时间处理
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 相对时间 | 显示"刚刚"、"5分钟前"等 | datetime.ts |
| 绝对时间 | 显示具体日期时间 | datetime.ts |
| 时间格式化 | 各种时间格式转换 | datetime.ts |

### 13.3 Cron 处理
| 功能点 | 说明 | 文件 |
|--------|------|------|
| Cron 解析 | 解析 Cron 表达式 | cron.ts |
| 中文本地化 | Cron 中文显示 | cron.ts |
| 格式转换 | 5段/6段 Cron 转换 | cron.ts |

### 13.4 Markdown 处理
| 功能点 | 说明 | 文件 |
|--------|------|------|
| YAML 转换 | 对话转 YAML | markdown.ts |
| Markdown 渲染 | Markdown 内容渲染 | markdown.ts |

---

## 十四、错误处理

### 14.1 错误边界
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 组件错误捕获 | 捕获组件渲染错误 | ErrorBoundary.tsx |
| 降级显示 | 错误时显示降级 UI | ErrorBoundary.tsx |

### 14.2 网络错误
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 请求拦截 | 统一错误处理 | database.ts |
| 重试机制 | 失败请求重试 | database.ts |
| 超时处理 | 请求超时处理 | database.ts |

---

## 十五、路由与导航

### 15.1 URL 路由
| 功能点 | 说明 | 文件 |
|--------|------|------|
| View 参数 | 支持 view=dashboard/settings/memorial | App.tsx |
| Todo 参数 | 支持 todo=ID 直接打开任务 | App.tsx |
| 浏览器历史 | 浏览器前进/后退支持 | App.tsx |

### 15.2 面板切换
| 功能点 | 说明 | 文件 |
|--------|------|------|
| 列表面板 | 显示任务列表 | App.tsx |
| 详情面板 | 显示任务详情 | App.tsx |
| 移动端切换 | 移动端面板切换 | App.tsx |
