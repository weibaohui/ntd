// 环节管理页面：左栏列表 + 右栏详情，与 Loop Studio 布局一致。

import { useEffect, useState, useCallback, useMemo } from 'react';
import {
  Button, Empty, Skeleton, Input, Modal, Form, Select, Tooltip, App as AntApp,
} from 'antd';
import {
  LeftOutlined, PlusOutlined, ExperimentOutlined, SearchOutlined,
  ThunderboltOutlined, ApartmentOutlined,
} from '@ant-design/icons';
import * as db from '@/utils/database';
import * as dbSteps from '@/utils/database/steps';
import { formatRelativeTime } from '@/utils/datetime';
import { StepDetailPanel } from './StepDetailPanel';
import type { StepSummary, Todo } from '@/types';

interface StepListProps {
  onBack?: () => void;
}

interface StepCreateForm {
  title: string;
  prompt: string;
  executor?: string;
}

export function StepList({ onBack }: StepListProps) {
  const { message } = AntApp.useApp();
  const [steps, setSteps] = useState<StepSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [searchKeyword, setSearchKeyword] = useState('');
  const [createOpen, setCreateOpen] = useState(false);
  const [form] = Form.useForm<StepCreateForm>();
  const [creating, setCreating] = useState(false);
  // 当前选中的环节 id（右侧展示详情）
  const [selectedStepId, setSelectedStepId] = useState<number | null>(null);
  const [executorOptions, setExecutorOptions] = useState<{ label: string; value: string }[]>([]);

  const reload = useCallback(() => {
    setLoading(true);
    dbSteps
      .listSteps()
      .then(setSteps)
      .catch(() => {
        message.error('加载环节列表失败');
        setSteps([]);
      })
      .finally(() => setLoading(false));
  }, [message]);

  useEffect(() => { reload(); }, [reload]);

  useEffect(() => {
    setExecutorOptions([
      { label: 'claudecode', value: 'claudecode' },
      { label: 'codebuddy', value: 'codebuddy' },
      { label: 'opencode', value: 'opencode' },
      { label: 'atomcode', value: 'atomcode' },
      { label: 'hermes', value: 'hermes' },
      { label: 'kimi', value: 'kimi' },
      { label: 'codex', value: 'codex' },
      { label: 'codewhale', value: 'codewhale' },
      { label: 'pi', value: 'pi' },
      { label: 'mimo', value: 'mimo' },
      { label: 'zhanlu', value: 'zhanlu' },
    ]);
  }, []);

  // 默认选中第一个（如果有）
  useEffect(() => {
    if (!loading && steps.length > 0 && selectedStepId === null) {
      setSelectedStepId(steps[0].id);
    }
  }, [loading, steps, selectedStepId]);

  // 客户端过滤
  const filtered = useMemo(() => {
    const kw = searchKeyword.trim().toLowerCase();
    if (!kw) return steps;
    return steps.filter(e => {
      const title = (e.title || '').toLowerCase();
      const prompt = (e.prompt || '').toLowerCase();
      return title.includes(kw) || prompt.includes(kw);
    });
  }, [steps, searchKeyword]);

  // 新建环节
  const handleCreate = useCallback(async (values: StepCreateForm) => {
    if (!values.title.trim()) { message.error('标题必填'); return; }
    setCreating(true);
    try {
      const created: Todo = await db.createTodo(
        values.title.trim(), values.prompt.trim(), [], [], undefined, undefined,
      );
      await dbSteps.promoteTodoToStep(created.id);
      message.success(`环节「${created.title}」已创建`);
      setCreateOpen(false);
      form.resetFields();
      reload();
    } catch {
      // axios 拦截器已弹错
    } finally {
      setCreating(false);
    }
  }, [form, message, reload]);

  return (
    <div style={{ display: 'flex', height: '100%', overflow: 'hidden' }}>
      {/* 左栏：环节列表 */}
      <div className="step-list-col" style={{
        width: 300, minWidth: 260, flexShrink: 0,
        display: 'flex', flexDirection: 'column',
        borderRight: '1px solid var(--color-border, #e2e8f0)',
        height: '100%', overflow: 'hidden',
      }}>
        {/* 头部 */}
        <div style={{ padding: '12px 16px', borderBottom: '1px solid var(--color-border, #e2e8f0)', flexShrink: 0 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
            {onBack && (
              <Button type="text" size="small" icon={<LeftOutlined />} onClick={onBack} aria-label="返回" />
            )}
            <h2 style={{ margin: 0, fontSize: 16, flex: 1 }}>
              <ExperimentOutlined style={{ marginRight: 6 }} />环节
            </h2>
            <Button type="primary" size="small" icon={<PlusOutlined />} onClick={() => setCreateOpen(true)}>
              新建
            </Button>
          </div>
          <Input
            placeholder="搜索环节..."
            prefix={<SearchOutlined style={{ color: '#bfbfbf' }} />}
            value={searchKeyword}
            onChange={e => setSearchKeyword(e.target.value)}
            allowClear
            size="small"
          />
        </div>

        {/* 列表 */}
        <div style={{ flex: 1, overflow: 'auto', padding: 8 }}>
          {loading ? (
            <Skeleton active style={{ padding: 12 }} />
          ) : filtered.length === 0 ? (
            <Empty
              image={Empty.PRESENTED_IMAGE_SIMPLE}
              description={searchKeyword.trim() ? '没有匹配的环节' : '暂无环节'}
              style={{ marginTop: 32 }}
            />
          ) : (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
              {filtered.map(step => (
                <div
                  key={step.id}
                  onClick={() => setSelectedStepId(step.id)}
                  role="button"
                  tabIndex={0}
                  onKeyDown={(e) => { if (e.key === 'Enter') setSelectedStepId(step.id); }}
                  style={{
                    display: 'flex', alignItems: 'center', gap: 8,
                    padding: '10px 12px', borderRadius: 8, cursor: 'pointer',
                    background: selectedStepId === step.id
                      ? 'var(--color-primary-bg, #f0f9ff)'
                      : 'transparent',
                    border: selectedStepId === step.id
                      ? '1px solid var(--color-primary, #0891b2)'
                      : '1px solid transparent',
                    transition: 'background 200ms, border-color 200ms',
                  }}
                  onMouseEnter={(e) => {
                    if (selectedStepId !== step.id) {
                      e.currentTarget.style.background = 'var(--color-bg-hover, #f1f5f9)';
                    }
                  }}
                  onMouseLeave={(e) => {
                    if (selectedStepId !== step.id) {
                      e.currentTarget.style.background = 'transparent';
                    }
                  }}
                >
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{
                      fontWeight: 500, fontSize: 13, color: 'var(--color-text, #0f172a)',
                      overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
                    }}>
                      {step.title}
                    </div>
                    <div style={{
                      fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)', marginTop: 2,
                    }}>
                      <ApartmentOutlined style={{ marginRight: 4 }} />
                      {step.used_by_loop_step_count} 引用 · 更新于 {formatRelativeTime(step.updated_at)}
                    </div>
                  </div>
                  {step.executor && (
                    <Tooltip title={step.executor}>
                      <span style={{
                        fontSize: 10, padding: '1px 6px', borderRadius: 4,
                        background: 'var(--color-bg-hover, #f1f5f9)',
                        color: 'var(--color-text-tertiary, #94a3b8)',
                        whiteSpace: 'nowrap', flexShrink: 0,
                      }}>
                        <ThunderboltOutlined /> {step.executor}
                      </span>
                    </Tooltip>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      {/* 右栏：环节详情 */}
      <div style={{ flex: 1, overflow: 'auto' }}>
        {selectedStepId !== null ? (
          <StepDetailPanel stepId={selectedStepId} />
        ) : (
          <Empty description="请选择一个环节" style={{ marginTop: 64 }} />
        )}
      </div>

      {/* 新建 Modal */}
      <Modal
        title="新建环节"
        open={createOpen}
        onCancel={() => { setCreateOpen(false); form.resetFields(); }}
        onOk={() => form.submit()}
        confirmLoading={creating}
        okText="创建"
        cancelText="取消"
        destroyOnClose
      >
        <Form form={form} layout="vertical" onFinish={handleCreate} initialValues={{ executor: 'claudecode' }}>
          <Form.Item label="标题" name="title" rules={[{ required: true, message: '标题必填' }]}>
            <Input placeholder="例如：代码审查环节" maxLength={100} />
          </Form.Item>
          <Form.Item label="提示词 (Prompt)" name="prompt" tooltip="描述这个环节能做什么">
            <Input.TextArea rows={5} placeholder="例如：你是资深代码审查员,负责..." maxLength={4000} />
          </Form.Item>
          <Form.Item label="执行器" name="executor">
            <Select options={executorOptions} placeholder="选择执行器" />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  );
}
