/**
 * BlackboardSettingsModal — 黑板设置弹窗（096-W4-4：从 BlackboardPage 主组件拆出）。
 *
 * 承接整块设置族逻辑：8 个表单 state + 保存/恢复默认 handler + Modal/Tab JSX。
 * 主组件只持有 open 开关，表单数据在弹窗内聚（打开时从 configData 初始化，
 * 保存成功后经 onSaved 回写父组件缓存）。
 *
 * 函数体与 JSX 逐字搬自原实现，行为等价。
 */

import { useCallback, useEffect, useState } from 'react';
import { Button, Form, Input, InputNumber, Modal, Space, Switch, Tabs, message } from 'antd';
import { updateBlackboardConfig } from '@/utils/database/blackboard';
import type { BlackboardData } from './types';

/**
 * Wiki 提示词默认值（单阶段）：与后端 `build_wiki_prompt()` 内置模板保持一致。
 *
 * ⚠️ 注意：此为前端副本，后端 `backend/src/services/blackboard.rs` 的
 * `build_wiki_prompt()` 函数中也有一份，修改时需同步更新两处。
 * 用于在 UI 上展示默认提示词内容，以及"恢复默认"时回填。
 */
const DEFAULT_WIKI_PROMPT = `你是一个工作空间黑板维护者。你的任务是分析新的执行记录，更新 Wiki 页面。

你拥有以下工具，可以直接在执行过程中调用：
- \`ls ~/.ntd/workspace/{{workspace_id}}/wiki/topics/\`：列出现有主题页面
- \`cat ~/.ntd/workspace/{{workspace_id}}/wiki/topics/<slug>.md\`：读取页面内容
- \`ntd todo execution get <id>\`：获取指定执行记录的完整结论（result 字段）

待分析的执行记录 ID 列表：
{{pending_record_ids}}

请按以下步骤操作：
1. 列出现有主题页面，了解当前 Wiki 结构
2. 逐个调用 \`ntd todo execution get <id>\` 获取每条执行记录的结论
3. 分析每条结论涉及哪些主题领域
4. 对于新主题：创建 \`~/.ntd/workspace/{{workspace_id}}/wiki/topics/<slug>.md\`
5. 对于已有主题：编辑文件，追加/更新结论（保持已有内容）
6. 每个页面结构：
   - # 标题（中文）
   - ## 已确认
   - ## 新发现
   - ## 待解决问题
   - ## 矛盾/风险
   - ## 下一步建议
7. 每条结论标注来源，使用 \`ntd todo execution get <record_id>\` 返回结果中的 \`todo_id\` 和 \`id\` 字段，
   生成 app 内链接：(来源: [record_{record_id}](/#/todos/{todo_id}/posts/{record_id}))

完成后输出简短确认即可，无需输出 YAML/JSON。`;

export interface BlackboardSettingsModalProps {
  open: boolean;
  onClose: () => void;
  workspaceId: number;
  /** 黑板配置（打开弹窗时的表单初始化源；null 时用默认值兜底） */
  configData: BlackboardData | null;
  /** 保存成功后回写父组件的 configData 缓存 */
  onSaved: (updated: BlackboardData) => void;
  isMobile: boolean;
}

