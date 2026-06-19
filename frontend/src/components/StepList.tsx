// 环节管理页面（kind=expert）。
//
// 设计要点：
// - 与 TodoList 共享底层数据（todos.kind 列），但语义独立：这里只看 kind=expert 的子集。
// - 复用 TodoCard 之类的渲染组件不可行（TodoList 用的是整列平铺，跟纪念板卡片不同），
//   这里采用列表 + 操作按钮的紧凑布局，强调"被哪些 loop 引用"这一复用度指标。
// - 内联新建环节：点击 "+ 新建环节" 弹出 modal，要求 title + prompt + executor，
//   保存后立即出现在列表里。复用现有 createTodo 但后端默认 kind=item，
//   所以保存成功后立刻调用 promote 接口把 kind 改为 'expert'，避免双 round-trip 时的中间态问题。
// - 降级：仅当 used_by_loop_stage_count === 0 时允许 demote，且要求二次确认。

import { useEffect, useState, useCallback, useMemo } from 'react';
import {
  Card,
  Button,
  Empty,
  Skeleton,
  Input,
  Modal,
  Form,
  Select,
  Tooltip,
  App as AntApp,
  Popconfirm,
} from 'antd';
import {
  LeftOutlined,
  PlusOutlined,
  ExperimentOutlined,
  SearchOutlined,
  ThunderboltOutlined,
  ApartmentOutlined,
  DeleteOutlined,
} from '@ant-design/icons';
import * as db from '@/utils/database';
import * as dbSteps from '@/utils/database/steps';
import { formatRelativeTime } from '@/utils/datetime';
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
  // 复用 todo 列表里已有的执行器下拉选项
  const [executorOptions, setExecutorOptions] = useState<{ label: string; value: string }[]>([]);

  // 加载环节列表
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

  useEffect(() => {
    reload();
  }, [reload]);

  // 加载执行器选项（与 todo 创建表单共用）
  // 复用后端 executors 表的设计, 简单起见先用硬编码列表 + Select, 后续可以扩展为远程拉取
  useEffect(() => {
    // 与 TodoDrawer 中的执行器列表保持一致, 避免用户在两个页面看到不同选项
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

  // 客户端过滤（标题 + 提示词）
  const filtered = useMemo(() => {
    const kw = searchKeyword.trim().toLowerCase();
    if (!kw) return steps;
    return steps.filter(e => {
      const title = (e.title || '').toLowerCase();
      const prompt = (e.prompt || '').toLowerCase();
      return title.includes(kw) || prompt.includes(kw);
    });
  }, [steps, searchKeyword]);

  // 内联新建环节：先 createTodo（kind=item），再 promote，避免直接拼 SQL
  const handleCreate = useCallback(
    async (values: StepCreateForm) => {
      if (!values.title.trim()) {
        message.error('标题必填');
        return;
      }
      setCreating(true);
      try {
        // 1) 用现有 createTodo 创建, 后端默认 kind='item'
        const created: Todo = await db.createTodo(
          values.title.trim(),
          values.prompt.trim(),
          [], // 无标签
          [], // 无 hooks
          undefined,
          undefined,
        );
        // 2) 立刻 promote 为 expert。如果 promote 失败, 已经创建的孤儿 todo 留给用户手动清理。
        await dbSteps.promoteTodoToStep(created.id);
        message.success(`环节「${created.title}」已创建`);
        setCreateOpen(false);
        form.resetFields();
        reload();
      } catch (e) {
        // axios 拦截器已经弹过错误, 这里只负责关闭 loading
        // 失败时 modal 保持打开, 允许用户修改后重试
      } finally {
        setCreating(false);
      }
    },
    [form, message, reload],
  );

  // 降级：先看是否被 loop 引用, 再走 demote 接口
  const handleDemote = useCallback(
    async (expert: StepSummary) => {
      try {
        await dbSteps.demoteTodoToItem(expert.id);
        message.success(`「${expert.title}」已降级为事项`);
        reload();
      } catch {
        // axios 拦截器弹错
      }
    },
    [message, reload],
  );

  return (
    <div className="expert-list-page">
      <div className="expert-header">
        <div className="expert-header-top">
          {onBack && (
            <Button
              type="text"
              size="small"
              icon={<LeftOutlined />}
              onClick={onBack}
              aria-label="返回"
            >
              返回
            </Button>
          )}
          <h2 style={{ margin: 0, fontSize: 18 }}>
            <ExperimentOutlined style={{ marginRight: 8 }} />
            环节管理
          </h2>
          <div style={{ flex: 1 }} />
          <Button
            type="primary"
            icon={<PlusOutlined />}
            onClick={() => setCreateOpen(true)}
          >
            新建环节
          </Button>
        </div>
        <div className="expert-search-bar">
          <Input
            placeholder="搜索环节标题或提示词..."
            prefix={<SearchOutlined style={{ color: '#bfbfbf' }} />}
            value={searchKeyword}
            onChange={e => setSearchKeyword(e.target.value)}
            allowClear
            size="middle"
          />
        </div>
      </div>

      <div className="expert-list-content" style={{ padding: '16px' }}>
        {loading ? (
          <Skeleton active />
        ) : filtered.length === 0 ? (
          <Empty
            description={
              searchKeyword.trim()
                ? '没有匹配的环节'
                : '暂无环节；点击右上角「新建环节」或在 TodoList 把已有事项提升为环节'
            }
          />
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
            {filtered.map(expert => (
              <ExpertCard
                key={expert.id}
                expert={expert}
                onDemote={handleDemote}
              />
            ))}
          </div>
        )}
      </div>

      <Modal
        title="新建环节"
        open={createOpen}
        onCancel={() => {
          setCreateOpen(false);
          form.resetFields();
        }}
        onOk={() => form.submit()}
        confirmLoading={creating}
        okText="创建"
        cancelText="取消"
        destroyOnClose
      >
        <Form
          form={form}
          layout="vertical"
          onFinish={handleCreate}
          initialValues={{ executor: 'claudecode' }}
        >
          <Form.Item
            label="标题"
            name="title"
            rules={[{ required: true, message: '标题必填' }]}
          >
            <Input placeholder="例如：代码审查环节" maxLength={100} />
          </Form.Item>
          <Form.Item
            label="提示词 (Prompt)"
            name="prompt"
            tooltip="描述这个环节能做什么,会被作为 system/initial prompt 注入执行器"
          >
            <Input.TextArea
              rows={5}
              placeholder="例如：你是资深代码审查员,负责..."
              maxLength={4000}
            />
          </Form.Item>
          <Form.Item label="执行器" name="executor">
            <Select
              options={executorOptions}
              placeholder="选择执行器"
            />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  );
}

// 单个环节卡片
function ExpertCard({
  expert,
  onDemote,
}: {
  expert: StepSummary;
  onDemote: (e: StepSummary) => void;
}) {
  const canDemote = expert.used_by_loop_stage_count === 0;
  return (
    <Card
      size="small"
      hoverable
      styles={{ body: { padding: 16 } }}
      title={
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <span style={{ color: '#999', fontSize: 13 }}>#{expert.id}</span>
          <span style={{ fontWeight: 500 }}>{expert.title}</span>
          <Tooltip title="被多少个 loop stage 引用, 反映复用度">
            <span
              style={{
                display: 'inline-flex',
                alignItems: 'center',
                gap: 4,
                padding: '2px 8px',
                borderRadius: 10,
                background: expert.used_by_loop_stage_count > 0 ? '#722ed1' : '#f0f0f0',
                color: expert.used_by_loop_stage_count > 0 ? '#fff' : '#999',
                fontSize: 12,
              }}
            >
              <ApartmentOutlined />
              {expert.used_by_loop_stage_count}
            </span>
          </Tooltip>
        </div>
      }
      extra={
        <div style={{ display: 'flex', gap: 8 }}>
          {expert.executor && (
            <Tooltip title={`执行器: ${expert.executor}`}>
              <ThunderboltOutlined style={{ color: '#fa8c16' }} />
              <span style={{ marginLeft: 4, fontSize: 12 }}>{expert.executor}</span>
            </Tooltip>
          )}
          <Popconfirm
            title="降级为事项"
            description={
              canDemote
                ? '确定将此环节降级为事项?降级后不会被任何 loop 引用'
                : '该环节正被 loop 引用,无法降级'
            }
            okButtonProps={{ disabled: !canDemote }}
            onConfirm={() => onDemote(expert)}
          >
            <Button
              type="text"
              size="small"
              danger
              icon={<DeleteOutlined />}
              disabled={!canDemote}
              aria-label="降级为事项"
            >
              降级为事项
            </Button>
          </Popconfirm>
        </div>
      }
    >
      <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
        {expert.prompt && (
          <div
            style={{
              fontSize: 13,
              color: 'var(--color-text-secondary, #666)',
              background: 'var(--color-bg-secondary, #fafafa)',
              padding: 8,
              borderRadius: 4,
              maxHeight: 80,
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              display: '-webkit-box',
              WebkitLineClamp: 3,
              WebkitBoxOrient: 'vertical',
            }}
          >
            {expert.prompt}
          </div>
        )}
        <div style={{ fontSize: 12, color: '#999' }}>
          更新于 {formatRelativeTime(expert.updated_at)}
        </div>
      </div>
    </Card>
  );
}