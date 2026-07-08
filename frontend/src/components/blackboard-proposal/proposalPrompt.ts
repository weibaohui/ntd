/**
 * 黑板「生成 Todo 建议」的 Prompt 模板常量。
 *
 * 设计取舍：
 * - 让 AI 自己用 cat 读取 topic 文件，而不是把全文塞进 params，既省 token，
 *   也与后端 build_wiki_prompt（backend/src/services/blackboard.rs）让 LLM 直接操作
 *   wiki 文件的既有模式保持一致；
 * - 占位符 {{topic_file_path}} 由 ProposalButton 在运行时替换为
 *   `~/.ntd/workspace/<id>/wiki/topics/<slug>.md`，`~` 交给 AI 的 shell 命令展开；
 * - 强约束输出「纯 YAML 列表」（title + prompt 两字段），方便前端 parseProposals
 *   用 js-yaml 稳定解析，降低格式容错压力。
 */
export const PROPOSAL_ACTION_TYPE = 'blackboard_propose';
export const PROPOSAL_ACTION_KEY = 'default';

/**
 * 生成建议与最终创建 Todo 统一使用的执行器。
 * 用户指定 pi（pi 为已注册执行器，见 backend/src/main.rs；本 workspace 日常即用 pi，
 * 默认 claudecode 在此环境未配置，故显式指定 pi 以确保能跑）。
 */
export const PROPOSAL_EXECUTOR = 'pi';

export const PROPOSAL_PROMPT = `你是任务拆解专家。先用 cat 命令（或文件读取工具）读取黑板主题文件：
  {{topic_file_path}}
然后分析其内容，识别其中值得行动的点：
- 待解决问题
- 矛盾 / 风险
- 下一步建议

针对每一个值得行动的点，生成一条可执行的 Todo 建议。每条建议必须包含：
- title：简洁有力的任务标题（不超过 30 字，动宾结构，不要带「建议」「应该」等虚词）
- prompt：完整的可执行 prompt，包含足够的背景与上下文，让执行器（如 Claude Code）能独立完成该任务

严格按以下 YAML 列表格式输出。不要加任何说明文字、不要解释、不要用 markdown 代码块标记包裹。
prompt 字段必须用 YAML 字面量块标量（prompt: | ）书写，以原样保留多行内容；
块内每个续行都必须比 prompt: 多缩进至少 2 个空格且缩进保持一致，
严禁出现顶格（无缩进）续行——否则前端解析会在该行截断、丢失后续内容：
- title: 任务标题1
  prompt: |
    完整的可执行 prompt1，可跨多行
- title: 任务标题2
  prompt: |
    完整的可执行 prompt2，可跨多行
`;