export function BlackboardSettingsModal({
  open,
  onClose,
  workspaceId,
  configData,
  onSaved,
  isMobile,
}: BlackboardSettingsModalProps) {
  const [settingsSaving, setSettingsSaving] = useState(false);
  const [debounceSecs, setDebounceSecs] = useState<number | null>(600);
  const [debounceCount, setDebounceCount] = useState<number | null>(10);
  const [wikiPrompt, setWikiPrompt] = useState<string>('');
  // Wiki 执行超时（秒）：与后端 DEFAULT_WIKI_TIMEOUT_SECS=300 一致，清空时回退默认
  const [wikiTimeoutSecs, setWikiTimeoutSecs] = useState<number | null>(300);
  const [bbEnabled, setBbEnabled] = useState<boolean>(true);
  const [activeTab, setActiveTab] = useState<'debounce' | 'prompt'>('debounce');

  // 打开弹窗时从已加载的黑板数据初始化表单（原 handleOpenSettings 的内联逻辑下沉）。
  // 配置由 GET /api/workspaces/{workspaceId}/blackboard 接口随内容一并返回（configData prop）。
  useEffect(() => {
    if (!open) return;
    if (configData) {
      setDebounceSecs(configData.blackboard_debounce_secs ?? 600);
      setDebounceCount(configData.blackboard_debounce_count ?? 10);
      setWikiPrompt(configData.wiki_prompt ?? '');
      setWikiTimeoutSecs(configData.wiki_timeout_secs ?? 300);
      setBbEnabled(configData.enabled ?? true);
    } else {
      setDebounceSecs(600);
      setDebounceCount(10);
      setWikiPrompt('');
      setWikiTimeoutSecs(300);
      setBbEnabled(true);
    }
    setActiveTab('debounce');
  }, [open, configData]);

  // 保存设置
  const handleSaveSettings = useCallback(async () => {
    setSettingsSaving(true);
    try {
      await updateBlackboardConfig(workspaceId, {
        // 用户清空输入时 null → 用默认值，避免后端意外覆盖
        blackboard_debounce_secs: debounceSecs ?? 600,
        blackboard_debounce_count: debounceCount ?? 10,
        wiki_prompt: wikiPrompt,
        wiki_timeout_secs: wikiTimeoutSecs ?? 300,
        enabled: bbEnabled,
      });
      // 保存成功后回写父组件缓存，避免下次打开弹窗读到旧值
      if (configData) {
        onSaved({
          ...configData,
          blackboard_debounce_secs: debounceSecs ?? 600,
          blackboard_debounce_count: debounceCount ?? 10,
          wiki_prompt: wikiPrompt,
          wiki_timeout_secs: wikiTimeoutSecs ?? 300,
          enabled: bbEnabled,
        });
      }
      message.success('设置已保存');
      onClose();
    } catch (err) {
      message.error('保存失败: ' + (err instanceof Error ? err.message : String(err)));
    } finally {
      setSettingsSaving(false);
    }
  }, [workspaceId, debounceSecs, debounceCount, wikiPrompt, wikiTimeoutSecs, bbEnabled, configData, onSaved, onClose]);

  // 恢复默认提示词：把 wikiPrompt 设为内置默认值。
  // 区别于"留空"的语义——留空表示后端使用内置默认；填入默认值表示用户显式采用内置模板。
  const handleRestorePrompt = useCallback(() => {
    setWikiPrompt(DEFAULT_WIKI_PROMPT);
  }, []);

  return (
    <Modal
      title="黑板设置"
      open={open}
      onOk={handleSaveSettings}
      onCancel={onClose}
      okText="保存"
      confirmLoading={settingsSaving}
      destroyOnHidden
      width={isMobile ? '90%' : 640}
    >
      <div style={{ marginBottom: 16, display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <span style={{ fontSize: 14, fontWeight: 500 }}>启用黑板</span>
        <Switch checked={bbEnabled} onChange={setBbEnabled} />
      </div>
      <Tabs
        activeKey={activeTab}
        onChange={(key) => setActiveTab(key as 'debounce' | 'prompt')}
        items={[
          {
            key: 'debounce',
            label: '防抖设置',
            children: (
              <DebounceSettingsTab
                debounceSecs={debounceSecs}
                setDebounceSecs={setDebounceSecs}
                debounceCount={debounceCount}
                setDebounceCount={setDebounceCount}
                wikiTimeoutSecs={wikiTimeoutSecs}
                setWikiTimeoutSecs={setWikiTimeoutSecs}
              />
            ),
          },
          {
            key: 'prompt',
            label: '提示词设置',
            children: (
              <PromptSettingsTab
                wikiPrompt={wikiPrompt}
                setWikiPrompt={setWikiPrompt}
                onRestorePrompt={handleRestorePrompt}
              />
            ),
          },
        ]}
      />
    </Modal>
  );
}

// ─── 设置弹窗子组件（避免 Tabs children 深层嵌套）─────────────────

interface DebounceSettingsTabProps {
  debounceSecs: number | null;
  setDebounceSecs: (v: number | null) => void;
  debounceCount: number | null;
  setDebounceCount: (v: number | null) => void;
  /** Wiki 执行超时（秒） */
  wikiTimeoutSecs: number | null;
  setWikiTimeoutSecs: (v: number | null) => void;
}

/** 防抖设置 Tab：防抖周期 + 触发条数 + Wiki 执行超时，受父组件状态控制 */
function DebounceSettingsTab({ debounceSecs, setDebounceSecs, debounceCount, setDebounceCount, wikiTimeoutSecs, setWikiTimeoutSecs }: DebounceSettingsTabProps) {
  return (
    <Form layout="vertical" style={{ marginTop: 16 }}>
      <Form.Item label="防抖周期">
        <InputNumber
          value={debounceSecs}
          // 用户清空输入时 value=null，不立即回填默认值，只透传 null 给 state；
          // 保存时由 handleSaveSettings 用 ?? 兜底，避免删值瞬间被 600 覆盖
          onChange={(v) => setDebounceSecs(v)}
          min={10}
          max={3600}
          addonAfter="秒"
          style={{ width: 200 }}
        />
      </Form.Item>
      <Form.Item label="触发条数">
        <InputNumber
          value={debounceCount}
          onChange={(v) => setDebounceCount(v)}
          min={1}
          max={100}
          addonAfter="条"
          style={{ width: 200 }}
        />
      </Form.Item>
      <Form.Item
        label="Wiki 执行超时"
        // 后端会把输入值钳制到 [60, 3600]，这里同步展示边界提示；
        // 默认 300 秒（5 分钟），慢模型可调大避免被强制超时
        extra="Wiki 自动维护与 Wiki 对话的最长执行时长（后端会自动钳制到 60–3600 秒），默认 300 秒"
      >
        <InputNumber
          value={wikiTimeoutSecs}
          onChange={(v) => setWikiTimeoutSecs(v)}
          min={60}
          max={3600}
          addonAfter="秒"
          style={{ width: 200 }}
        />
      </Form.Item>
      <Form.Item extra="达到条数阈值或周期到期时，统一处理 pending 的 todo，减少频繁的 LLM 调用" />
    </Form>
  );
}

interface PromptSettingsTabProps {
  wikiPrompt: string;
  setWikiPrompt: (v: string) => void;
  onRestorePrompt: () => void;
}

/** 提示词设置 Tab：单阶段 Wiki 提示词 */
function PromptSettingsTab({
  wikiPrompt, setWikiPrompt,
  onRestorePrompt,
}: PromptSettingsTabProps) {
  return (
    <div style={{ marginTop: 16 }}>
      <div style={{ marginBottom: 20 }}>
        <Space style={{ marginBottom: 8 }}>
          <Button onClick={onRestorePrompt}>恢复默认</Button>
          <span style={{ color: '#888', fontSize: 12 }}>
            Wiki 提示词（单阶段：分析记录 + 直接编辑文件）
          </span>
        </Space>
        <Input.TextArea
          value={wikiPrompt}
          onChange={(e) => setWikiPrompt(e.target.value)}
          rows={16}
          placeholder="留空使用内置默认，如需自定义请直接在此输入"
        />
      </div>
    </div>
  );
}
