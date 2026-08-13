/**
 * 黑板域共享类型（096-W4-4：从 BlackboardPage 抽离，供页面/弹窗/Hook 共同引用）。
 */

/** 黑板 API 返回的配置形状（与后端 BlackboardResponse 对应，不含内容） */
export interface BlackboardData {
  id: number;
  workspace_id: number;
  updated_at: string | null;
  /** 黑板更新防抖周期（秒）*/
  blackboard_debounce_secs: number;
  /** 黑板更新防抖条数阈值 */
  blackboard_debounce_count: number;
  /** Wiki 更新提示词模板（单阶段） */
  wiki_prompt: string;
  /** Wiki 对话使用的执行器名称，空/undefined 表示使用默认值 claudecode */
  wiki_chat_executor?: string | null;
  /** Wiki 执行超时（秒），控制 Wiki 任务与 Wiki 对话的最长存活时间 */
  wiki_timeout_secs: number;
  /** 黑板功能总开关 */
  enabled: boolean;
}

/** Wiki 文件列表项（对应后端 WikiFileItem） */
export interface WikiFileItem {
  slug: string;
  file_type: 'index' | 'topic' | 'log' | string;
}

/** Wiki 文件内容（对应后端 WikiFileContent） */
export interface WikiFileContent {
  slug: string;
  content: string;
}
