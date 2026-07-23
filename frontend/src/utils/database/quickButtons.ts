import { api, unwrap } from './client';

/**
 * 快捷话术按钮：用户在回复框上方自定义的快捷按钮。
 * 按 workspace 隔离，点击把 prompt_text 填入回复输入框。
 */
export interface QuickButton {
  id: number;
  button_name: string;
  prompt_text: string;
  workspace_id?: number | null;
  created_at: string | null;
  updated_at: string | null;
}

export interface CreateQuickButtonParams {
  /** 按钮显示名称（同一 workspace 内唯一，重名后端返回 400） */
  button_name: string;
  /** 点击后填入回复输入框的话术 */
  prompt_text: string;
}

export interface UpdateQuickButtonParams {
  /** 新名称，省略表示不改；提供时不能与他人重名 */
  button_name?: string;
  /** 新话术，省略表示不改 */
  prompt_text?: string;
}

/** 列出当前 workspace 下的全部快捷按钮（后端按创建时间升序返回） */
export async function getQuickButtons(workspaceId: number): Promise<QuickButton[]> {
  return unwrap(await api.get(`/api/workspaces/${workspaceId}/quick-buttons`));
}

/** 创建快捷按钮。重名由后端返回 400，调用方 catch 后提示即可 */
export async function createQuickButton(
  workspaceId: number,
  params: CreateQuickButtonParams,
): Promise<{ id: number }> {
  return unwrap(await api.post(`/api/workspaces/${workspaceId}/quick-buttons`, params));
}

/** 更新快捷按钮（只传需要改的字段） */
export async function updateQuickButton(
  workspaceId: number,
  id: number,
  params: UpdateQuickButtonParams,
): Promise<void> {
  await api.put(`/api/workspaces/${workspaceId}/quick-buttons/${id}`, params);
}

/** 删除快捷按钮 */
export async function deleteQuickButton(workspaceId: number, id: number): Promise<void> {
  await api.delete(`/api/workspaces/${workspaceId}/quick-buttons/${id}`);
}
